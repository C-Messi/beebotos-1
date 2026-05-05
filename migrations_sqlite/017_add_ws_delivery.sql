-- Add WebSocket delivery tracking for chat messages
-- This enables reliable message delivery via WebSocket with fallback polling

ALTER TABLE chat_messages ADD COLUMN ws_delivered_at TEXT DEFAULT NULL;

CREATE INDEX idx_chat_messages_undelivered ON chat_messages(session_id, ws_delivered_at, created_at)
WHERE ws_delivered_at IS NULL;
