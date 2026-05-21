//! WebChat Service
//!
//! Unified chat persistence and session management for all channels
//! (webchat, personal_wechat, lark, dingtalk, qq, feishu, etc.).

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use tracing::{error, info};

use crate::error::AppError;

/// Chat session model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub title: String,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// SQLite-compatible row for ChatSession
#[derive(sqlx::FromRow)]
struct ChatSessionRow {
    id: String,
    user_id: String,
    channel: String,
    title: String,
    is_pinned: i32,
    is_archived: i32,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ChatSessionRow> for ChatSession {
    type Error = String;

    fn try_from(row: ChatSessionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            user_id: row.user_id,
            channel: row.channel,
            title: row.title,
            is_pinned: row.is_pinned != 0,
            is_archived: row.is_archived != 0,
            created_at: parse_sqlite_datetime(&row.created_at)?,
            updated_at: parse_sqlite_datetime(&row.updated_at)?,
        })
    }
}

fn parse_sqlite_datetime(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = value.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }

    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc())
        .map_err(|e| format!("Invalid datetime '{}': {}", value, e))
}

/// Chat message model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Value,
    pub token_usage: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// Search result from persisted session history.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatSessionSearchResult {
    pub message_id: String,
    pub session_id: String,
    pub session_title: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub score: f32,
}

/// SQLite-compatible row for ChatMessage
#[derive(sqlx::FromRow)]
struct ChatMessageRow {
    id: String,
    session_id: String,
    role: String,
    content: String,
    metadata: String,
    token_usage: Option<String>,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct ChatSessionSearchRow {
    message_id: String,
    session_id: String,
    session_title: String,
    role: String,
    content: String,
    created_at: String,
    rank: f64,
}

impl TryFrom<ChatMessageRow> for ChatMessage {
    type Error = String;

    fn try_from(row: ChatMessageRow) -> Result<Self, Self::Error> {
        let message_id = row.id.clone();
        Ok(Self {
            id: row.id,
            session_id: row.session_id,
            role: row.role,
            content: row.content,
            metadata: serde_json::from_str(&row.metadata).unwrap_or_else(|e| {
                tracing::warn!(
                    "Invalid WebChat metadata JSON for message {}; using empty metadata: {}",
                    message_id,
                    e
                );
                serde_json::json!({})
            }),
            token_usage: row
                .token_usage
                .map(|s| {
                    serde_json::from_str(&s).map_err(|e| {
                        tracing::warn!(
                            "Invalid WebChat token_usage JSON for message {}; ignoring it: {}",
                            message_id,
                            e
                        );
                        e
                    })
                })
                .transpose()
                .ok()
                .flatten(),
            created_at: parse_sqlite_datetime(&row.created_at)?,
        })
    }
}

impl TryFrom<ChatSessionSearchRow> for ChatSessionSearchResult {
    type Error = String;

    fn try_from(row: ChatSessionSearchRow) -> Result<Self, Self::Error> {
        Ok(Self {
            message_id: row.message_id,
            session_id: row.session_id,
            session_title: row.session_title,
            role: row.role,
            content: row.content,
            created_at: parse_sqlite_datetime(&row.created_at)?,
            score: 1.0 / (row.rank.abs() as f32 + 1.0),
        })
    }
}

fn build_session_fts_query(query: &str) -> String {
    let mut tokens: Vec<String> = query
        .split_whitespace()
        .filter_map(sanitize_session_fts_token)
        .take(12)
        .collect();
    if tokens.is_empty() {
        if let Some(token) = sanitize_session_fts_token(query) {
            tokens.push(token);
        }
    }
    tokens.join(" OR ")
}

fn sanitize_session_fts_token(token: &str) -> Option<String> {
    let cleaned: String = token
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(format!("{}*", cleaned))
    }
}

/// WebChat service for unified chat management
#[derive(Debug, Clone)]
pub struct WebchatService {
    db: SqlitePool,
}

