# Agent Memory Layering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate durable Markdown memory from complete session history and expose session history through `session_search`.

**Architecture:** Gateway owns SQLite session search. Agents expose a scoped tool and call Gateway through `SystemInfoProvider`. Conversation writes stop going to `MEMORY.md`.

**Tech Stack:** Rust, SQLx SQLite, SQLite FTS5, existing BeeBotOS agent/runtime abstractions.

---

### Task 1: SessionDB FTS Search

**Files:**
- Create: `migrations_sqlite/021_add_chat_message_fts.sql`
- Modify: `apps/gateway/src/services/webchat_service.rs`

- [ ] Add FTS5 schema and triggers for `chat_messages`.
- [ ] Add `WebchatService::search_messages`.
- [ ] Add tests for match ranking, user isolation, and current-session exclusion.

### Task 2: Cross-Crate Search Provider

**Files:**
- Modify: `crates/agents/src/system_info.rs`
- Modify: `apps/gateway/src/main.rs`

- [ ] Add `SessionSearchHit` and `search_sessions` to `SystemInfoProvider`.
- [ ] Implement provider method in Gateway by delegating to `WebchatService`.

### Task 3: Agent Tool

**Files:**
- Modify: `crates/agents/src/agent_impl.rs`

- [ ] Add `session_search` tool definition.
- [ ] Execute it via `SystemInfoProvider`, scoped by current `user_id`.
- [ ] Format compact results with session id, role, timestamp, and snippet.

### Task 4: Memory Write Split

**Files:**
- Modify: `apps/gateway/src/services/message_processor.rs`

- [ ] Remove ordinary conversation writes to `MemoryFileType::Core`.
- [ ] Keep existing `chat_messages` persistence as the full history source.

### Task 5: Verification

**Commands:**
- `cargo test -p beebotos-gateway webchat_service`
- `cargo test -p beebotos-agents session_search`
- `cargo check -p beebotos-gateway`
