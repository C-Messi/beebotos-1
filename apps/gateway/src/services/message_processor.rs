//! 消息处理器
//!
//! 集成消息去重、会话管理、多模态处理、Memory 协同和持久化

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use beebotos_agents::communication::channel::session_manager::{SessionManager, SessionMessage};
use beebotos_agents::communication::channel::ChannelEvent;
use beebotos_agents::communication::{Message, MessageType, PlatformType};
use beebotos_agents::deduplicator::MessageDeduplicator;
use beebotos_agents::llm::Message as LLMMessage;
use beebotos_agents::media::multimodal::MultimodalProcessor;
use beebotos_agents::memory::{MarkdownMemoryEntry, MemoryFileType};
use beebotos_agents::skills::unified_react_executor::STREAM_EVENT_PREFIX;
use beebotos_agents::ChannelRegistry;
use regex::Regex;
use serde::Deserialize;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::clients::ClawHubClient;
use crate::error::GatewayError;
use crate::services::agent_resolver::AgentResolver;
use crate::services::llm_service::LlmService;
use crate::services::react_trace_ws::ToolCallTraceStore;
use crate::services::webchat_service::WebchatService;

struct WebChatWorkflowProgressReporter {
    channel_registry: Arc<ChannelRegistry>,
    channel_id: String,
    workflow_id: String,
    emitted_steps: Mutex<HashSet<String>>,
}

impl WebChatWorkflowProgressReporter {
    fn new(
        channel_registry: Arc<ChannelRegistry>,
        channel_id: impl Into<String>,
        workflow_id: impl Into<String>,
    ) -> Self {
        Self {
            channel_registry,
            channel_id: channel_id.into(),
            workflow_id: workflow_id.into(),
            emitted_steps: Mutex::new(HashSet::new()),
        }
    }

    async fn send_event(&self, event: serde_json::Value) {
        let Some(channel) = self
            .channel_registry
            .get_channel_by_platform(PlatformType::WebChat)
            .await
        else {
            warn!("WebChat workflow progress skipped: channel unavailable");
            return;
        };

        let guard = channel.read().await;
        let Some(webchat) = guard
            .as_any()
            .downcast_ref::<beebotos_agents::communication::channel::WebChatChannel>()
        else {
            warn!("WebChat workflow progress skipped: channel type mismatch");
            return;
        };

        if let Err(e) = webchat.send_tool_call(&self.channel_id, event).await {
            warn!("Failed to send WebChat workflow progress: {}", e);
        }
    }
}

#[async_trait::async_trait]
impl beebotos_agents::workflow::StepProgressReporter for WebChatWorkflowProgressReporter {
    async fn on_step_complete(&self, instance: &beebotos_agents::workflow::WorkflowInstance) {
        let mut pending_events = Vec::new();

        {
            let Ok(mut emitted) = self.emitted_steps.lock() else {
                warn!("WebChat workflow progress skipped: emitted-step lock poisoned");
                return;
            };
            for (step_id, step_state) in &instance.step_states {
                if !step_state.status.is_terminal() || emitted.contains(step_id) {
                    continue;
                }
                emitted.insert(step_id.clone());
                pending_events.push(workflow_step_tool_call_event(
                    &self.workflow_id,
                    &instance.id,
                    emitted.len(),
                    step_id,
                    step_state,
                ));
            }
        }

        for event in pending_events {
            self.send_event(event).await;
        }
    }
}

fn workflow_output_preview(output: &Option<serde_json::Value>, max_chars: usize) -> String {
    let Some(output) = output else {
        return String::new();
    };
    let text = match output {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        _ => serde_json::to_string(output).unwrap_or_default(),
    };
    text.chars().take(max_chars).collect()
}

