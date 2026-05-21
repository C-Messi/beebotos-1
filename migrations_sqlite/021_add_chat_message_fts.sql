-- Add FTS5 index for complete session message history.

CREATE VIRTUAL TABLE IF NOT EXISTS chat_messages_fts USING fts5(
    message_id UNINDEXED,
    session_id UNINDEXED,
    user_id UNINDEXED,
    role UNINDEXED,
    content,
    created_at UNINDEXED
);

INSERT INTO chat_messages_fts(message_id, session_id, user_id, role, content, created_at)
SELECT m.id, m.session_id, s.user_id, m.role, m.content, m.created_at
FROM chat_messages m
JOIN chat_sessions s ON s.id = m.session_id
WHERE NOT EXISTS (
    SELECT 1 FROM chat_messages_fts f WHERE f.message_id = m.id
);

CREATE TRIGGER IF NOT EXISTS chat_messages_fts_ai AFTER INSERT ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(message_id, session_id, user_id, role, content, created_at)
    SELECT NEW.id, NEW.session_id, s.user_id, NEW.role, NEW.content, NEW.created_at
    FROM chat_sessions s WHERE s.id = NEW.session_id;
END;

CREATE TRIGGER IF NOT EXISTS chat_messages_fts_ad AFTER DELETE ON chat_messages BEGIN
    DELETE FROM chat_messages_fts WHERE message_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS chat_messages_fts_au AFTER UPDATE OF role, content, session_id, created_at ON chat_messages BEGIN
    DELETE FROM chat_messages_fts WHERE message_id = OLD.id;
    INSERT INTO chat_messages_fts(message_id, session_id, user_id, role, content, created_at)
    SELECT NEW.id, NEW.session_id, s.user_id, NEW.role, NEW.content, NEW.created_at
    FROM chat_sessions s WHERE s.id = NEW.session_id;
END;
