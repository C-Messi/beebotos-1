//! 会话管理模块
//!
//! 管理多轮对话上下文，支持跨消息的记忆和状态保持

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

use crate::communication::PlatformType;
use crate::error::{AgentError, Result};

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 会话 ID
    pub id: String,
    /// 平台类型
    pub platform: PlatformType,
    /// 频道/聊天 ID
    pub channel_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 最后活动时间
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// 消息历史
    pub messages: Vec<SessionMessage>,
    /// 会话元数据
    pub metadata: HashMap<String, String>,
}

/// 会话中的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    /// 消息 ID
    pub id: String,
    /// 角色 (user/assistant/system)
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 是否有图片
    pub has_image: bool,
    /// 图片 URLs
    pub image_urls: Vec<String>,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 会话处理锁 Guard
///
/// 在获取时将会话 ID 加入 processing 集合，在 Drop 时自动移除。
pub struct ProcessingGuard {
    processing: Arc<Mutex<HashSet<String>>>,
    session_id: String,
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        let processing = self.processing.clone();
        let session_id = self.session_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut guard = processing.lock().await;
                guard.remove(&session_id);
            });
        }
    }
}

/// 会话管理器
pub struct SessionManager {
    /// 会话存储
    sessions: RwLock<HashMap<String, Session>>,
    /// 用户当前会话映射 (user_id -> session_id)
    user_sessions: RwLock<HashMap<String, String>>,
    /// 正在处理消息的会话 ID 集合
    processing: Arc<Mutex<HashSet<String>>>,
    /// 会话超时时间（秒）
    timeout_seconds: u64,
    /// 最大消息历史数
    max_history: usize,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(timeout_seconds: u64, max_history: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            user_sessions: RwLock::new(HashMap::new()),
            processing: Arc::new(Mutex::new(HashSet::new())),
            timeout_seconds,
            max_history,
        }
    }

    /// 获取或创建会话
    pub async fn get_or_create_session(
        &self,
        platform: PlatformType,
        channel_id: &str,
        user_id: &str,
    ) -> Result<Session> {
        let user_key = format!("{}:{}:{}", platform, channel_id, user_id);

        // 检查用户是否有活跃会话
        let session_id = {
            let user_sessions = self.user_sessions.read().await;
            user_sessions.get(&user_key).cloned()
        };

        if let Some(ref sid) = session_id {
            // 检查会话是否有效
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(sid) {
                let elapsed = chrono::Utc::now().signed_duration_since(session.last_activity);
                if elapsed.num_seconds() < self.timeout_seconds as i64 {
                    debug!("找到活跃会话: {}", sid);
                    return Ok(session.clone());
                }
            }
        }

        // 创建新会话
        self.create_session(platform, channel_id, user_id).await
    }

    /// 创建新会话
    async fn create_session(
        &self,
        platform: PlatformType,
        channel_id: &str,
        user_id: &str,
    ) -> Result<Session> {
        let session_id = Uuid::new_v4().to_string();
        let user_key = format!("{}:{}:{}", platform, channel_id, user_id);

        let session = Session {
            id: session_id.clone(),
            platform,
            channel_id: channel_id.to_string(),
            user_id: user_id.to_string(),
            created_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            messages: Vec::new(),
            metadata: HashMap::new(),
        };

        // 保存会话
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session.clone());
        }

        // 更新用户会话映射
        {
            let mut user_sessions = self.user_sessions.write().await;
            user_sessions.insert(user_key, session_id.clone());
        }

        info!("🆕 创建新会话: {} (用户: {})", session_id, user_id);
        Ok(session)
    }

    /// 尝试获取会话的处理锁
    ///
    /// 如果该会话当前没有正在处理的消息，则获取锁并返回 guard；
    /// 如果已经有消息在处理中，则返回 None。
    /// guard 会在 Drop 时自动释放锁。
    pub async fn try_start_processing(&self, session_id: &str) -> Option<ProcessingGuard> {
        let mut processing = self.processing.lock().await;
        if processing.contains(session_id) {
            None
        } else {
            processing.insert(session_id.to_string());
            Some(ProcessingGuard {
                processing: self.processing.clone(),
                session_id: session_id.to_string(),
            })
        }
    }

    /// 添加消息到会话
    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        has_image: bool,
        image_urls: Vec<String>,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            // 添加新消息
            let msg = SessionMessage {
                id: Uuid::new_v4().to_string(),
                role: role.to_string(),
                content: content.to_string(),
                has_image,
                image_urls,
                timestamp: chrono::Utc::now(),
            };

            session.messages.push(msg);
            session.last_activity = chrono::Utc::now();

            // 限制历史消息数
            if session.messages.len() > self.max_history {
                let to_remove = session.messages.len() - self.max_history;
                session.messages.drain(0..to_remove);
            }

            debug!(
                "会话 {} 添加消息，当前历史: {} 条",
                session_id,
                session.messages.len()
            );
            Ok(())
        } else {
            Err(AgentError::not_found(format!("会话不存在: {}", session_id)))
        }
    }

    /// 获取会话历史（用于 LLM 上下文）
    pub async fn get_history_for_llm(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionMessage>> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(session_id) {
            let start = session.messages.len().saturating_sub(limit);
            Ok(session.messages[start..].to_vec())
        } else {
            Err(AgentError::not_found(format!("会话不存在: {}", session_id)))
        }
    }

    /// 获取指定平台/频道/用户对应的活跃会话历史
    pub async fn get_history_for_key(
        &self,
        platform: PlatformType,
        channel_id: &str,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionMessage>> {
        let user_key = format!("{}:{}:{}", platform, channel_id, user_id);
        let session_id = {
            let user_sessions = self.user_sessions.read().await;
            user_sessions.get(&user_key).cloned()
        }
        .ok_or_else(|| AgentError::not_found(format!("会话不存在: {}", user_key)))?;

        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            let elapsed = chrono::Utc::now().signed_duration_since(session.last_activity);
            if elapsed.num_seconds() < self.timeout_seconds as i64 {
                let start = session.messages.len().saturating_sub(limit);
                return Ok(session.messages[start..].to_vec());
            }
        }

        Err(AgentError::not_found(format!("会话不存在: {}", user_key)))
    }

    /// 更新会话元数据
    pub async fn update_metadata(&self, session_id: &str, key: &str, value: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.metadata.insert(key.to_string(), value.to_string());
            session.last_activity = chrono::Utc::now();
            Ok(())
        } else {
            Err(AgentError::not_found(format!("会话不存在: {}", session_id)))
        }
    }

    /// 结束会话
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let mut user_sessions = self.user_sessions.write().await;

        if let Some(session) = sessions.get(session_id) {
            let user_key = format!(
                "{}:{}:{}",
                session.platform, session.channel_id, session.user_id
            );
            user_sessions.remove(&user_key);
            sessions.remove(session_id);
            info!("🔚 结束会话: {}", session_id);
            Ok(())
        } else {
            Err(AgentError::not_found(format!("会话不存在: {}", session_id)))
        }
    }

    /// 清理过期会话
    pub async fn cleanup_expired(&self) -> usize {
        let now = chrono::Utc::now();
        let mut sessions = self.sessions.write().await;
        let mut user_sessions = self.user_sessions.write().await;

        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| {
                now.signed_duration_since(session.last_activity)
                    .num_seconds()
                    > self.timeout_seconds as i64
            })
            .map(|(id, _)| id.clone())
            .collect();

        for session_id in &expired {
            if let Some(session) = sessions.get(session_id) {
                let user_key = format!(
                    "{}:{}:{}",
                    session.platform, session.channel_id, session.user_id
                );
                user_sessions.remove(&user_key);
            }
            sessions.remove(session_id);
        }

        if !expired.is_empty() {
            info!("🧹 清理 {} 个过期会话", expired.len());
        }

        expired.len()
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> SessionStats {
        let sessions = self.sessions.read().await;
        let total_sessions = sessions.len();
        let total_messages: usize = sessions.values().map(|s| s.messages.len()).sum();

        SessionStats {
            total_sessions,
            total_messages,
            active_users: sessions
                .values()
                .map(|s| &s.user_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }
    }
}

