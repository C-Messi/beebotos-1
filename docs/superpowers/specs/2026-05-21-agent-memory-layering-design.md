# Agent Memory Layering Design

## Goal

Make BeeBotOS agent memory predictable by separating stable profile/project memory from full historical conversations, while giving agents an explicit tool to search old sessions.

## Architecture

Use the existing Markdown memory files for stable startup context and the existing WebChat SQLite session tables for complete conversation history. `USER.md` stores durable user preferences. `MEMORY.md` stores project, environment, and tool pitfalls. Full user/assistant turns stay in `chat_messages`, indexed by FTS5.

Agents receive a frozen memory snapshot at turn start. Writes made during the turn are persisted to storage only and become visible on later turns. Historical conversation recall is explicit through a `session_search` tool instead of automatic injection from every old message.

## Components

- `USER.md`: long-term user profile and preferences.
- `MEMORY.md`: project, environment, and tool facts.
- `chat_sessions` / `chat_messages`: complete session history.
- `chat_messages_fts`: SQLite FTS5 index over persisted messages.
- `session_search`: built-in agent tool scoped by current `user_id` and current session.

## Data Flow

1. Gateway persists each user and assistant turn to `chat_messages`.
2. FTS triggers keep `chat_messages_fts` in sync.
3. Agent startup/turn setup loads Markdown profile/project context.
4. Agent calls `session_search` only when old conversation context is needed.
5. Current-turn writes are not injected back into the same turn.

## Non-Goals

- No Hermes-style plugin framework.
- No vector search for session history in this phase.
- No automatic summarizer or memory promotion policy in this phase.

## Testing

Cover FTS indexing, user isolation, current-session exclusion, tool formatting, and removal of ordinary conversation writes to `MEMORY.md`.