impl WebchatService {
    /// Create a new WebchatService
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Create a new chat session
    pub async fn create_session(
        &self,
        user_id: &str,
        channel: &str,
        title: &str,
    ) -> Result<ChatSession, AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO chat_sessions (user_id, channel, title, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
        )
        .bind(user_id)
        .bind(channel)
        .bind(title)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to create chat session: {}", e);
            AppError::database(e)
        })?;

        let row: ChatSessionRow =
            sqlx::query_as("SELECT * FROM chat_sessions WHERE user_id = ?1 AND created_at = ?2")
                .bind(user_id)
                .bind(&now)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        let session: ChatSession = row
            .try_into()
            .map_err(|e: String| AppError::Internal(format!("Failed to parse session: {}", e)))?;

        info!("Created chat session {} for user {}", session.id, user_id);
        Ok(session)
    }

    /// Get a single session by ID, verifying ownership
    pub async fn get_session(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<ChatSession, AppError> {
        let row: ChatSessionRow =
            sqlx::query_as("SELECT * FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => AppError::not_found("Session", session_id),
                    _ => AppError::database(e),
                })?;

        row.try_into()
            .map_err(|e: String| AppError::Internal(format!("Failed to parse session: {}", e)))
    }

    /// List sessions for a user, ordered by updated_at desc
    pub async fn list_sessions(&self, user_id: &str) -> Result<Vec<ChatSession>, AppError> {
        let rows: Vec<ChatSessionRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_sessions
            WHERE user_id = ?1 AND is_archived = 0
            ORDER BY is_pinned DESC, updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let sessions: Result<Vec<_>, _> = rows.into_iter().map(|r| r.try_into()).collect();

        sessions.map_err(|e: String| AppError::Internal(format!("Failed to parse sessions: {}", e)))
    }

    /// Get messages for a session, verifying ownership
    pub async fn get_messages(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<Vec<ChatMessage>, AppError> {
        // Verify ownership
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if count == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        let rows: Vec<ChatMessageRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_messages
            WHERE session_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let messages: Result<Vec<_>, _> = rows.into_iter().map(|r| r.try_into()).collect();

        messages.map_err(|e: String| AppError::Internal(format!("Failed to parse messages: {}", e)))
    }

    /// Search persisted messages across a user's sessions.
    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        exclude_session_id: Option<&str>,
    ) -> Result<Vec<ChatSessionSearchResult>, AppError> {
        let fts_query = build_session_fts_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<ChatSessionSearchRow> = sqlx::query_as(
            r#"
            SELECT
                f.message_id,
                f.session_id,
                s.title AS session_title,
                m.role,
                m.content,
                m.created_at,
                bm25(chat_messages_fts) AS rank
            FROM chat_messages_fts f
            JOIN chat_messages m ON m.id = f.message_id
            JOIN chat_sessions s ON s.id = f.session_id
            WHERE chat_messages_fts MATCH ?1
              AND f.user_id = ?2
              AND (?3 IS NULL OR f.session_id != ?3)
            ORDER BY rank ASC, m.created_at DESC
            LIMIT ?4
            "#,
        )
        .bind(&fts_query)
        .bind(user_id)
        .bind(exclude_session_id)
        .bind(limit.clamp(1, 20) as i64)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        rows.into_iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to parse search result: {}", e)))
    }

    /// Save a message to a session
    pub async fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        metadata: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<String, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let metadata_json = metadata
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let token_usage_json = token_usage.map(|v| v.to_string());
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO chat_messages (id, session_id, role, content, metadata, token_usage, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(&metadata_json)
        .bind(token_usage_json.as_deref())
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to save chat message: {}", e);
            AppError::database(e)
        })?;

        Ok(id)
    }

    /// Update session title
    pub async fn update_title(
        &self,
        session_id: &str,
        user_id: &str,
        title: &str,
    ) -> Result<ChatSession, AppError> {
        let result =
            sqlx::query("UPDATE chat_sessions SET title = ?1 WHERE id = ?2 AND user_id = ?3")
                .bind(title)
                .bind(session_id)
                .bind(user_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        let row: ChatSessionRow = sqlx::query_as(
            "SELECT id, user_id, channel, title, is_pinned, is_archived, created_at, updated_at \
             FROM chat_sessions WHERE id = ?1 AND user_id = ?2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        row.try_into().map_err(|e: String| AppError::Internal(e))
    }

    /// Toggle pin status, returning new pin state
    pub async fn toggle_pin(&self, session_id: &str, user_id: &str) -> Result<bool, AppError> {
        let current: Option<i32> = sqlx::query_scalar(
            "SELECT is_pinned FROM chat_sessions WHERE id = ?1 AND user_id = ?2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let current = current.ok_or_else(|| AppError::not_found("Session", session_id))?;
        let new_pinned = if current == 0 { 1 } else { 0 };

        sqlx::query("UPDATE chat_sessions SET is_pinned = ?1 WHERE id = ?2 AND user_id = ?3")
            .bind(new_pinned)
            .bind(session_id)
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::database(e))?;

        Ok(new_pinned != 0)
    }

    /// Archive a session
    pub async fn archive_session(&self, session_id: &str, user_id: &str) -> Result<(), AppError> {
        let result =
            sqlx::query("UPDATE chat_sessions SET is_archived = 1 WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        Ok(())
    }

    /// Delete a session (cascades to messages via FK)
    pub async fn delete_session(&self, session_id: &str, user_id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
            .bind(session_id)
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        info!("Deleted chat session {}", session_id);
        Ok(())
    }

    /// Validate that a session exists and belongs to the given user
    pub async fn validate_session(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<bool, AppError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;
        Ok(count > 0)
    }

    /// Get or create a session for external channels (personal_wechat, lark,
    /// etc.)
    pub async fn get_or_create_channel_session(
        &self,
        user_id: &str,
        channel: &str,
        _sender_id: &str,
    ) -> Result<String, AppError> {
        // Look for existing session for this user + channel
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM chat_sessions WHERE user_id = ?1 AND channel = ?2 LIMIT 1",
        )
        .bind(user_id)
        .bind(channel)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        if let Some(id) = existing {
            return Ok(id);
        }

        // Create new session
        let title = format!("{} Chat", channel);
        let session = self.create_session(user_id, channel, &title).await?;
        info!(
            "Created new channel session {} for {} / {}",
            session.id, channel, user_id
        );
        Ok(session.id)
    }

    /// Mark a message as successfully delivered via WebSocket
    pub async fn mark_ws_delivered(&self, message_id: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE chat_messages SET ws_delivered_at = datetime('now') WHERE id = ?1")
            .bind(message_id)
            .execute(&self.db)
            .await
            .map_err(|e| {
                error!(
                    "Failed to mark message {} as ws_delivered: {}",
                    message_id, e
                );
                AppError::database(e)
            })?;
        Ok(())
    }

    /// Get assistant messages that have not been delivered via WebSocket
    pub async fn get_undelivered_messages(
        &self,
        session_id: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<ChatMessage>, AppError> {
        let rows: Vec<ChatMessageRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_messages
            WHERE session_id = ?1 AND role = 'assistant' AND ws_delivered_at IS NULL AND created_at > ?2
            ORDER BY created_at ASC
            "#
        )
        .bind(session_id)
        .bind(since.to_rfc3339())
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        rows.into_iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(e))
    }

    /// Get the latest assistant message persisted for a specific channel_id
    /// stored in message metadata.
    pub async fn get_latest_assistant_message_by_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<ChatMessage>, AppError> {
        let escaped_channel_id = channel_id.replace('\\', "\\\\").replace('"', "\\\"");
        let pattern = format!("%\"channel_id\":\"{}\"%", escaped_channel_id);
        let row: Option<ChatMessageRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_messages
            WHERE role = 'assistant' AND metadata LIKE ?1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(pattern)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        row.map(|r| r.try_into())
            .transpose()
            .map_err(|e| AppError::Internal(e))
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            r#"
            CREATE TABLE chat_sessions (
                id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
                user_id TEXT NOT NULL,
                channel TEXT NOT NULL DEFAULT 'webchat',
                title TEXT NOT NULL DEFAULT 'New Chat',
                is_pinned INTEGER DEFAULT 0,
                is_archived INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE chat_messages (
                id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
                session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                token_usage TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                ws_delivered_at TEXT DEFAULT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE chat_messages_fts USING fts5(
                message_id UNINDEXED,
                session_id UNINDEXED,
                user_id UNINDEXED,
                role UNINDEXED,
                content,
                created_at UNINDEXED
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TRIGGER chat_messages_ai AFTER INSERT ON chat_messages BEGIN
                INSERT INTO chat_messages_fts(message_id, session_id, user_id, role, content, created_at)
                SELECT NEW.id, NEW.session_id, s.user_id, NEW.role, NEW.content, NEW.created_at
                FROM chat_sessions s WHERE s.id = NEW.session_id;
            END;
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn search_messages_filters_user_and_excludes_current_session() {
        let svc = WebchatService::new(test_pool().await);
        let old_session = svc
            .create_session("user-a", "webchat", "Old")
            .await
            .unwrap();
        let current_session = svc
            .create_session("user-a", "webchat", "Current")
            .await
            .unwrap();
        let other_user_session = svc
            .create_session("user-b", "webchat", "Other")
            .await
            .unwrap();

        svc.save_message(
            &old_session.id,
            "user",
            "Rust memory layering should use sqlite fts",
            None,
            None,
        )
        .await
        .unwrap();
        svc.save_message(
            &current_session.id,
            "user",
            "Rust memory layering from current turn",
            None,
            None,
        )
        .await
        .unwrap();
        svc.save_message(
            &other_user_session.id,
            "user",
            "Rust memory layering from another user",
            None,
            None,
        )
        .await
        .unwrap();

        let hits = svc
            .search_messages(
                "user-a",
                "rust memory layering",
                10,
                Some(&current_session.id),
            )
            .await
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, old_session.id);
        assert_eq!(hits[0].role, "user");
        assert!(hits[0].content.contains("sqlite fts"));
    }
}