/// 会话统计
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub total_messages: usize,
    pub active_users: usize,
}

/// 便捷函数：创建默认配置的会话管理器
impl SessionManager {
    /// 创建默认会话管理器（30分钟超时，100条历史）
    pub fn default() -> Arc<Self> {
        Arc::new(Self::new(1800, 100))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager() {
        let manager = SessionManager::new(60, 10);

        // 创建会话
        let session = manager
            .get_or_create_session(PlatformType::Lark, "chat123", "user456")
            .await
            .unwrap();

        assert_eq!(session.platform, PlatformType::Lark);
        assert_eq!(session.channel_id, "chat123");
        assert_eq!(session.user_id, "user456");

        // 添加消息
        manager
            .add_message(&session.id, "user", "Hello", false, vec![])
            .await
            .unwrap();
        manager
            .add_message(&session.id, "assistant", "Hi there", false, vec![])
            .await
            .unwrap();

        // 获取历史
        let history = manager.get_history_for_llm(&session.id, 10).await.unwrap();
        assert_eq!(history.len(), 2);

        // 获取同一会话
        let session2 = manager
            .get_or_create_session(PlatformType::Lark, "chat123", "user456")
            .await
            .unwrap();
        assert_eq!(session.id, session2.id);

        // 结束会话
        manager.end_session(&session.id).await.unwrap();
    }
}