fn workflow_step_tool_call_event(
    workflow_id: &str,
    instance_id: &str,
    round: usize,
    step_id: &str,
    step_state: &beebotos_agents::workflow::StepState,
) -> serde_json::Value {
    let output_preview = workflow_output_preview(&step_state.output, 280);
    let mut reasoning = format!(
        "Workflow '{}' step '{}' {} in {}s",
        workflow_id,
        step_id,
        step_state.status,
        step_state.duration_secs()
    );
    if let Some(error) = &step_state.error {
        reasoning.push_str(&format!("; error: {}", error));
    } else if !output_preview.is_empty() {
        reasoning.push_str(&format!("; output: {}", output_preview));
    }

    serde_json::json!({
        "id": format!("workflow-{}-{}", instance_id, step_id),
        "round": round,
        "tool_name": step_id,
        "reasoning": reasoning,
        "arguments": {
            "workflow_id": workflow_id,
            "instance_id": instance_id,
            "step_id": step_id,
            "status": step_state.status.to_string(),
            "duration_secs": step_state.duration_secs(),
            "execution_time_ms": step_state.execution_time_ms,
            "retry_count": step_state.retry_count,
            "output_preview": output_preview,
            "error": step_state.error,
        },
        "status": step_state.status.to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
}

fn workflow_step_tool_call_events(
    workflow_id: &str,
    instance: &beebotos_agents::workflow::WorkflowInstance,
) -> Vec<serde_json::Value> {
    instance
        .step_states
        .iter()
        .enumerate()
        .map(|(idx, (step_id, step_state))| {
            workflow_step_tool_call_event(workflow_id, &instance.id, idx + 1, step_id, step_state)
        })
        .collect()
}

struct WorkflowChatResult {
    text: String,
    tool_calls: Vec<serde_json::Value>,
}

/// 消息处理器
pub struct MessageProcessor {
    /// 去重器
    deduplicator: Arc<MessageDeduplicator>,
    /// 会话管理器
    session_manager: Arc<SessionManager>,
    /// 多模态处理器
    multimodal_processor: MultimodalProcessor,
    /// LLM 服务
    llm_service: Arc<LlmService>,
    /// 频道注册表
    channel_registry: Arc<ChannelRegistry>,
    /// Memory 系统
    memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
    /// Webchat 持久化服务
    webchat_service: Option<Arc<WebchatService>>,
    /// Skill 注册表
    skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    /// MCP manager for workflow step agents that need external tools
    mcp_manager: Option<Arc<beebotos_agents::mcp::MCPManager>>,
    /// Workflow 注册表
    workflow_registry:
        Option<Arc<tokio::sync::RwLock<beebotos_agents::workflow::WorkflowRegistry>>>,
    /// ClawHub 客户端（技能市场）
    clawhub_client: Option<ClawHubClient>,
    tool_call_trace_store: Option<Arc<ToolCallTraceStore>>,
}

impl MessageProcessor {
    /// 创建新的消息处理器
    pub fn new(
        llm_service: Arc<LlmService>,
        channel_registry: Arc<ChannelRegistry>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        webchat_service: Option<Arc<WebchatService>>,
        skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
        mcp_manager: Option<Arc<beebotos_agents::mcp::MCPManager>>,
        workflow_registry: Option<
            Arc<tokio::sync::RwLock<beebotos_agents::workflow::WorkflowRegistry>>,
        >,
        clawhub_client: Option<ClawHubClient>,
        tool_call_trace_store: Option<Arc<ToolCallTraceStore>>,
    ) -> Self {
        Self {
            deduplicator: Arc::new(MessageDeduplicator::default()),
            session_manager: SessionManager::default(),
            multimodal_processor: MultimodalProcessor::new(),
            llm_service,
            channel_registry,
            memory_system,
            webchat_service,
            skill_registry,
            mcp_manager,
            workflow_registry,
            clawhub_client,
            tool_call_trace_store,
        }
    }

    /// Expose the in-memory session manager for internal consumers like cron
    /// result reconciliation.
    pub fn session_manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.session_manager)
    }

    fn is_stop_command(content: &str) -> bool {
        content
            .split_whitespace()
            .next()
            .map(|cmd| cmd.eq_ignore_ascii_case("/stop"))
            .unwrap_or(false)
    }

    async fn persist_webchat_user_message(
        &self,
        db_session_id: &str,
        platform: PlatformType,
        channel_id: &str,
        user_id: &str,
        content: &str,
        command: Option<&str>,
    ) {
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    db_session_id,
                    "user",
                    content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "channel_id": channel_id,
                        "command": command,
                    })),
                    None,
                )
                .await;
        }
    }

    async fn send_workflow_result(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        db_session_id: &str,
        result: &WorkflowChatResult,
    ) -> Result<(), GatewayError> {
        let mut saved_message_id: Option<String> = None;
        if let Some(ref svc) = self.webchat_service {
            if let Ok(id) = svc
                .save_message(
                    db_session_id,
                    "assistant",
                    &result.text,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "channel_id": channel_id,
                        "workflow": true,
                        "tool_calls": result.tool_calls,
                    })),
                    None,
                )
                .await
            {
                saved_message_id = Some(id);
            }
        }

        if platform == PlatformType::WebChat {
            let mut metadata = HashMap::new();
            if !result.tool_calls.is_empty() {
                if let Ok(tool_calls_json) = serde_json::to_string(&result.tool_calls) {
                    metadata.insert("tool_calls".to_string(), tool_calls_json);
                }
            }

            let reply = Message {
                id: saved_message_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .unwrap_or_else(Uuid::new_v4),
                thread_id: original.thread_id,
                platform,
                message_type: MessageType::Text,
                content: result.text.clone(),
                metadata,
                timestamp: chrono::Utc::now(),
            };

            if let Some(channel) = self.channel_registry.get_channel_by_platform(platform).await {
                channel
                    .read()
                    .await
                    .send(channel_id, &reply)
                    .await
                    .map_err(|e| GatewayError::Internal {
                        message: format!("Failed to send workflow reply: {}", e),
                        correlation_id: Uuid::new_v4().to_string(),
                    })?;
            }
        } else {
            self.send_reply(platform, channel_id, original, &result.text)
                .await?;
        }

        if let (Some(ref svc), Some(ref msg_id)) =
            (self.webchat_service.as_ref(), saved_message_id.as_ref())
        {
            let _ = svc.mark_ws_delivered(msg_id).await;
        }

        Ok(())
    }

    async fn handle_stop_command(
        &self,
        platform: PlatformType,
        channel_id: &str,
        message: &Message,
        session_id: &str,
        db_session_id: &str,
        user_id: &str,
        content: &str,
    ) -> Result<(), GatewayError> {
        let cancelled_db = beebotos_agents::session_cancellation::cancel(db_session_id).await;
        let cancelled_session = if db_session_id == session_id {
            false
        } else {
            beebotos_agents::session_cancellation::cancel(session_id).await
        };
        let cancelled = cancelled_db || cancelled_session;
        let response = if cancelled {
            "⏹️ 已停止当前任务。"
        } else {
            "ℹ️ 当前没有正在运行的任务。"
        };

        let _ = self
            .session_manager
            .add_message(session_id, "user", content, false, vec![])
            .await;
        let _ = self
            .session_manager
            .add_message(session_id, "assistant", response, false, vec![])
            .await;

        let mut saved_message_id: Option<String> = None;
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    db_session_id,
                    "user",
                    content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "channel_id": channel_id,
                        "command": "stop",
                    })),
                    None,
                )
                .await;
            saved_message_id = svc
                .save_message(
                    db_session_id,
                    "assistant",
                    response,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "channel_id": channel_id,
                        "cancelled": cancelled,
                    })),
                    None,
                )
                .await
                .ok();
        }

        if self
            .send_reply(platform, channel_id, message, response)
            .await
            .is_ok()
        {
            if let (Some(ref svc), Some(id)) = (self.webchat_service.as_ref(), saved_message_id) {
                let _ = svc.mark_ws_delivered(&id).await;
            }
        }

        Ok(())
    }

    /// 处理频道事件
    pub async fn process_event(&self, event: ChannelEvent) -> Result<(), GatewayError> {
        match event {
            ChannelEvent::MessageReceived {
                platform,
                channel_id,
                message,
            } => self.handle_message(platform, &channel_id, message).await,
            _ => {
                debug!("Unhandled channel event: {:?}", event);
                Ok(())
            }
        }
    }

    /// 处理消息
    async fn handle_message(
        &self,
        platform: PlatformType,
        channel_id: &str,
        message: Message,
    ) -> Result<(), GatewayError> {
        // 1. 消息去重检查
        if let Some(msg_id) = message.metadata.get("message_id") {
            if !self
                .deduplicator
                .should_process_key(&platform.to_string(), msg_id)
                .await
            {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message
            .metadata
            .get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self
            .session_manager
            .get_or_create_session(platform, channel_id, &user_id)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to create session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        info!("💬 会话 {} - 用户 {} 发送消息", session.id, user_id);

        // 2.5 统一获取/创建 DB session
        let db_session_id = if let Some(ref svc) = self.webchat_service {
            if platform == PlatformType::WebChat {
                // WebChat: 验证前端提供的 session_id，无效则自动创建
                let provided_sid = message
                    .metadata
                    .get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    Ok(false) => {
                        return Err(GatewayError::not_found("Session", &provided_sid));
                    }
                    Err(e) => {
                        return Err(GatewayError::Internal {
                            message: format!("Failed to validate webchat session: {}", e),
                            correlation_id: Uuid::new_v4().to_string(),
                        });
                    }
                }
            } else {
                // 外部渠道：按 user_id + channel 查找或创建
                let sender_id = message
                    .metadata
                    .get("sender_id")
                    .cloned()
                    .unwrap_or_else(|| channel_id.to_string());
                match svc
                    .get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id)
                    .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        warn!("Failed to get/create channel session: {}", e);
                        session.id.clone()
                    }
                }
            }
        } else {
            session.id.clone()
        };

        // 3. 处理多模态内容（下载图片等）
        let (content, images) = self.process_multimodal(&message).await?;

        if Self::is_stop_command(&content) {
            return self
                .handle_stop_command(
                    platform,
                    channel_id,
                    &message,
                    &session.id,
                    &db_session_id,
                    &user_id,
                    &content,
                )
                .await;
        }

        // 2.1 会话并发保护：同一 session 同时只能处理一条消息。
        // /stop is handled before this guard so users can interrupt an active task.
        let _processing_guard = self.session_manager.try_start_processing(&session.id).await;
        if _processing_guard.is_none() {
            info!("⏳ 会话 {} 正在处理中，跳过新消息", session.id);
            return Ok(());
        }

        // 🟢 P1 FIX: Check for /workflow command trigger
        if let Some(workflow_result) = self
            .try_execute_workflow_command(&content, platform, channel_id)
            .await
        {
            match workflow_result {
                Ok(result) => {
                    // Add workflow command to history
                    self.session_manager
                        .add_message(&session.id, "user", &content, false, vec![])
                        .await
                        .ok();
                    // Add workflow result as assistant response
                    self.session_manager
                        .add_message(&session.id, "assistant", &result.text, false, vec![])
                        .await
                        .ok();
                    self.persist_webchat_user_message(
                        &db_session_id,
                        platform,
                        channel_id,
                        &user_id,
                        &content,
                        Some("workflow"),
                    )
                    .await;
                    self.send_workflow_result(
                        platform,
                        channel_id,
                        &message,
                        &db_session_id,
                        &result,
                    )
                    .await?;
                    return Ok(());
                }
                Err(e) => {
                    let error_msg = format!("Workflow execution error: {}", e);
                    self.send_reply(platform, channel_id, &message, &error_msg)
                        .await?;
                    return Ok(());
                }
            }
        }

        // 🟢 P1 FIX: Try natural-language workflow matching
        if let Some(workflow_result) = self.try_match_workflow_by_content(&content).await {
            match workflow_result {
                Ok(result_text) => {
                    self.session_manager
                        .add_message(&session.id, "user", &content, false, vec![])
                        .await
                        .ok();
                    self.session_manager
                        .add_message(&session.id, "assistant", &result_text, false, vec![])
                        .await
                        .ok();
                    let msg_id = if let Some(ref svc) = self.webchat_service {
                        svc.save_message(&db_session_id, "assistant", &result_text, Some(serde_json::json!({"platform": platform.to_string(), "channel_id": channel_id})), None).await.ok()
                    } else {
                        None
                    };
                    if self
                        .send_reply(platform, channel_id, &message, &result_text)
                        .await
                        .is_ok()
                    {
                        if let (Some(ref svc), Some(id)) = (self.webchat_service.as_ref(), msg_id) {
                            let _ = svc.mark_ws_delivered(&id).await;
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    let error_msg = format!("Workflow execution error: {}", e);
                    self.send_reply(platform, channel_id, &message, &error_msg)
                        .await?;
                    return Ok(());
                }
            }
        }

        // 4. 添加用户消息到会话历史
        let image_urls: Vec<String> = images
            .iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(
                &session.id,
                "user",
                &content,
                !images.is_empty(),
                image_urls,
            )
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    &db_session_id,
                    "user",
                    &content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "has_image": !images.is_empty(),
                        "channel_id": channel_id,
                    })),
                    None,
                )
                .await;
        }

        // 5. 构建 LLM 上下文（包含历史消息）
        // 🆕 FIX: Limit history to 6 turns and truncate long messages.
        let history = self
            .session_manager
            .get_history_for_llm(&session.id, 6)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;
        let history: Vec<_> = history
            .into_iter()
            .map(|mut m| {
                if m.content.chars().count() > 300 {
                    m.content = m.content.chars().take(300).collect::<String>() + "...";
                }
                m
            })
            .collect();

        // 5.5 Memory 检索
        let (memory_context, _direct_answer) = self.build_memory_context(&content, &None).await;

        // 6. 调用 LLM（注入记忆上下文）
        let llm_response = self
            .call_llm_with_context(&message, &history, &images, &memory_context)
            .await?;

        // 7. 添加助手回复到会话历史
        self.session_manager
            .add_message(&session.id, "assistant", &llm_response, false, vec![])
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add assistant message: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 7.5 持久化 AI 回复
        let mut saved_message_id: Option<String> = None;
        if let Some(ref svc) = self.webchat_service {
            let token_usage = serde_json::json!({
                "model": "kimi-k2.5",
                "prompt_tokens": history.len(),
                "completion_tokens": llm_response.len(),
            });
            if let Ok(id) = svc
                .save_message(
                    &db_session_id,
                    "assistant",
                    &llm_response,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "channel_id": channel_id,
                    })),
                    Some(token_usage),
                )
                .await
            {
                saved_message_id = Some(id);
            }
        }

        // 8. 发送回复
        if let Err(e) = self
            .send_reply(platform, channel_id, &message, &llm_response)
            .await
        {
            warn!(
                "Failed to send reply via WebSocket (will be available for polling): {}",
                e
            );
        } else if let (Some(ref svc), Some(ref msg_id)) =
            (self.webchat_service.as_ref(), saved_message_id.as_ref())
        {
            let _ = svc.mark_ws_delivered(msg_id).await;
        }

        self.promote_user_message_facts(&content, &user_id, &db_session_id, platform)
            .await;

        Ok(())
    }

    /// 处理消息（通过 AgentRuntime）
    ///
    /// 🆕 CRON FIX: When `completion_tx` is provided, the background task will
    /// send the LLM response (or error) through it when finished. This allows
    /// cron jobs to synchronously wait for the result while still reusing the
    /// same processing pipeline as regular user messages.
    pub async fn handle_message_via_agent(
        &self,
        platform: PlatformType,
        channel_id: &str,
        message: Message,
        resolver: Arc<AgentResolver>,
        agent_runtime: Arc<dyn gateway::AgentRuntime>,
        completion_tx: Option<tokio::sync::oneshot::Sender<Result<String, GatewayError>>>,
    ) -> Result<(), GatewayError> {
        // 1. 消息去重检查
        if let Some(msg_id) = message.metadata.get("message_id") {
            if !self
                .deduplicator
                .should_process_key(&platform.to_string(), msg_id)
                .await
            {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message
            .metadata
            .get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self
            .session_manager
            .get_or_create_session(platform, channel_id, &user_id)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to create session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        info!("💬 会话 {} - 用户 {} 发送消息", session.id, user_id);

        // 2.5 统一获取/创建 DB session
        let db_session_id = if let Some(ref svc) = self.webchat_service {
            if platform == PlatformType::WebChat {
                // WebChat: 验证前端提供的 session_id，无效则自动创建
                let provided_sid = message
                    .metadata
                    .get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    Ok(false) => {
                        return Err(GatewayError::not_found("Session", &provided_sid));
                    }
                    Err(e) => {
                        return Err(GatewayError::Internal {
                            message: format!("Failed to validate webchat session: {}", e),
                            correlation_id: Uuid::new_v4().to_string(),
                        });
                    }
                }
            } else {
                // 外部渠道：按 user_id + channel 查找或创建
                let sender_id = message
                    .metadata
                    .get("sender_id")
                    .cloned()
                    .unwrap_or_else(|| channel_id.to_string());
                match svc
                    .get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id)
                    .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        warn!("Failed to get/create channel session: {}", e);
                        session.id.clone()
                    }
                }
            }
        } else {
            session.id.clone()
        };

        // 3. 处理多模态内容（下载图片等）
        let (content, images) = self.process_multimodal(&message).await?;

        if Self::is_stop_command(&content) {
            return self
                .handle_stop_command(
                    platform,
                    channel_id,
                    &message,
                    &session.id,
                    &db_session_id,
                    &user_id,
                    &content,
                )
                .await;
        }

        // 2.1 会话并发保护：同一 session 同时只能处理一条消息。
        // /stop is handled before this guard so users can interrupt an active task.
        let _processing_guard = self.session_manager.try_start_processing(&session.id).await;
        if _processing_guard.is_none() {
            info!("⏳ 会话 {} 正在处理中，跳过新消息", session.id);
            return Ok(());
        }

        // 🟢 P1 FIX: Check for /workflow command trigger (same as handle_message)
        if let Some(workflow_result) = self
            .try_execute_workflow_command(&content, platform, channel_id)
            .await
        {
            match workflow_result {
                Ok(result) => {
                    self.session_manager
                        .add_message(&session.id, "user", &content, false, vec![])
                        .await
                        .ok();
                    self.session_manager
                        .add_message(&session.id, "assistant", &result.text, false, vec![])
                        .await
                        .ok();
                    self.persist_webchat_user_message(
                        &db_session_id,
                        platform,
                        channel_id,
                        &user_id,
                        &content,
                        Some("workflow"),
                    )
                    .await;
                    self.send_workflow_result(
                        platform,
                        channel_id,
                        &message,
                        &db_session_id,
                        &result,
                    )
                    .await?;
                    return Ok(());
                }
                Err(e) => {
                    let error_msg = format!("Workflow execution error: {}", e);
                    self.send_reply(platform, channel_id, &message, &error_msg)
                        .await?;
                    return Ok(());
                }
            }
        }

        // 4. 添加用户消息到会话历史
        let image_urls: Vec<String> = images
            .iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(
                &session.id,
                "user",
                &content,
                !images.is_empty(),
                image_urls,
            )
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    &db_session_id,
                    "user",
                    &content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "has_image": !images.is_empty(),
                        "channel_id": channel_id,
                    })),
                    None,
                )
                .await;
        }

        // 5. 解析 agent_id
        let agent_id = resolver.resolve(platform, channel_id, &user_id).await?;

        // 6.5 Memory 检索
        // 🆕 FIX: 先匹配 skill，统一在 build_memory_context 内注入 skill prompt
        // 并控制总预算
        let mut skill_match = self.try_match_skill(&content).await;

        // 🆕 FIX: Session-level skill inheritance. If current message doesn't match any
        // skill, but the session has an active_skill from previous turns,
        // inherit it to avoid losing skill context in multi-turn conversations.
        // 🆕 SKILL MATCHING V2: Removed all hardcoded keyword rules (exit_keywords,
        // domain relevance). The Agent layer now uses LLM to determine if the
        // user has switched topics. Gateway only checks if the skill still
        // exists and is enabled.
        if skill_match.is_none() {
            let active_skill = session.metadata.get("active_skill").cloned();
            if let Some(skill_id) = active_skill {
                if let Some(ref registry) = self.skill_registry {
                    if let Some(skill) = registry.get(&skill_id).await {
                        if skill.enabled {
                            // 🆕 SKILL MATCHING V2: No hardcoded domain relevance check.
                            // Let the Agent's LLM determine if the skill is still relevant.
                            skill_match = Some((
                                skill_id.clone(),
                                skill.skill.name.clone(),
                                skill.skill.manifest.description.clone(),
                                skill.skill.manifest.prompt_template.clone(),
                            ));
                            info!(
                                "🎯 Inherited active skill '{}' for query '{}'",
                                skill_id,
                                content.chars().take(40).collect::<String>()
                            );
                        }
                    }
                }
            }
        } else {
            // Update active_skill in session metadata when a new skill is matched
            if let Some((ref skill_id, _, _, _)) = skill_match {
                let _ = self
                    .session_manager
                    .update_metadata(&session.id, "active_skill", skill_id)
                    .await;
            }
        }

        // 6. 构建 LLM 上下文（包含历史消息）
        // 🆕 FIX: Limit history to 6 turns for ALL skills to prevent prompt bloat.
        let history_limit = 6;
        let history = self
            .session_manager
            .get_history_for_llm(&session.id, history_limit)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;
        // 🆕 FIX: Truncate each history message to max 300 chars to keep prompt small.
        let history: Vec<_> = history
            .into_iter()
            .map(|mut m| {
                if m.content.chars().count() > 300 {
                    m.content = m.content.chars().take(300).collect::<String>() + "...";
                }
                m
            })
            .collect();

        let (memory_context, direct_answer) =
            self.build_memory_context(&content, &skill_match).await;

        // 🟢 P2 FIX: Memory 精确匹配直接返回，跳过 LLM
        if let Some(answer) = direct_answer {
            info!(
                "🧠 P2 FAST PATH: Memory direct answer, skipping Agent/LLM for '{}'",
                content.chars().take(40).collect::<String>()
            );
            // 更新会话历史
            self.session_manager
                .add_message(&session.id, "assistant", &answer, false, vec![])
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to add assistant message: {}", e),
                    correlation_id: Uuid::new_v4().to_string(),
                })?;
            // 持久化并发送回复
            let msg_id = if let Some(ref svc) = self.webchat_service {
                svc.save_message(&db_session_id, "assistant", &answer, Some(serde_json::json!({"platform": platform.to_string(), "channel_id": channel_id})), None).await.ok()
            } else {
                None
            };
            if self
                .send_reply(platform, channel_id, &message, &answer)
                .await
                .is_ok()
            {
                if let (Some(ref svc), Some(id)) = (self.webchat_service.as_ref(), msg_id) {
                    let _ = svc.mark_ws_delivered(&id).await;
                }
            }
            self.promote_user_message_facts(&content, &user_id, &db_session_id, platform)
                .await;
            return Ok(());
        }

        // 7. 处理 Skill planning 判断
        // 🆕 SKILL MATCHING V2: Removed all hardcoded skill type checks
        // (travel/planner/analytical/generative). Planning need is now
        // determined by the Agent's LLM Intent Analyzer. Gateway no longer
        // injects plan=true based on skill name keywords.
        let has_skill_plan = false;

        // 8. 构造 TaskConfig
        let mut task_input = serde_json::json!({
            "message": content,
            "history": history.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "images": images.iter().map(|img| format!("data:{};base64,{},", img.mime_type, img.data)).collect::<Vec<_>>(),
            "platform": platform.to_string(),
            "channel_id": channel_id,
            "user_id": user_id,
            "session_id": db_session_id,
            "db_session_id": db_session_id,
            "metadata": message.metadata,
            "memory_context": memory_context,
        });
        if let Some((skill_id, skill_name, skill_desc, skill_prompt)) = skill_match {
            if let Some(obj) = task_input.as_object_mut() {
                obj.insert(
                    "skill_hint".to_string(),
                    serde_json::json!({
                        "id": skill_id,
                        "name": skill_name,
                        "description": skill_desc,
                        "prompt_template": skill_prompt,
                    }),
                );
                if has_skill_plan {
                    obj.insert("plan".to_string(), serde_json::json!("true"));
                }
                // 🆕 FIX: For weather_assistant, fetch real-time weather data and inject into
                // task_input
                if skill_name.to_lowercase().contains("weather") {
                    if let Some(city) = Self::extract_city_from_weather_query(&content) {
                        if let Some(weather_data) = Self::fetch_weather_data(&city).await {
                            obj.insert("weather_data".to_string(), serde_json::json!(weather_data));
                            info!("🌤️ Injected weather data for '{}' into task input", city);
                        }
                    }
                }
            }
        }

        // 🆕 STREAMING: Create stream channel for WebChat platform
        let (stream_tx, stream_rx) = if platform == PlatformType::WebChat {
            let (tx, rx) = tokio::sync::mpsc::channel::<String>(100);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // 🟢 P2 FIX: 发送"正在思考..."占位消息（非 WebChat 平台）。
        // WebChat 平台通过流式输出提供实时反馈，不需要占位消息。
        if platform != PlatformType::WebChat {
            let placeholder = "🤖 正在思考，请稍候...";
            self.send_reply(platform, channel_id, &message, placeholder)
                .await?;
        }

        // 克隆需要在后台任务中使用的数据
        let processor = Arc::new(MessageProcessor {
            deduplicator: Arc::clone(&self.deduplicator),
            session_manager: Arc::clone(&self.session_manager),
            multimodal_processor: MultimodalProcessor::new(), // placeholder, not used in bg
            llm_service: Arc::clone(&self.llm_service),
            channel_registry: Arc::clone(&self.channel_registry),
            memory_system: self.memory_system.as_ref().map(Arc::clone),
            webchat_service: self.webchat_service.as_ref().map(Arc::clone),
            skill_registry: self.skill_registry.as_ref().map(Arc::clone),
            mcp_manager: self.mcp_manager.as_ref().map(Arc::clone),
            workflow_registry: self.workflow_registry.as_ref().map(Arc::clone),
            clawhub_client: self.clawhub_client.clone(),
            tool_call_trace_store: self.tool_call_trace_store.as_ref().map(Arc::clone),
        });
        // 🆕 FIX: Register cancellation token for this session before spawning
        // background task. The returned generation token prevents a slow old
        // task from deleting a new task's sender.
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let cancel_gen =
            beebotos_agents::session_cancellation::register(&db_session_id, cancel_tx).await;

        if let Some(obj) = task_input.as_object_mut() {
            obj.insert(
                "cancellation_generation".to_string(),
                serde_json::json!(cancel_gen.to_string()),
            );
        }
        if platform == PlatformType::WebChat {
            if let Some(store) = &self.tool_call_trace_store {
                let _ = store.drain(&db_session_id);
                store.start_session(&db_session_id);
            }
        }

        let task = gateway::TaskConfig {
            task_type: "llm_chat".to_string(),
            input: task_input,
            timeout_secs: 500,
            priority: 5,
            stream_tx,
        };

        let session_id = session.id.clone();
        let db_session_id_bg = db_session_id.clone();
        let user_id_bg = user_id.clone();
        let content_bg = content.clone();
        let channel_id_bg = channel_id.to_string();
        let agent_id_bg = agent_id.clone();
        let message_bg = message.clone();
        let platform_bg = platform;
        let agent_runtime_bg = Arc::clone(&agent_runtime);
        let tool_calls = Arc::new(tokio::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let tool_calls_bg = Arc::clone(&tool_calls);

        let (stream_count_tx, mut stream_count_rx) = tokio::sync::oneshot::channel::<usize>();

        // 🆕 STREAMING: Spawn a task to consume stream chunks and send to WebSocket
        if let Some(stream_rx) = stream_rx {
            let channel_id_stream = channel_id_bg.clone();
            let processor_stream = Arc::clone(&processor);
            let tool_calls_stream = Arc::clone(&tool_calls);
            let stream_handle = tokio::spawn(async move {
                let mut rx = stream_rx;
                let mut chunk_count = 0;
                while let Some(chunk) = rx.recv().await {
                    chunk_count += 1;
                    if let Some(event_json) = chunk.strip_prefix(STREAM_EVENT_PREFIX) {
                        match serde_json::from_str::<serde_json::Value>(event_json) {
                            Ok(event) => {
                                tool_calls_stream.lock().await.push(event.clone());
                                match processor_stream
                                    .channel_registry
                                    .get_channel_by_platform(PlatformType::WebChat)
                                    .await
                                {
                                    Some(channel) => {
                                        let guard = channel.read().await;
                                        if let Some(webchat) = guard.as_any().downcast_ref::<
                                            beebotos_agents::communication::channel::WebChatChannel,
                                        >() {
                                            let _ = webchat
                                                .send_tool_call(&channel_id_stream, event)
                                                .await;
                                        }
                                    }
                                    None => {
                                        warn!(
                                            "Tool call event dropped: WebChat channel not \
                                             available"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Invalid stream event payload dropped: {}", e);
                            }
                        }
                        continue;
                    }

                    match processor_stream
                        .channel_registry
                        .get_channel_by_platform(PlatformType::WebChat)
                        .await
                    {
                        Some(channel) => {
                            let guard = channel.read().await;
                            if let Some(webchat) = guard.as_any()
                                .downcast_ref::<beebotos_agents::communication::channel::WebChatChannel>()
                            {
                                let _ = webchat.send_stream_chunk(&channel_id_stream, &chunk, false).await;
                            }
                        }
                        None => {
                            warn!(
                                "Stream chunk dropped ({} bytes): WebChat channel not available",
                                chunk.len()
                            );
                        }
                    }
                }
                info!(
                    "Stream consumer finished for session {}: {} chunks processed",
                    channel_id_stream, chunk_count
                );
                // Send finished=true when stream ends
                match processor_stream
                    .channel_registry
                    .get_channel_by_platform(PlatformType::WebChat)
                    .await
                {
                    Some(channel) => {
                        let guard = channel.read().await;
                        if let Some(webchat) = guard.as_any()
                            .downcast_ref::<beebotos_agents::communication::channel::WebChatChannel>()
                        {
                            let _ = webchat.send_stream_chunk(&channel_id_stream, "", true).await;
                        }
                    }
                    None => {
                        warn!(
                            "Failed to send finished=true for session {}: WebChat channel not \
                             available",
                            channel_id_stream
                        );
                    }
                }
                let _ = stream_count_tx.send(chunk_count);
            });
            let _ = beebotos_agents::session_cancellation::set_abort_handle(
                &db_session_id,
                cancel_gen,
                stream_handle.abort_handle(),
            )
            .await;
        } else {
            let _ = stream_count_tx.send(0);
        }

        let db_session_id_cleanup = db_session_id.clone();
        let channel_id_cleanup = channel_id_bg.clone();
        let session_id_cleanup = session_id.clone();
        let processor_cleanup = Arc::clone(&processor);
        let message_bg_cleanup = message_bg.clone();
        let work_handle = tokio::spawn(async move {
            info!("🤖 [BG] Agent {} 开始后台处理消息", agent_id_bg);
            let start = std::time::Instant::now();

            let result = agent_runtime_bg.execute_task(&agent_id_bg, task).await;
            let (llm_response, completion_result) = match result {
                Ok(r) if r.success => {
                    let response = r
                        .output
                        .as_str()
                        .map(|s| s.to_string())
                        .or_else(|| {
                            r.output
                                .get("response")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| "Agent returned empty response".to_string());
                    (response.clone(), Ok(response))
                }
                Ok(r) => {
                    let err = r
                        .error
                        .clone()
                        .unwrap_or_else(|| "Agent processing failed".to_string());
                    (err.clone(), Err(GatewayError::internal(err)))
                }
                Err(e) => {
                    error!("❌ [BG] Agent execution failed: {}", e);
                    let err = format!("处理失败: {}", e);
                    (err.clone(), Err(GatewayError::internal(err)))
                }
            };

            info!(
                "🤖 [BG] Agent {} 回复 ({}ms): {}",
                agent_id_bg,
                start.elapsed().as_millis(),
                llm_response.chars().take(100).collect::<String>()
            );

            // 更新会话历史
            let _ = processor
                .session_manager
                .add_message(&session_id, "assistant", &llm_response, false, vec![])
                .await;

            let stream_chunk_count = if platform_bg == PlatformType::WebChat {
                match tokio::time::timeout(std::time::Duration::from_secs(2), &mut stream_count_rx)
                    .await
                {
                    Ok(Ok(count)) => count,
                    Ok(Err(_)) => 0,
                    Err(_) => {
                        warn!(
                            "[BG] Timed out waiting for WebChat stream completion for session {}",
                            channel_id_bg
                        );
                        0
                    }
                }
            } else {
                0
            };

            // Capture tool calls only after the stream consumer has finished
            // draining the side-channel events, so the persisted message keeps
            // the complete tool-call list.
            let trace_tool_calls = processor
                .tool_call_trace_store
                .as_ref()
                .map(|store| store.finish_session(&db_session_id_bg))
                .unwrap_or_default();
            let tool_calls_snapshot =
                merge_tool_call_snapshots(tool_calls_bg.lock().await.clone(), trace_tool_calls);

            // 持久化 AI 回复
            let mut saved_message_id: Option<String> = None;
            if let Some(ref svc) = processor.webchat_service {
                if let Ok(id) = svc
                    .save_message(
                        &db_session_id_bg,
                        "assistant",
                        &llm_response,
                        Some(serde_json::json!({
                            "platform": platform_bg.to_string(),
                            "channel_id": channel_id_bg.clone(),
                            "tool_calls": tool_calls_snapshot.clone(),
                        })),
                        None,
                    )
                    .await
                {
                    saved_message_id = Some(id);
                }
            }

            // 🆕 STREAMING: For non-WebChat platforms, send the full reply directly.
            // For WebChat, stream chunks are preferred; if no chunks were produced
            // (for example approval-confirmation fast paths), send the full reply.
            if platform_bg != PlatformType::WebChat {
                if let Err(e) = processor
                    .send_reply(platform_bg, &channel_id_bg, &message_bg, &llm_response)
                    .await
                {
                    warn!(
                        "[BG] Failed to send reply (will be available for polling): {}",
                        e
                    );
                }
            } else if completion_result.is_err() || stream_chunk_count == 0 {
                let mut reply = message_bg.clone();
                reply.id = saved_message_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .unwrap_or_else(Uuid::new_v4);
                reply.content = llm_response.clone();
                reply.metadata.clear();
                if !tool_calls_snapshot.is_empty() {
                    if let Ok(tool_calls_json) = serde_json::to_string(&tool_calls_snapshot) {
                        reply
                            .metadata
                            .insert("tool_calls".to_string(), tool_calls_json);
                    }
                }

                if let Some(channel) = processor
                    .channel_registry
                    .get_channel_by_platform(PlatformType::WebChat)
                    .await
                {
                    if let Err(e) = channel.read().await.send(&channel_id_bg, &reply).await {
                        warn!("[BG] Failed to send final WebChat reply: {}", e);
                    }
                } else {
                    warn!("[BG] Failed to send final WebChat reply: channel unavailable");
                }
            }
            // Mark non-WebChat deliveries after the response has been handed to
            // the channel. WebChat delivery is acknowledged by the browser via
            // /webchat/messages/:id/ack, so reconnect recovery is not defeated
            // by a best-effort server-side broadcast.
            if let (Some(ref svc), Some(ref msg_id)) = (
                processor.webchat_service.as_ref(),
                saved_message_id.as_ref(),
            ) {
                if platform_bg != PlatformType::WebChat {
                    let _ = svc.mark_ws_delivered(msg_id).await;
                }
            }

            processor
                .promote_user_message_facts(
                    &content_bg,
                    &user_id_bg,
                    &db_session_id_bg,
                    platform_bg,
                )
                .await;

            completion_result
        });
        let _ = beebotos_agents::session_cancellation::set_abort_handle(
            &db_session_id_cleanup,
            cancel_gen,
            work_handle.abort_handle(),
        )
        .await;

        tokio::spawn(async move {
            match work_handle.await {
                Ok(completion_result) => {
                    if let Some(tx) = completion_tx {
                        let _ = tx.send(completion_result);
                    }
                }
                Err(e) if e.is_cancelled() => {
                    if let Some(store) = processor_cleanup.tool_call_trace_store.as_ref() {
                        let _ = store.finish_session(&db_session_id_cleanup);
                    }
                    info!(
                        "[BG] Agent task interrupted for WebChat session {}",
                        channel_id_cleanup
                    );
                    let interrupted_text = "⏹️ 已停止当前任务。".to_string();

                    let _ = processor_cleanup
                        .session_manager
                        .add_message(
                            &session_id_cleanup,
                            "assistant",
                            &interrupted_text,
                            false,
                            vec![],
                        )
                        .await;

                    let mut saved_message_id: Option<String> = None;
                    if let Some(ref svc) = processor_cleanup.webchat_service {
                        if let Ok(id) = svc
                            .save_message(
                                &db_session_id_cleanup,
                                "assistant",
                                &interrupted_text,
                                Some(serde_json::json!({
                                    "platform": platform_bg.to_string(),
                                    "channel_id": channel_id_cleanup.clone(),
                                    "interrupted": true,
                                })),
                                None,
                            )
                            .await
                        {
                            saved_message_id = Some(id);
                        }
                    }

                    let mut reply = message_bg_cleanup.clone();
                    reply.id = saved_message_id
                        .as_deref()
                        .and_then(|id| Uuid::parse_str(id).ok())
                        .unwrap_or_else(Uuid::new_v4);
                    reply.content = interrupted_text.clone();
                    reply.metadata.clear();

                    let mut delivered = false;
                    if let Some(channel) = processor_cleanup
                        .channel_registry
                        .get_channel_by_platform(PlatformType::WebChat)
                        .await
                    {
                        delivered = channel
                            .read()
                            .await
                            .send(&channel_id_cleanup, &reply)
                            .await
                            .is_ok();
                    }

                    if let (Some(ref svc), Some(ref msg_id)) = (
                        processor_cleanup.webchat_service.as_ref(),
                        saved_message_id.as_ref(),
                    ) {
                        if delivered {
                            let _ = svc.mark_ws_delivered(msg_id).await;
                        }
                    }

                    if let Some(tx) = completion_tx {
                        let _ = tx.send(Ok(interrupted_text));
                    }
                }
                Err(e) => {
                    if let Some(store) = processor_cleanup.tool_call_trace_store.as_ref() {
                        let _ = store.finish_session(&db_session_id_cleanup);
                    }
                    let err = format!("Agent task join failed: {}", e);
                    warn!("[BG] {} for session {}", err, channel_id_cleanup);
                    if let Some(tx) = completion_tx {
                        let _ = tx.send(Err(GatewayError::internal(err)));
                    }
                }
            }

            // Unregister cancellation token when background work completes or
            // is aborted. Only remove if the generation matches, preventing
            // race with newer tasks.
            beebotos_agents::session_cancellation::unregister(&db_session_id_cleanup, cancel_gen)
                .await;
        });

        Ok(())
    }

    async fn promote_user_message_facts(
        &self,
        content: &str,
        user_id: &str,
        session_id: &str,
        platform: PlatformType,
    ) {
        let Some(memory) = self.memory_system.as_ref() else {
            return;
        };
        let facts = match self
            .extract_promoted_memory_facts_with_llm(content, memory)
            .await
        {
            Ok(Some(facts)) => facts,
            Ok(None) => Self::extract_promoted_memory_facts(content),
            Err(e) => {
                warn!("LLM memory fact promotion failed: {}", e);
                Self::extract_promoted_memory_facts(content)
            }
        };
        if facts.is_empty() {
            return;
        }

        for fact in facts {
            if let Err(e) = self
                .apply_promoted_memory_fact(memory, &fact, user_id, session_id, platform)
                .await
            {
                warn!("Failed to promote memory fact '{}': {}", fact.title, e);
            }
        }
    }

    async fn extract_promoted_memory_facts_with_llm(
        &self,
        content: &str,
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
    ) -> Result<Option<Vec<PromotedMemoryFact>>, GatewayError> {
        if !Self::should_review_memory_fact(content) {
            return Ok(None);
        }

        let memory_context = Self::memory_promotion_context(memory).await;
        let messages = vec![
            LLMMessage::system(Self::memory_promotion_system_prompt()),
            LLMMessage::user(format!(
                "Current long-term memory:\n{}\n\nLatest user message:\n{}",
                memory_context,
                Self::truncate_for_prompt(content, 4000)
            )),
        ];
        let raw = self
            .llm_service
            .chat(messages, Some(700), None, Some("none".to_string()), None)
            .await?;

        Ok(Some(Self::parse_promoted_memory_facts(&raw)))
    }

    async fn apply_promoted_memory_fact(
        &self,
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
        fact: &PromotedMemoryFact,
        user_id: &str,
        session_id: &str,
        platform: PlatformType,
    ) -> beebotos_agents::error::Result<()> {
        match fact.action {
            MemoryPromotionAction::Add => {
                if Self::memory_fact_exists(memory, fact).await? {
                    return Ok(());
                }
                self.store_promoted_memory_fact(memory, fact, user_id, session_id, platform)
                    .await
            }
            MemoryPromotionAction::Replace => {
                Self::remove_matching_memory_facts(memory, fact).await?;
                if !Self::memory_fact_exists(memory, fact).await? {
                    self.store_promoted_memory_fact(memory, fact, user_id, session_id, platform)
                        .await?;
                }
                Ok(())
            }
            MemoryPromotionAction::Remove => {
                Self::remove_matching_memory_facts(memory, fact).await?;
                Ok(())
            }
            MemoryPromotionAction::Ignore => Ok(()),
        }
    }

    async fn store_promoted_memory_fact(
        &self,
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
        fact: &PromotedMemoryFact,
        user_id: &str,
        session_id: &str,
        platform: PlatformType,
    ) -> beebotos_agents::error::Result<()> {
        let entry = MarkdownMemoryEntry::new(&fact.title, &fact.content)
            .with_category(&fact.category)
            .with_importance(fact.importance)
            .with_session_id(session_id)
            .with_metadata("source", "fact_promotion")
            .with_metadata("user_id", user_id)
            .with_metadata("platform", platform.to_string())
            .with_metadata("action", format!("{:?}", fact.action).to_lowercase());

        memory.store(fact.file_type, &entry, None).await?;
        info!(
            "Promoted memory fact '{}' to {}",
            fact.title,
            fact.file_type.filename(None)
        );
        Ok(())
    }

    async fn memory_fact_exists(
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
        fact: &PromotedMemoryFact,
    ) -> beebotos_agents::error::Result<bool> {
        let existing = memory.storage().read_entries(fact.file_type, None).await?;
        let needle = Self::normalize_fact_text(&fact.content);
        Ok(existing
            .iter()
            .any(|entry| Self::normalize_fact_text(&entry.content).contains(&needle)))
    }

    async fn remove_matching_memory_facts(
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
        fact: &PromotedMemoryFact,
    ) -> beebotos_agents::error::Result<bool> {
        let Some(pattern) = fact
            .replace_match
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(false);
        };

        let mut changed = false;
        let mut entries = memory.storage().read_entries(fact.file_type, None).await?;
        let needle = Self::normalize_fact_text(pattern);

        for entry in &mut entries {
            let before = entry.content.clone();
            if Self::normalize_fact_text(&entry.content).contains(&needle) {
                entry.content = Self::remove_matching_lines(&entry.content, pattern);
                changed |= entry.content != before;
            }
        }

        let before_len = entries.len();
        entries.retain(|entry| !entry.content.trim().is_empty());
        changed |= entries.len() != before_len;

        if changed {
            memory
                .rewrite_entries(fact.file_type, &entries, None)
                .await?;
        }

        Ok(changed)
    }

    fn extract_promoted_memory_facts(content: &str) -> Vec<PromotedMemoryFact> {
        let text = content.trim();
        if text.is_empty() || text.starts_with('/') {
            return Vec::new();
        }

        let mut facts = Vec::new();

        if let Some(name) = Self::capture_first(
            text,
            r"(?:我叫|我的名字是|我名字叫)\s*([\p{Han}A-Za-z·]{2,24})",
        ) {
            facts.push(PromotedMemoryFact {
                action: MemoryPromotionAction::Add,
                file_type: MemoryFileType::User,
                title: "Basic Information".to_string(),
                content: format!("- Name: {}", name),
                category: "profile".to_string(),
                importance: 1.0,
                replace_match: None,
            });
        }

        if Self::contains_any(text, &["打篮球", "篮球"])
            && Self::contains_any(text, &["我喜欢", "喜欢", "经常", "平常", "平时", "常常"])
        {
            let habit = if Self::contains_any(text, &["经常", "平常", "平时", "常常"]) {
                "经常打篮球"
            } else {
                "喜欢打篮球"
            };
            facts.push(PromotedMemoryFact {
                action: MemoryPromotionAction::Add,
                file_type: MemoryFileType::User,
                title: "Interests".to_string(),
                content: format!("- Interests: {}", habit),
                category: "profile".to_string(),
                importance: 0.8,
                replace_match: None,
            });
        }

        if let Some(shell) = Self::extract_shell_fact(text) {
            facts.push(PromotedMemoryFact {
                action: MemoryPromotionAction::Add,
                file_type: MemoryFileType::Core,
                title: "Local Environment".to_string(),
                content: format!("- Local shell: {}", shell.to_lowercase()),
                category: "environment".to_string(),
                importance: 1.0,
                replace_match: None,
            });
        }

        facts
    }

    fn parse_promoted_memory_facts(raw: &str) -> Vec<PromotedMemoryFact> {
        let Some(payload) = Self::extract_json_payload(raw) else {
            return Vec::new();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
            return Vec::new();
        };

        let decisions = if let Some(items) = value.get("decisions").and_then(|v| v.as_array()) {
            items.clone()
        } else if let Some(items) = value.as_array() {
            items.clone()
        } else if value.get("action").is_some() {
            vec![value]
        } else {
            Vec::new()
        };

        decisions
            .into_iter()
            .filter_map(|value| serde_json::from_value::<MemoryPromotionDecision>(value).ok())
            .filter_map(Self::decision_to_promoted_fact)
            .collect()
    }

    fn decision_to_promoted_fact(decision: MemoryPromotionDecision) -> Option<PromotedMemoryFact> {
        let action = MemoryPromotionAction::parse(&decision.action)?;
        if action == MemoryPromotionAction::Ignore {
            return None;
        }

        let confidence = decision.confidence.unwrap_or(0.0);
        if confidence < 0.72 {
            return None;
        }

        let file_type = Self::memory_file_type_from_target(decision.target.as_deref()?)?;
        let content = Self::normalize_promoted_fact_content(&decision.content.unwrap_or_default());
        if action != MemoryPromotionAction::Remove
            && (content.is_empty() || content.len() > 600 || Self::looks_sensitive(&content))
        {
            return None;
        }

        Some(PromotedMemoryFact {
            action,
            file_type,
            title: decision
                .title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Promoted Fact".to_string()),
            content,
            category: decision.category.unwrap_or_else(|| match file_type {
                MemoryFileType::User => "profile".to_string(),
                _ => "project".to_string(),
            }),
            importance: decision.importance.unwrap_or(0.7).clamp(0.0, 1.0),
            replace_match: decision
                .replace_match
                .filter(|value| !value.trim().is_empty()),
        })
    }

    fn memory_file_type_from_target(target: &str) -> Option<MemoryFileType> {
        match target.trim().to_lowercase().as_str() {
            "user" | "user.md" | "profile" => Some(MemoryFileType::User),
            "memory" | "memory.md" | "core" | "agent" | "project" | "environment" => {
                Some(MemoryFileType::Core)
            }
            _ => None,
        }
    }

    fn extract_json_payload(raw: &str) -> Option<String> {
        let mut text = raw.trim();
        if text.starts_with("```") {
            text = text.trim_start_matches("```").trim_start();
            if let Some(rest) = text.strip_prefix("json") {
                text = rest.trim_start();
            }
            if let Some(idx) = text.rfind("```") {
                text = &text[..idx];
            }
        }

        let start = text.find(['{', '['])?;
        let end_obj = text.rfind('}');
        let end_arr = text.rfind(']');
        let end = match (end_obj, end_arr) {
            (Some(a), Some(b)) => a.max(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return None,
        };
        Some(text[start..=end].trim().to_string())
    }

    fn should_review_memory_fact(content: &str) -> bool {
        let text = content.trim();
        if text.is_empty() || text.starts_with('/') || text.chars().count() > 4000 {
            return false;
        }

        !matches!(
            text.to_lowercase().as_str(),
            "hi" | "hello" | "hey" | "你好" | "在吗" | "ok" | "嗯" | "好的"
        )
    }

    async fn memory_promotion_context(
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
    ) -> String {
        let mut output = String::new();
        Self::push_memory_entries(&mut output, memory, MemoryFileType::User, "USER.md").await;
        Self::push_memory_entries(&mut output, memory, MemoryFileType::Core, "MEMORY.md").await;
        Self::truncate_for_prompt(&output, 6000)
    }

    async fn push_memory_entries(
        output: &mut String,
        memory: &beebotos_agents::memory::UnifiedMemorySystem,
        file_type: MemoryFileType,
        label: &str,
    ) {
        output.push_str(label);
        output.push('\n');
        match memory.storage().read_entries(file_type, None).await {
            Ok(entries) if !entries.is_empty() => {
                for entry in entries {
                    output.push_str("- ");
                    output.push_str(&entry.title);
                    output.push_str(": ");
                    output.push_str(&entry.content.replace('\n', " "));
                    output.push('\n');
                }
            }
            _ => output.push_str("- <empty>\n"),
        }
    }

    fn memory_promotion_system_prompt() -> &'static str {
        r#"You are BeeBotOS memory reviewer. Decide whether the latest user message contains durable facts worth promoting to long-term memory.

Memory layers:
- USER.md: user identity, stable preferences, interests, communication style, personal profile facts.
- MEMORY.md: agent/project/environment/tool notes, local setup, project conventions, operational pitfalls.
- SessionDB: ordinary conversation, transient events, task details, and anything not durable.

Return JSON only:
{"decisions":[{"action":"add|replace|remove|ignore","target":"user|memory","title":"short section","content":"one concise Markdown bullet","category":"profile|environment|project|preference|tool_pitfall","importance":0.0-1.0,"confidence":0.0-1.0,"replace_match":"optional existing fact phrase","reason":"short"}]}

Rules:
- Prefer ignore unless the fact is durable and useful in future sessions.
- Use add for new durable facts.
- Use replace when the new message changes or contradicts existing memory; set replace_match to the old phrase to remove.
- Use remove when the user says a stored fact is wrong or should be forgotten; set replace_match.
- Never store passwords, tokens, secrets, private keys, or one-off sensitive content.
- For transient facts like "today I played basketball", ignore.
- Keep content short, factual, and directly usable. Do not store ordinary chat transcripts."#
    }

    fn extract_shell_fact(text: &str) -> Option<String> {
        let lower = text.to_lowercase();
        if !lower.contains("shell") && !text.contains("终端") {
            return None;
        }

        for shell in ["fish", "zsh", "bash", "powershell", "pwsh", "nushell", "nu"] {
            if lower.contains(shell) {
                return Some(shell.to_string());
            }
        }

        if lower.split_whitespace().any(|part| part == "sh") {
            return Some("sh".to_string());
        }
        None
    }

    fn capture_first(text: &str, pattern: &str) -> Option<String> {
        let re = Regex::new(pattern).ok()?;
        re.captures(text)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn contains_any(text: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| text.contains(needle))
    }

    fn normalize_fact_text(text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
    }

    fn normalize_promoted_fact_content(content: &str) -> String {
        let content = content.trim();
        if content.is_empty() || content.starts_with('-') {
            content.to_string()
        } else {
            format!("- {}", content)
        }
    }

    fn remove_matching_lines(content: &str, pattern: &str) -> String {
        let needle = Self::normalize_fact_text(pattern);
        content
            .lines()
            .filter(|line| !Self::normalize_fact_text(line).contains(&needle))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
        let mut out = String::new();
        for ch in text.chars().take(max_chars) {
            out.push(ch);
        }
        out
    }

    fn looks_sensitive(text: &str) -> bool {
        let lower = text.to_lowercase();
        [
            "password", "passwd", "token", "api_key", "apikey", "secret", "私钥", "密码", "密钥",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
    }

    /// 处理多模态内容
    async fn process_multimodal(
        &self,
        message: &Message,
    ) -> Result<(String, Vec<ProcessedImage>), GatewayError> {
        // 检查是否有图片
        if let Some(image_key) = self.extract_image_key(&message.content) {
            info!("🖼️ 检测到图片: {}", image_key);

            // 获取 channel 以下载图片
            if let Some(channel) = self
                .channel_registry
                .get_channel_by_platform(message.platform)
                .await
            {
                let message_id = message.metadata.get("message_id").map(|s| s.as_str());

                // 下载图片
                match channel
                    .read()
                    .await
                    .download_image(&image_key, message_id)
                    .await
                {
                    Ok(image_data) => {
                        // 处理图片
                        let processed = self.process_image(&image_data)?;
                        let text = self.clean_text_content(&message.content);
                        return Ok((text, vec![processed]));
                    }
                    Err(e) => {
                        warn!("图片下载失败: {}", e);
                    }
                }
            }
        }

        // 纯文本消息
        Ok((message.content.clone(), vec![]))
    }

    /// 提取图片 key
    fn extract_image_key(&self, content: &str) -> Option<String> {
        // 匹配 image_key: xxx 格式
        if let Some(pos) = content.find("image_key:") {
            let start = pos + "image_key:".len();
            let rest = &content[start..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ']')
                .unwrap_or(rest.len());
            let key = rest[..end].trim();
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
        None
    }

    /// 清理文本内容
    fn clean_text_content(&self, content: &str) -> String {
        // 移除 image_key 标记
        let re = regex::Regex::new(r"\[?图片\]?\s*image_key:\s*\S+").unwrap();
        re.replace_all(content, "[图片]").to_string()
    }

    /// 处理图片
    fn process_image(&self, data: &[u8]) -> Result<ProcessedImage, GatewayError> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;

        // 检测图片格式
        let format = self.detect_image_format(data)?;

        // 编码为 base64
        let base64_data = STANDARD.encode(data);

        Ok(ProcessedImage {
            data: base64_data,
            format: format.clone(),
            mime_type: format.mime_type().to_string(),
        })
    }

    /// 检测图片格式
    fn detect_image_format(&self, data: &[u8]) -> Result<ImageFormat, GatewayError> {
        if data.len() < 8 {
            return Err(GatewayError::Internal {
                message: "Image data too small".to_string(),
                correlation_id: Uuid::new_v4().to_string(),
            });
        }

        // PNG: 89 50 4E 47
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Ok(ImageFormat::Png);
        }
        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Ok(ImageFormat::Jpeg);
        }
        // GIF: GIF87a or GIF89a
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }
        // WebP: RIFF....WEBP
        if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
            return Ok(ImageFormat::Webp);
        }

        Err(GatewayError::Internal {
            message: "Unknown image format".to_string(),
            correlation_id: Uuid::new_v4().to_string(),
        })
    }

    /// Extract a direct MCP skill reference like
    /// "mcp:alpaca/get_crypto_latest_trade" from text.
    fn extract_mcp_skill_reference(content: &str) -> Option<String> {
        // Look for pattern "mcp:word/word" (alphanumeric, underscore, hyphen allowed)
        let re = Regex::new(r"mcp:[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+").ok()?;
        re.find(content).map(|m| m.as_str().to_string())
    }

    /// 🆕 FIX: Gateway-side skill matching is disabled.
    /// LLM now has full autonomy to choose the appropriate skill from the
    /// catalog based on user intent. This avoids keyword-misunderstanding
    /// issues where gateway matches a skill that does not fit the user request.
    async fn try_match_skill(&self, _content: &str) -> Option<(String, String, String, String)> {
        None
    }

    /// 🆕 FIX: Try to discover and install a skill from ClawHub when no local
    /// match is found. Downloads the skill package, extracts it, and
    /// registers it into the local SkillRegistry.
    async fn try_install_from_clawhub(
        &self,
        query: &str,
    ) -> Option<(String, String, String, String)> {
        let client = self.clawhub_client.as_ref()?;
        let registry = self.skill_registry.as_ref()?;

        // 1. Search ClawHub for relevant skills
        info!(
            "🔍 ClawHub: searching for skill matching '{}'",
            query.chars().take(40).collect::<String>()
        );
        let results = match client.search_skills(query).await {
            Ok(r) if !r.is_empty() => r,
            Ok(_) => {
                info!(
                    "🔍 ClawHub: no skills found for '{}'",
                    query.chars().take(40).collect::<String>()
                );
                return None;
            }
            Err(e) => {
                warn!("🔍 ClawHub search failed: {}", e);
                return None;
            }
        };

        let best = &results[0];
        info!("🔍 ClawHub: found skill '{}' ({})", best.name, best.id);

        // 2. Download skill package
        let pkg_bytes = match client.download_skill(&best.id, None).await {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("🔍 ClawHub download failed for '{}': {}", best.id, e);
                return None;
            }
        };
        info!(
            "🔍 ClawHub: downloaded {} bytes for '{}'",
            pkg_bytes.len(),
            best.id
        );

        // 3. Save to skills/market/{id}/
        let market_dir = std::path::PathBuf::from("skills/market").join(&best.id);
        if let Err(e) = tokio::fs::create_dir_all(&market_dir).await {
            warn!(
                "🔍 ClawHub: failed to create dir '{}': {}",
                market_dir.display(),
                e
            );
            return None;
        }

        // Try to parse as ZIP first, then fallback to raw markdown
        let skill_md_content = if pkg_bytes.len() > 4 && pkg_bytes[0..4] == [0x50, 0x4B, 0x03, 0x04]
        {
            // ZIP archive
            match Self::extract_skill_md_from_zip(&pkg_bytes, &market_dir).await {
                Ok(content) => content,
                Err(e) => {
                    warn!(
                        "🔍 ClawHub: ZIP extraction failed: {}, falling back to description",
                        e
                    );
                    Self::build_fallback_skill_md(best)
                }
            }
        } else if let Ok(text) = String::from_utf8(pkg_bytes.clone()) {
            // Plain text / markdown
            text
        } else {
            warn!("🔍 ClawHub: package is not text or ZIP, using fallback markdown");
            Self::build_fallback_skill_md(best)
        };

        let md_path = market_dir.join("SKILL.md");
        if let Err(e) = tokio::fs::write(&md_path, &skill_md_content).await {
            warn!("🔍 ClawHub: failed to write '{}': {}", md_path.display(), e);
            return None;
        }

        // 4. Parse markdown sections (same logic as builtin_loader)
        let sections = Self::parse_markdown_sections(&skill_md_content);
        let description = sections
            .get("description")
            .cloned()
            .unwrap_or_else(|| best.description.clone());
        let prompt_template = sections.get("prompt_template").cloned().unwrap_or_default();
        let examples = sections.get("examples").cloned().unwrap_or_default();
        let capabilities: Vec<String> = sections
            .get("capabilities")
            .map(|text| {
                text.lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                            Some(trimmed[2..].trim().to_string())
                        } else if trimmed.starts_with("• ") {
                            Some(trimmed[2..].trim().to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let version = match beebotos_agents::skills::registry::Version::parse(&best.version) {
            Ok(v) => v,
            Err(_) => beebotos_agents::skills::registry::Version::new(1, 0, 0),
        };

        let skill = beebotos_agents::skills::loader::LoadedSkill {
            id: best.id.clone(),
            name: best.name.clone(),
            version,
            wasm_path: std::path::PathBuf::new(),
            source_path: md_path.clone(),
            manifest: beebotos_agents::skills::loader::SkillManifest {
                id: best.id.clone(),
                name: best.name.clone(),
                version: beebotos_agents::skills::registry::Version::new(1, 0, 0),
                description: description.clone(),
                author: best.author.clone(),
                capabilities: if capabilities.is_empty() {
                    vec!["llm:chat".to_string()]
                } else {
                    capabilities
                },
                permissions: vec!["llm:chat".to_string()],
                entry_point: "run".to_string(),
                license: best.license.clone(),
                functions: vec![],
                prompt_template: prompt_template.clone(),
                examples,
                when_to_use: description.clone(),
                when_not_to_use: None,
                activation_examples: vec![],
                activation_negative_examples: vec![],
                dependencies: vec![],
                ..Default::default()
            },
        };

        // 5. Register into SkillRegistry
        registry
            .register(skill, "market", best.capabilities.clone())
            .await;
        info!(
            "✅ ClawHub: skill '{}' installed and registered from marketplace",
            best.id
        );

        Some((
            best.id.clone(),
            best.name.clone(),
            description,
            prompt_template,
        ))
    }

    /// Build fallback markdown skill from metadata when download/extraction
    /// fails
    fn build_fallback_skill_md(meta: &crate::clients::SkillMetadata) -> String {
        format!(
            "# {}\n\n## Description\n{}\n\n## Prompt Template\n\nYou are a helpful assistant \
             specialized in {}. Answer user questions accurately and concisely.\n",
            meta.name, meta.description, meta.name
        )
    }

    /// Extract SKILL.md from a ZIP archive
    async fn extract_skill_md_from_zip(
        data: &[u8],
        _dest_dir: &std::path::Path,
    ) -> Result<String, String> {
        use std::io::{Cursor, Read};
        let cursor = Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("ZIP error: {}", e))?;

        // Try to find SKILL.md or any .md file
        let mut found = None;
        for i in 0..archive.len() {
            let file = archive
                .by_index(i)
                .map_err(|e| format!("ZIP read error: {}", e))?;
            let name = file.name().to_lowercase();
            if name.ends_with("skill.md") || name.ends_with(".md") {
                found = Some(i);
                if name.ends_with("skill.md") {
                    break;
                }
            }
        }

        if let Some(idx) = found {
            let mut file = archive
                .by_index(idx)
                .map_err(|e| format!("ZIP read error: {}", e))?;
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("ZIP text read error: {}", e))?;
            Ok(content)
        } else {
            Err("No .md file found in ZIP".to_string())
        }
    }

    /// Parse markdown sections (same logic as builtin_loader)
    fn parse_markdown_sections(content: &str) -> std::collections::HashMap<String, String> {
        let mut sections = std::collections::HashMap::new();
        let mut current_section: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();

        for line in content.lines() {
            if line.starts_with("## ") {
                if let Some(ref name) = current_section {
                    let body = current_lines.join("\n").trim().to_string();
                    if !body.is_empty() {
                        sections.insert(name.clone(), body);
                    }
                }
                current_section = Some(line[3..].trim().to_lowercase().replace(' ', "_"));
                current_lines.clear();
            } else if current_section.is_some() {
                current_lines.push(line.to_string());
            }
        }

        if let Some(ref name) = current_section {
            let body = current_lines.join("\n").trim().to_string();
            if !body.is_empty() {
                sections.insert(name.clone(), body);
            }
        }

        sections
    }

    /// 🟢 P1 FIX: Try to execute a workflow from chat command `/workflow <id>`
    async fn try_execute_workflow_command(
        &self,
        content: &str,
        platform: PlatformType,
        channel_id: &str,
    ) -> Option<Result<WorkflowChatResult, GatewayError>> {
        let trimmed = content.trim();
        if !trimmed.starts_with("/workflow") {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            return Some(Err(GatewayError::bad_request(
                "Usage: /workflow <workflow_id>",
            )));
        }

        let workflow_id = parts[1];
        info!("Chat workflow trigger: {}", workflow_id);

        let registry = self.workflow_registry.as_ref()?;
        let def = {
            let reg = registry.read().await;
            match reg.get(workflow_id) {
                Some(d) => d.clone(),
                None => return Some(Err(GatewayError::not_found("Workflow", workflow_id))),
            }
        };

        // Build temporary agent for execution
        let skill_registry = match self.skill_registry.as_ref() {
            Some(r) => r.clone(),
            None => {
                return Some(Err(GatewayError::service_unavailable(
                    "SkillRegistry",
                    "Not initialized",
                )))
            }
        };

        let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> = Arc::new(
            crate::services::agent_runtime_manager::GatewayLLMInterface::new(
                self.llm_service.clone(),
            ),
        );

        let mut agent = beebotos_agents::AgentBuilder::new("workflow-runner")
            .description("Temporary agent for workflow execution")
            .build()
            .with_skill_registry(skill_registry)
            .with_llm_interface(llm_interface);
        if let Some(ref mcp_manager) = self.mcp_manager {
            agent = agent.with_mcp(mcp_manager.clone());
        }

        let engine = beebotos_agents::workflow::WorkflowEngine::new();
        let trigger_context = serde_json::json!({
            "trigger_type": "chat_command",
            "command": trimmed,
            "platform": "chat"
        });

        let progress_reporter = if platform == PlatformType::WebChat {
            Some(WebChatWorkflowProgressReporter::new(
                self.channel_registry.clone(),
                channel_id.to_string(),
                workflow_id.to_string(),
            ))
        } else {
            None
        };

        match engine
            .execute(
                &def,
                &agent,
                trigger_context,
                progress_reporter
                    .as_ref()
                    .map(|reporter| reporter as &dyn beebotos_agents::workflow::StepProgressReporter),
            )
            .await
        {
            Ok(instance) => {
                let status = instance.status.to_string();
                let mut result = format!(
                    "✅ Workflow '{}' completed with status: {}\n\n",
                    workflow_id, status
                );

                for (step_id, step_state) in &instance.step_states {
                    result.push_str(&format!(
                        "- **{}**: {} ({}s)\n",
                        step_id,
                        step_state.status,
                        step_state.duration_secs()
                    ));
                    if let Some(ref output) = step_state.output {
                        let output_str = match output {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Null => String::new(),
                            _ => serde_json::to_string_pretty(output).unwrap_or_default(),
                        };
                        if !output_str.is_empty() && output_str.len() < 5000 {
                            result.push_str(&format!("  - Output: {}\n", output_str));
                        } else if output_str.len() >= 5000 {
                            result.push_str(&format!(
                                "  - Output: {}... (truncated)\n",
                                &output_str[..500]
                            ));
                        }
                    }
                    if let Some(ref err) = step_state.error {
                        result.push_str(&format!("  - Error: {}\n", err));
                    }
                }

                if !instance.error_log.is_empty() {
                    result.push_str("\n**Errors:**\n");
                    for err in &instance.error_log {
                        result.push_str(&format!(
                            "- {}: {}\n",
                            err.step_id.as_deref().unwrap_or("workflow"),
                            err.message
                        ));
                    }
                }

                Some(Ok(WorkflowChatResult {
                    text: result,
                    tool_calls: workflow_step_tool_call_events(workflow_id, &instance),
                }))
            }
            Err(e) => Some(Err(GatewayError::Internal {
                message: format!("Workflow execution failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })),
        }
    }

    /// 🟢 P1 FIX: Try to match and execute a workflow by natural language
    /// content (e.g. user says "生成今日早报" matches workflow named
    /// "daily_news")
    async fn try_match_workflow_by_content(
        &self,
        content: &str,
    ) -> Option<Result<String, GatewayError>> {
        // Skip commands and very short inputs
        let trimmed = content.trim();
        if trimmed.starts_with('/') || trimmed.len() < 4 {
            return None;
        }

        let lower_content = trimmed.to_lowercase();

        // 🟢 P1 FIX: Negative word filtering — skip if user explicitly rejects or
        // denies
        let negative_words = [
            "不要",
            "不想",
            "别",
            "停止",
            "取消",
            "不需要",
            "不用",
            "不",
            "no ",
            "don't ",
            "stop ",
            "cancel ",
            "not ",
            "never ",
            "no need",
            "don't want",
            "stop doing",
            "cancel the",
        ];
        for neg in &negative_words {
            if lower_content.contains(neg) {
                debug!(
                    "Workflow natural-language match skipped due to negative word '{}'",
                    neg.trim()
                );
                return None;
            }
        }

        let registry = self.workflow_registry.as_ref()?;
        let reg = registry.read().await;

        let mut best_match: Option<(&beebotos_agents::workflow::WorkflowDefinition, u32)> = None;
        for def in reg.list_all() {
            let mut score: u32 = 0;
            let lower_name = def.name.to_lowercase();
            let lower_id = def.id.to_lowercase();

            // Exact ID match (highest priority)
            if lower_content == lower_id {
                score = 100;
            }
            // Content contains workflow name (with word-boundary check for ASCII)
            else if Self::is_substring_match(&lower_content, &lower_name) {
                score = 50 + lower_name.len() as u32;
            }
            // Content contains workflow ID (with word-boundary check for ASCII)
            else if Self::is_substring_match(&lower_content, &lower_id) {
                score = 30 + lower_id.len() as u32;
            }
            // Tag match
            else {
                for tag in &def.tags {
                    if Self::is_substring_match(&lower_content, &tag.to_lowercase()) {
                        score = score.max(20);
                    }
                }
            }

            // Only consider workflows with manual trigger for natural language matching
            let has_manual = def.triggers.iter().any(|t| {
                matches!(
                    t.trigger_type,
                    beebotos_agents::workflow::TriggerType::Manual { .. }
                )
            });
            if !has_manual {
                score = 0;
            }

            if score > 0 {
                if best_match.as_ref().map_or(true, |(_, s)| score > *s) {
                    best_match = Some((def, score));
                }
            }
        }

        // Threshold: require at least 20 score (name/id substring match)
        let (def, score) = best_match?;
        if score < 20 {
            return None;
        }

        info!(
            "Natural language workflow match: '{}' -> {} (score: {})",
            trimmed, def.id, score
        );

        // Execute matched workflow
        let skill_registry = match self.skill_registry.as_ref() {
            Some(r) => r.clone(),
            None => {
                return Some(Err(GatewayError::service_unavailable(
                    "SkillRegistry",
                    "Not initialized",
                )))
            }
        };

        let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> = Arc::new(
            crate::services::agent_runtime_manager::GatewayLLMInterface::new(
                self.llm_service.clone(),
            ),
        );

        let mut agent = beebotos_agents::AgentBuilder::new("workflow-runner")
            .description("Temporary agent for workflow execution")
            .build()
            .with_skill_registry(skill_registry)
            .with_llm_interface(llm_interface);
        if let Some(ref mcp_manager) = self.mcp_manager {
            agent = agent.with_mcp(mcp_manager.clone());
        }

        let engine = beebotos_agents::workflow::WorkflowEngine::new();
        let trigger_context = serde_json::json!({
            "trigger_type": "natural_language",
            "matched_text": trimmed,
            "workflow_id": def.id,
            "match_score": score
        });

        match engine.execute(def, &agent, trigger_context, None).await {
            Ok(instance) => {
                let status = instance.status.to_string();
                let mut result = format!(
                    "✅ Workflow '{}' completed with status: {}\n\n",
                    def.id, status
                );
                for (step_id, step_state) in &instance.step_states {
                    result.push_str(&format!(
                        "- **{}**: {} ({}s)\n",
                        step_id,
                        step_state.status,
                        step_state.duration_secs()
                    ));
                    if let Some(ref output) = step_state.output {
                        let output_str = match output {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Null => String::new(),
                            _ => serde_json::to_string_pretty(output).unwrap_or_default(),
                        };
                        if !output_str.is_empty() && output_str.len() < 5000 {
                            result.push_str(&format!("  - Output: {}\n", output_str));
                        } else if output_str.len() >= 5000 {
                            result.push_str(&format!(
                                "  - Output: {}... (truncated)\n",
                                &output_str[..500]
                            ));
                        }
                    }
                    if let Some(ref err) = step_state.error {
                        result.push_str(&format!("  - Error: {}\n", err));
                    }
                }
                if !instance.error_log.is_empty() {
                    result.push_str("\n**Errors:**\n");
                    for err in &instance.error_log {
                        result.push_str(&format!(
                            "- {}: {}\n",
                            err.step_id.as_deref().unwrap_or("workflow"),
                            err.message
                        ));
                    }
                }
                Some(Ok(result))
            }
            Err(e) => Some(Err(GatewayError::Internal {
                message: format!("Workflow execution failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })),
        }
    }

    /// 🟢 P1 FIX: Check if pattern matches content with word-boundary
    /// awareness. For ASCII text, uses regex word boundaries to avoid
    /// substring matches like "news" matching inside "newspaper". For
    /// non-ASCII (e.g. Chinese), falls back to simple contains.
    fn is_substring_match(content: &str, pattern: &str) -> bool {
        if pattern.is_empty() {
            return false;
        }
        // Fast path: exact match
        if content == pattern {
            return true;
        }
        // For ASCII-only patterns, enforce word boundaries to reduce false positives
        let is_ascii = pattern.chars().all(|c| c.is_ascii());
        if is_ascii {
            // Use regex \b for word boundaries; escape regex metacharacters in pattern
            let escaped = regex::escape(pattern);
            let re_str = format!(r"\b{}\b", escaped);
            if let Ok(re) = regex::Regex::new(&re_str) {
                return re.is_match(content);
            }
        }
        // Fallback for non-ASCII or regex compilation failure
        content.contains(pattern)
    }

    /// P2 FIX: 提取共享的 Memory 搜索逻辑，消除双重搜索
    ///
    /// 🟢 P2 FIX: 返回 (memory_context, direct_answer)。如果 Memory
    /// 中有高置信度的精确匹配问答对， 直接提取答案返回，跳过 LLM 调用。
    ///
    /// 🆕 FIX (方案B): 固定档案与动态记忆分独立预算，简单查询可跳过冗余档案
    async fn build_memory_context(
        &self,
        content: &str,
        skill_match: &Option<(String, String, String, String)>,
    ) -> (String, Option<String>) {
        let mut memory_context = String::new();
        let mut direct_answer: Option<String> = None;

        // 🆕 FIX: 根据 query 复杂度动态调整参数
        // 🆕 SKILL MATCHING V2: Removed hardcoded skill-based profile skipping
        // (travel_planner/weather). All skills now receive consistent context;
        // the Agent's LLM decides what to use.
        let char_count = content.chars().count();
        let is_simple = char_count <= 10;
        let is_complex = char_count > 30
            || content.contains("计划")
            || content.contains("规划")
            || content.contains("步骤")
            || content.contains("安排")
            || content.contains("攻略")
            || content.contains("对比")
            || content.contains("分析")
            || content.contains("总结");
        let search_limit = if is_complex {
            6
        } else if char_count > 15 {
            4
        } else {
            2
        };

        // 🆕 FIX (方案B): 独立预算体系
        // 简单查询：固定档案 300 chars + 动态记忆 400 chars
        // 普通查询：固定档案 600 chars + 动态记忆 800 chars
        // 复杂查询：固定档案 1000 chars + 动态记忆 1200 chars
        let (system_budget, dynamic_budget): (usize, usize) = if is_simple {
            (300, 400)
        } else if is_complex {
            (1000, 1200)
        } else {
            (600, 800)
        };
        // 🆕 FIX: 当外部注入了大段 skill prompt 等额外 context 时，相应缩减 dynamic
        // memory budget
        let extra_context_len = skill_match.as_ref().map_or(0, |(_, name, _, prompt)| {
            let wrapper_len = format!(
                "\n\n[系统提示：你当前正在使用 {} 技能处理此请求。请遵循以下专业指引]\n",
                name
            )
            .chars()
            .count();
            prompt.chars().count() + wrapper_len
        });
        let adjusted_dynamic_budget = dynamic_budget.saturating_sub(extra_context_len).max(150);

        // 🆕 FIX: 前缀文本长度预扣，确保各段总长度（含前缀）不超预算
        let system_prefix =
            "\n\n[系统提示：以下是该用户的固定档案和AI人格设定，回答时必须始终遵守]\n";
        let dynamic_prefix = "\n\n[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n";
        let system_context_budget = system_budget.saturating_sub(system_prefix.chars().count());
        let dynamic_context_budget =
            adjusted_dynamic_budget.saturating_sub(dynamic_prefix.chars().count());

        // 🆕 FIX: 预加载 USER.md 和 SOUL.md 作为固定系统上下文
        if let Some(ref memory) = self.memory_system {
            // Skip fixed profiles entirely when system_budget is 0 (e.g. travel_planner)
            if system_budget > 0 {
                let storage = memory.storage();
                let mut system_context = String::new();

                if is_simple {
                    // 🆕 FIX: 极简模式也加载核心用户档案（名字、语言偏好等关键字段）
                    // 先加载 USER.md 的前 2 条有效关键信息
                    if let Ok(entries) = storage
                        .read_entries(beebotos_agents::memory::MemoryFileType::User, None)
                        .await
                    {
                        let mut user_parts = Vec::new();
                        for entry in entries {
                            let trimmed = entry.content.trim();
                            let is_placeholder = trimmed.contains("*To be filled")
                                || trimmed.starts_with("- Name:") && trimmed.len() < 12
                                || trimmed.starts_with("- Preferred language:")
                                    && trimmed.len() < 25
                                || trimmed.starts_with("- Timezone:") && trimmed.len() < 15
                                || trimmed.starts_with("- Communication style:")
                                    && trimmed.len() < 26
                                || trimmed.starts_with("- Notification preferences:")
                                    && trimmed.len() < 31
                                || trimmed.starts_with("- Professional background:")
                                    && trimmed.len() < 30
                                || trimmed.starts_with("- Technical skills:") && trimmed.len() < 23
                                || trimmed.starts_with("- Hobbies:") && trimmed.len() < 14;
                            if !trimmed.is_empty() && !is_placeholder {
                                user_parts.push(trimmed.to_string());
                                if user_parts.len() >= 1 {
                                    break;
                                } // 🆕 FIX: 简单模式只取1条最关键档案，给SOUL.
                                  // md留空间
                            }
                        }
                        if !user_parts.is_empty() {
                            system_context.push_str("## 用户档案\n");
                            for part in &user_parts {
                                system_context.push_str(&part);
                                system_context.push('\n');
                            }
                            info!(
                                "📄 Simple query mode: loaded USER.md core profile ({} entries)",
                                user_parts.len()
                            );
                        }
                    }

                    // 再加载 SOUL.md 的第一句核心人格描述
                    if let Ok(entries) = storage
                        .read_entries(beebotos_agents::memory::MemoryFileType::Soul, None)
                        .await
                    {
                        for entry in entries {
                            let trimmed = entry.content.trim();
                            if !trimmed.is_empty()
                                && !trimmed.starts_with('#')
                                && !trimmed.starts_with("---")
                            {
                                let first_line = trimmed.lines().next().unwrap_or(trimmed);
                                if first_line.len() > 10 {
                                    if !system_context.is_empty() {
                                        system_context.push('\n');
                                    }
                                    system_context.push_str("## AI 人格设定\n");
                                    system_context.push_str(first_line);
                                    system_context.push('\n');
                                    break;
                                }
                            }
                        }
                    }
                    if system_context.is_empty() {
                        system_context = "You are a helpful assistant for BeeBotOS. Answer the \
                                          user in a friendly and concise manner.\n"
                            .to_string();
                    }
                    info!(
                        "📄 Simple query mode: loaded minimal persona ({} chars)",
                        system_context.chars().count()
                    );
                } else {
                    // 标准模式：加载 USER.md + SOUL.md
                    // Read USER.md
                    match storage
                        .read_entries(beebotos_agents::memory::MemoryFileType::User, None)
                        .await
                    {
                        Ok(entries) => {
                            let mut user_parts = Vec::new();
                            for entry in entries {
                                let trimmed = entry.content.trim();
                                let is_placeholder = trimmed.contains("*To be filled")
                                    || trimmed.starts_with("- Name:") && trimmed.len() < 12
                                    || trimmed.starts_with("- Preferred language:")
                                        && trimmed.len() < 25
                                    || trimmed.starts_with("- Timezone:") && trimmed.len() < 15
                                    || trimmed.starts_with("- Communication style:")
                                        && trimmed.len() < 26
                                    || trimmed.starts_with("- Notification preferences:")
                                        && trimmed.len() < 31
                                    || trimmed.starts_with("- Professional background:")
                                        && trimmed.len() < 30
                                    || trimmed.starts_with("- Technical skills:")
                                        && trimmed.len() < 23
                                    || trimmed.starts_with("- Hobbies:") && trimmed.len() < 14;
                                if !trimmed.is_empty() && !is_placeholder {
                                    user_parts.push(trimmed.to_string());
                                }
                            }
                            if !user_parts.is_empty() {
                                system_context.push_str("## 用户档案\n");
                                for part in &user_parts {
                                    system_context.push_str(&part);
                                    system_context.push('\n');
                                }
                                system_context.push('\n');
                                info!("📄 Loaded USER.md profile ({} entries)", user_parts.len());
                            } else {
                                info!("📄 USER.md loaded but no valid entries after filtering");
                            }
                        }
                        Err(e) => {
                            warn!("📄 Failed to load USER.md: {}", e);
                        }
                    }

                    // Read SOUL.md
                    match storage
                        .read_entries(beebotos_agents::memory::MemoryFileType::Soul, None)
                        .await
                    {
                        Ok(entries) => {
                            let mut soul_parts = Vec::new();
                            for entry in entries {
                                let trimmed = entry.content.trim();
                                let is_placeholder = trimmed.contains("Helpful and friendly")
                                    && trimmed.len() < 30
                                    || trimmed.starts_with("- Professional but approachable")
                                        && trimmed.len() < 35
                                    || trimmed.starts_with("- Detail-oriented")
                                        && trimmed.len() < 20
                                    || trimmed.starts_with("- Clear and concise")
                                        && trimmed.len() < 22
                                    || trimmed.starts_with("- Use examples when helpful")
                                        && trimmed.len() < 30
                                    || trimmed
                                        .starts_with("- Ask clarifying questions when needed")
                                        && trimmed.len() < 42
                                    || trimmed.starts_with("- Respect user privacy")
                                        && trimmed.len() < 25
                                    || trimmed.starts_with("- Decline harmful requests")
                                        && trimmed.len() < 30
                                    || trimmed.starts_with("- Be honest about limitations")
                                        && trimmed.len() < 32;
                                if !trimmed.is_empty() && !is_placeholder {
                                    soul_parts.push(trimmed.to_string());
                                }
                            }
                            if !soul_parts.is_empty() {
                                system_context.push_str("## AI 人格设定\n");
                                for part in &soul_parts {
                                    system_context.push_str(&part);
                                    system_context.push('\n');
                                }
                                system_context.push('\n');
                                info!("📄 Loaded SOUL.md profile ({} entries)", soul_parts.len());
                            } else {
                                info!("📄 SOUL.md loaded but no valid entries after filtering");
                            }
                        }
                        Err(e) => {
                            warn!("📄 Failed to load SOUL.md: {}", e);
                        }
                    }
                }

                // 🆕 FIX (方案B): 对固定档案做硬截断（统一字符计数，已预扣前缀长度）
                if !system_context.is_empty() {
                    let system_chars = system_context.chars().count();
                    if system_chars > system_context_budget {
                        let suffix = "\n...（档案已精简）\n";
                        let suffix_len = suffix.chars().count();
                        let truncate_limit = system_context_budget.saturating_sub(suffix_len);

                        let mut truncated = String::new();
                        let mut char_count = 0;
                        for ch in system_context.chars() {
                            if char_count >= truncate_limit {
                                break;
                            }
                            truncated.push(ch);
                            char_count += 1;
                        }
                        truncated.push_str(suffix);
                        system_context = truncated;

                        debug_assert!(
                            system_context.chars().count() <= system_context_budget,
                            "System context truncation failed: {} > {}",
                            system_context.chars().count(),
                            system_context_budget
                        );
                        info!(
                            "📄 System context truncated to {} chars (budget={})",
                            system_context.chars().count(),
                            system_budget
                        );
                    }
                    memory_context.push_str(system_prefix);
                    memory_context.push_str(&system_context);
                }
            } // end if system_budget > 0

            match memory.search(content, search_limit).await {
                Ok(results) if !results.is_empty() => {
                    info!(
                        "Memory search returned {} results (limit={}) for query '{}'",
                        results.len(),
                        search_limit,
                        content.chars().take(40).collect::<String>()
                    );
                    let content_lower = content.to_lowercase().trim().to_string();

                    // 🟢 P2 FIX: 检查是否有精确问答对可直接返回
                    for r in &results {
                        let mem_lower = r.entry.content.to_lowercase();
                        if mem_lower.contains(&content_lower) {
                            for marker in &["assistant:", "答：", "a:", "回答：", "助手："]
                            {
                                if let Some(pos) = mem_lower.find(marker) {
                                    let answer =
                                        r.entry.content[pos + marker.len()..].trim().to_string();
                                    if answer.chars().count() > 5 && answer.chars().count() < 500 {
                                        info!(
                                            "🧠 P2 MEMORY DIRECT HIT: 精确匹配，直接返回答案 ({} \
                                             chars)",
                                            answer.chars().count()
                                        );
                                        direct_answer = Some(answer);
                                        break;
                                    }
                                }
                            }
                            if direct_answer.is_some() {
                                break;
                            }
                        }
                    }

                    let filtered: Vec<_> = results
                        .iter()
                        .filter(|r| !r.entry.content.to_lowercase().contains(&content_lower))
                        .take(search_limit)
                        .collect();
                    if !filtered.is_empty() {
                        memory_context.push_str(dynamic_prefix);
                        // 🆕 FIX (方案B): 动态记忆独立预算，从 0 开始计算（已预扣前缀长度）
                        // 🆕 FIX: 单条记忆最多 200 chars，避免一条超长记忆占满 budget
                        const MAX_ENTRY_LEN: usize = 200;
                        let mut total_chars = 0;
                        for r in filtered {
                            let mut entry_text = r.entry.content.clone();
                            let entry_text_chars = entry_text.chars().count();
                            if entry_text_chars > MAX_ENTRY_LEN {
                                let mut truncated = String::new();
                                let mut char_count = 0;
                                for ch in entry_text.chars() {
                                    if char_count >= MAX_ENTRY_LEN - 3 {
                                        // 留 3 字符给 "..."
                                        break;
                                    }
                                    truncated.push(ch);
                                    char_count += 1;
                                }
                                truncated.push_str("...");
                                entry_text = truncated;
                            }
                            let entry = format!("- {}\n", entry_text);
                            let entry_chars = entry.chars().count();
                            if total_chars + entry_chars > dynamic_context_budget {
                                memory_context.push_str("- ...（更多记忆已省略）\n");
                                break;
                            }
                            memory_context.push_str(&entry);
                            total_chars += entry_chars;
                        }
                        info!(
                            "Injecting memory context ({} chars, system_budget={}, \
                             dynamic_budget={}) into LLM prompt",
                            memory_context.chars().count(),
                            system_budget,
                            adjusted_dynamic_budget
                        );
                    } else {
                        info!("All memory results were self-referential, skipping injection");
                    }
                }
                Ok(_) => {
                    info!(
                        "Memory search returned no results for query '{}'",
                        content.chars().take(40).collect::<String>()
                    );
                }
                Err(e) => {
                    warn!("Memory search failed: {}", e);
                }
            }
        }

        // 🆕 FIX: 统一注入 skill prompt（无论 memory_system 是否存在）
        if let Some((_, ref skill_name, _, ref skill_prompt)) = skill_match {
            if !skill_prompt.is_empty() {
                let injection = format!("\n\n[{}]\n{}", skill_name, skill_prompt);
                memory_context.push_str(&injection);
                info!(
                    "🎯 Skill prompt injected ({} chars) for '{}'",
                    skill_prompt.chars().count(),
                    skill_name
                );
            }
        }

        // 🆕 FIX: 总预算防御性截断
        let total_budget = system_budget + dynamic_budget;
        let current_chars = memory_context.chars().count();
        if current_chars > total_budget {
            let suffix = "\n...[上下文已精简]\n";
            let keep_chars = total_budget.saturating_sub(suffix.chars().count());
            memory_context = Self::truncate_to_chars(&memory_context, keep_chars);
            memory_context.push_str(suffix);
            warn!(
                "🎯 Total memory context truncated from {} to {} chars (total_budget={})",
                current_chars,
                memory_context.chars().count(),
                total_budget
            );
        }

        (memory_context, direct_answer)
    }

    /// 调用 LLM 并传入上下文
    async fn call_llm_with_context(
        &self,
        message: &Message,
        history: &[SessionMessage],
        _images: &[ProcessedImage],
        memory_context: &str,
    ) -> Result<String, GatewayError> {
        // 构建包含历史和记忆的提示
        let mut context = String::new();

        if !memory_context.is_empty() {
            context.push_str("以下是与当前对话相关的历史记忆，供你参考：\n");
            context.push_str(memory_context);
            context.push_str("\n\n");
        }

        for msg in history.iter().take(history.len().saturating_sub(1)) {
            let role = match msg.role.as_str() {
                "user" => "用户",
                "assistant" => "助手",
                _ => &msg.role,
            };
            context.push_str(&format!("{}: {}\n", role, msg.content));
        }

        // 当前消息
        context.push_str(&format!("用户: {}\n", message.content));

        info!("🤖 调用 LLM，上下文长度: {} 字符", context.len());

        // P1 FIX: 实际使用构建的 context，而非忽略它
        let mut contextual_message = message.clone();
        contextual_message.content = context;
        self.llm_service.process_message(&contextual_message).await
    }

    /// 发送回复
    async fn send_reply(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        response: &str,
    ) -> Result<(), GatewayError> {
        // 检查回复中是否包含图片标记
        if response.contains("![") && response.contains("](") {
            // 需要发送图文混合消息
            self.send_mixed_message(platform, channel_id, original, response)
                .await
        } else {
            // 纯文本回复
            let reply = Message {
                id: Uuid::new_v4(),
                thread_id: original.thread_id,
                platform,
                message_type: MessageType::Text,
                content: response.to_string(),
                metadata: HashMap::new(),
                timestamp: chrono::Utc::now(),
            };

            if let Some(channel) = self
                .channel_registry
                .get_channel_by_platform(platform)
                .await
            {
                channel
                    .read()
                    .await
                    .send(channel_id, &reply)
                    .await
                    .map_err(|e| GatewayError::Internal {
                        message: format!("Failed to send reply: {}", e),
                        correlation_id: Uuid::new_v4().to_string(),
                    })?;

                info!("✅ 回复已发送到 {:?} 频道 {}", platform, channel_id);
            }

            Ok(())
        }
    }

    /// 发送图文混合消息
    async fn send_mixed_message(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        response: &str,
    ) -> Result<(), GatewayError> {
        // 提取文本和图片
        let parts = self.parse_mixed_content(response);

        for part in parts {
            match part {
                MessagePart::Text(text) => {
                    let reply = Message {
                        id: Uuid::new_v4(),
                        thread_id: original.thread_id,
                        platform,
                        message_type: MessageType::Text,
                        content: text,
                        metadata: HashMap::new(),
                        timestamp: chrono::Utc::now(),
                    };

                    if let Some(channel) = self
                        .channel_registry
                        .get_channel_by_platform(platform)
                        .await
                    {
                        if let Err(e) = channel.read().await.send(channel_id, &reply).await {
                            error!("发送文本消息失败: {}", e);
                        }
                    }
                }
                MessagePart::Image { data, mime_type } => {
                    // 发送图片
                    self.send_image(platform, channel_id, original, &data, &mime_type)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// 解析混合内容
    fn parse_mixed_content(&self, content: &str) -> Vec<MessagePart> {
        let mut parts = Vec::new();
        let mut last_end = 0;

        // 匹配 markdown 图片 ![alt](url)
        // 使用lazy_static避免重复编译正则表达式
        static IMAGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = IMAGE_RE.get_or_init(|| {
            regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("Invalid regex pattern")
        });

        for cap in re.captures_iter(content) {
            let full_match = match cap.get(0) {
                Some(m) => m,
                None => continue,
            };
            let start = full_match.start();
            let end = full_match.end();

            // 添加前面的文本
            if start > last_end {
                let text = content[last_end..start].trim();
                if !text.is_empty() {
                    parts.push(MessagePart::Text(text.to_string()));
                }
            }

            // 添加图片
            let url = &cap[2];
            if url.starts_with("data:image") {
                // base64 编码的图片
                if let Some((mime_type, data)) = self.parse_data_url(url) {
                    parts.push(MessagePart::Image { data, mime_type });
                }
            }

            last_end = end;
        }

        // 添加剩余文本
        if last_end < content.len() {
            let text = content[last_end..].trim();
            if !text.is_empty() {
                parts.push(MessagePart::Text(text.to_string()));
            }
        }

        parts
    }

    /// 解析 data URL
    fn parse_data_url(&self, url: &str) -> Option<(String, String)> {
        // data:image/png;base64,xxxx
        let prefix = "data:image/";
        if !url.starts_with(prefix) {
            return None;
        }

        let rest = &url[prefix.len()..];
        let semi_pos = rest.find(';')?;
        let comma_pos = rest.find(',')?;

        let format = &rest[..semi_pos];
        let data = &rest[comma_pos + 1..];

        Some((format!("image/{}", format), data.to_string()))
    }

    /// 发送图片
    async fn send_image(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        image_data: &str,
        mime_type: &str,
    ) -> Result<(), GatewayError> {
        // 解码 base64
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;

        let data = STANDARD
            .decode(image_data)
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to decode image: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 创建图片消息
        let mut metadata = HashMap::new();
        metadata.insert("image_data".to_string(), image_data.to_string());
        metadata.insert("mime_type".to_string(), mime_type.to_string());

        let reply = Message {
            id: Uuid::new_v4(),
            thread_id: original.thread_id,
            platform,
            message_type: MessageType::Image,
            content: format!("[图片] {} bytes", data.len()),
            metadata,
            timestamp: chrono::Utc::now(),
        };

        if let Some(channel) = self
            .channel_registry
            .get_channel_by_platform(platform)
            .await
        {
            channel
                .read()
                .await
                .send(channel_id, &reply)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to send image: {}", e),
                    correlation_id: Uuid::new_v4().to_string(),
                })?;

            info!("✅ 图片已发送到 {:?} 频道 {}", platform, channel_id);
        }

        Ok(())
    }

    /// 🆕 FIX: 按字符截断字符串
    fn truncate_to_chars(s: &str, limit: usize) -> String {
        let mut result = String::new();
        let mut count = 0;
        for ch in s.chars() {
            if count >= limit {
                break;
            }
            result.push(ch);
            count += 1;
        }
        result
    }

    /// 🆕 FIX: Fetch real-time weather data from wttr.in (free, no API key
    /// required)
    async fn fetch_weather_data(city: &str) -> Option<String> {
        let url = format!("https://wttr.in/{}?format=%C|%t|%h|%w|%p", city);
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() && !trimmed.contains("Unknown location") {
                        info!("🌤️ Weather data fetched for {}: {}", city, trimmed);
                        return Some(trimmed.to_string());
                    }
                }
                Err(e) => warn!("Failed to read weather response: {}", e),
            },
            Ok(resp) => warn!("Weather API returned status: {}", resp.status()),
            Err(e) => warn!("Weather API request failed: {}", e),
        }
        None
    }

    /// 🆕 FIX: Extract city name from a weather query (e.g. "深圳天气怎么样" ->
    /// "深圳")
    fn extract_city_from_weather_query(query: &str) -> Option<String> {
        // Match patterns like "XX市天气", "XX天气", "今天XX天气"
        let re = Regex::new(r"今天?的?(.*?)(?:市)?(?:的)?天气").ok()?;
        if let Some(cap) = re.captures(query) {
            if let Some(city) = cap.get(1) {
                let city = city.as_str().trim();
                if !city.is_empty() {
                    return Some(city.to_string());
                }
            }
        }
        // Fallback: try to find city names from a common list
        let common_cities = [
            "北京",
            "上海",
            "广州",
            "深圳",
            "杭州",
            "南京",
            "成都",
            "重庆",
            "武汉",
            "西安",
            "天津",
            "苏州",
            "长沙",
            "郑州",
            "沈阳",
            "青岛",
            "宁波",
            "东莞",
            "无锡",
            "佛山",
            "合肥",
            "大连",
            "福州",
            "厦门",
            "哈尔滨",
            "济南",
            "温州",
            "南宁",
            "长春",
            "泉州",
            "石家庄",
            "贵阳",
            "南昌",
            "金华",
            "常州",
            "嘉兴",
            "珠海",
            "惠州",
            "中山",
            "江门",
            "兰州",
            "海口",
            "三亚",
            "乌鲁木齐",
            "呼和浩特",
            "银川",
            "西宁",
            "拉萨",
            "昆明",
            "太原",
        ];
        for city in &common_cities {
            if query.contains(city) {
                return Some(city.to_string());
            }
        }
        None
    }

    /// 🆕 FIX: 评估查询复杂度
    fn estimate_query_complexity(query: &str) -> QueryComplexity {
        let len = query.chars().count();
        let complex_keywords = [
            "计划", "规划", "分析", "对比", "步骤", "方案", "周", "预算", "攻略", "安排", "行程",
        ];
        let keyword_score = complex_keywords
            .iter()
            .filter(|k| query.contains(**k))
            .count();

        if len > 15 || keyword_score >= 2 {
            QueryComplexity::High
        } else if len > 8 || keyword_score >= 1 {
            QueryComplexity::Medium
        } else {
            QueryComplexity::Low
        }
    }
}

/// 查询复杂度等级，用于判断 Skill Planning 是否需要启用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryComplexity {
    /// 简单短查询，如 "hi"、"你好"
    Low,
    /// 中等查询，含一个复杂关键词或长度稍长
    Medium,
    /// 复杂查询，含多个关键词或长句，需要多步规划
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryPromotionAction {
    Add,
    Replace,
    Remove,
    Ignore,
}

impl MemoryPromotionAction {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "add" => Some(Self::Add),
            "replace" | "update" => Some(Self::Replace),
            "remove" | "delete" | "forget" => Some(Self::Remove),
            "ignore" | "none" => Some(Self::Ignore),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MemoryPromotionDecision {
    action: String,
    target: Option<String>,
    title: Option<String>,
    content: Option<String>,
    category: Option<String>,
    importance: Option<f32>,
    confidence: Option<f32>,
    replace_match: Option<String>,
}

#[derive(Debug, Clone)]
struct PromotedMemoryFact {
    action: MemoryPromotionAction,
    file_type: MemoryFileType,
    title: String,
    content: String,
    category: String,
    importance: f32,
    replace_match: Option<String>,
}

/// 处理后的图片
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    pub data: String,
    pub format: ImageFormat,
    pub mime_type: String,
}

/// 图片格式
#[derive(Debug, Clone)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageFormat {
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
        }
    }
}

fn merge_tool_call_snapshots(
    stream_calls: Vec<serde_json::Value>,
    trace_calls: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    let mut calls = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for call in stream_calls.into_iter().chain(trace_calls) {
        let key = serde_json::json!({
            "round": call.get("round").cloned().unwrap_or(serde_json::Value::Null),
            "tool_name": call.get("tool_name").cloned().unwrap_or(serde_json::Value::Null),
            "arguments": call.get("arguments").cloned().unwrap_or(serde_json::Value::Null),
            "status": call.get("status").cloned().unwrap_or(serde_json::Value::Null),
        })
        .to_string();
        if seen.insert(key) {
            calls.push(call);
        }
    }
    calls
}

/// 消息部分
enum MessagePart {
    Text(String),
    Image { data: String, mime_type: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_user_name_to_user_profile() {
        let facts = MessageProcessor::extract_promoted_memory_facts("我叫齐世浩，记住我");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].file_type, MemoryFileType::User);
        assert_eq!(facts[0].title, "Basic Information");
        assert_eq!(facts[0].content, "- Name: 齐世浩");
    }

    #[test]
    fn promotes_basketball_to_user_profile() {
        let facts = MessageProcessor::extract_promoted_memory_facts("我平常经常打篮球");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].file_type, MemoryFileType::User);
        assert_eq!(facts[0].title, "Interests");
        assert_eq!(facts[0].content, "- Interests: 经常打篮球");
    }

    #[test]
    fn promotes_shell_to_agent_memory() {
        let facts = MessageProcessor::extract_promoted_memory_facts("我电脑使用的shell环境是fish");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].file_type, MemoryFileType::Core);
        assert_eq!(facts[0].title, "Local Environment");
        assert_eq!(facts[0].content, "- Local shell: fish");
    }

    #[test]
    fn parses_llm_user_fact_from_json_fence() {
        let facts = MessageProcessor::parse_promoted_memory_facts(
            r#"```json
            {
              "decisions": [
                {
                  "action": "add",
                  "target": "user",
                  "title": "Interests",
                  "content": "- Interests: 最近迷上羽毛球",
                  "category": "profile",
                  "importance": 0.8,
                  "confidence": 0.91
                }
              ]
            }
            ```"#,
        );

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].action, MemoryPromotionAction::Add);
        assert_eq!(facts[0].file_type, MemoryFileType::User);
        assert_eq!(facts[0].content, "- Interests: 最近迷上羽毛球");
    }

    #[test]
    fn parses_llm_ignore_as_no_fact() {
        let facts = MessageProcessor::parse_promoted_memory_facts(
            r#"{"decisions":[{"action":"ignore","confidence":0.95,"reason":"transient event"}]}"#,
        );

        assert!(facts.is_empty());
    }

    #[test]
    fn parses_llm_replace_fact() {
        let facts = MessageProcessor::parse_promoted_memory_facts(
            r#"{
              "decisions": [
                {
                  "action": "replace",
                  "target": "user",
                  "title": "Interests",
                  "content": "- Interests: 现在主要打羽毛球",
                  "category": "profile",
                  "importance": 0.85,
                  "confidence": 0.93,
                  "replace_match": "打篮球"
                }
              ]
            }"#,
        );

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].action, MemoryPromotionAction::Replace);
        assert_eq!(facts[0].file_type, MemoryFileType::User);
        assert_eq!(facts[0].replace_match.as_deref(), Some("打篮球"));
    }

    #[test]
    fn merges_stream_and_trace_tool_calls() {
        let calls = merge_tool_call_snapshots(
            vec![serde_json::json!({
                "round": 1,
                "tool_name": "mcp_tool_search",
                "arguments": {"q": "time"},
                "status": "started"
            })],
            vec![serde_json::json!({
                "round": 2,
                "tool_name": "get_time",
                "arguments": {"timezone": "Asia/Shanghai"},
                "status": "started"
            })],
        );

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["tool_name"], "mcp_tool_search");
        assert_eq!(calls[1]["tool_name"], "get_time");
    }
}
