# Plan: Tool Work Directory + Direct LLM Tool Invocation

## Background

After thorough codebase investigation, two architectural gaps were identified:

1. **Tool working directory is uncontrolled**: `default_tool_set(skill_dir)` receives a skill-specific directory or `"."`. File tools (`file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `text_grep`) have **zero path sandboxing** — they can read/write any path on the filesystem. Only `ProcessExecTool` and `BashShellTool` enforce `allowed_work_dirs`.

2. **LLM cannot directly invoke底层 tools in main chat**: The main chat flow (`handle_llm_task_internal`) has two tool-invocation paths:
   - **Native function calling** (`call_llm_with_tools` → `chat_with_tools_react`): The loop is complete, but `NativeToolAdapter::execute()` is a **stub that returns errors**. Real tool execution never happens.
   - **Text-based `SKILL:` trigger**: The agent parses `SKILL:<skill_id>|{params}` and calls `execute_registered_skill()` — this executes **registered skills** (WASM/code/knowledge), not底层 tools like `file_write` or `file_glob`.

## Requirement 1: Unified Tool Working Directory `/data/workspace/`

All file operations and command executions should default to `/data/workspace/`.

### Changes needed:
- `Agent` struct: add `tool_work_dir: PathBuf` (default `/data/workspace/`)
- `Agent::with_tool_work_dir()` builder method
- `spawn_sub_agent`: inherit parent's `tool_work_dir`
- All `default_tool_set()` callers use `self.tool_work_dir`
- File tools (`file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `text_grep`): 
  - Resolve relative paths against `tool_work_dir`
  - Reject absolute paths outside `tool_work_dir` (security boundary)
  - Auto-create parent directories under `tool_work_dir`
- Ensure `/data/workspace/` directory exists at runtime

## Requirement 2: LLM Direct Tool Invocation

When user says "写一个1+1=？的python应用" or "查找本机的文件", the LLM should be able to directly decide to call `file_write` or `file_glob`, execute it, see the result, and then respond to the user — without going through a registered skill or ReAct reasoning loop.

---

## Option A: Fix Native Function Calling (Recommended)

Leverage the existing `chat_with_tools_react()` multi-turn loop (already handles `tool_calls` → execute → `Role::Tool` feedback → next LLM call). The only missing piece is the bridge from `ToolCall` to real `SkillTool::execute()`.

### Architecture

```
User: "写一个1+1的python应用"
  ↓
Agent::handle_llm_task_internal()
  ↓ Intent analysis (unchanged)
  ↓ Build native_tools from BOTH registered skills AND底层 tools
  ↓ Agent has llm_client (LLMClient) + tool_set (HashMap<name, SkillTool>)
  ↓
LLMClient::chat_with_tools_react(
    user_message,
    [SkillToolHandler(file_write), SkillToolHandler(file_glob), ...],
    max_rounds=10
)
  ↓
Provider returns tool_calls: [{name:"file_write", arguments:'{"path":"app.py",...}'}]
  ↓
SkillToolHandler::execute() → parse JSON → SkillTool::execute() → "File written"
  ↓
Append Role::Tool message with result → next LLM call
  ↓
Provider returns final answer: "已创建 app.py，内容为 print(1+1)"
```

### Implementation Steps

1. **Create `SkillToolHandler`** (`crates/agents/src/llm/skill_tool_handler.rs`)
   - Implements `ToolHandler` trait
   - Holds `Box<dyn SkillTool>`
   - `definition()` maps `SkillTool::name/description/parameters_schema` → `Tool`
   - `execute(arguments_json)` parses JSON → `serde_json::Value` → `SkillTool::execute()`

2. **Modify `Agent` struct** (`agent_impl.rs`)
   - Add `tool_work_dir: PathBuf` (default `/data/workspace/`)
   - Add `llm_client: Option<Arc<LLMClient>>` — direct access to LLMClient for native tool calling
   - Add `with_llm_client()` builder method
   - `spawn_sub_agent`: inherit both fields

3. **Modify `GatewayAgentRuntime`** (`runtime/agent_runtime_impl.rs`)
   - Add `llm_client: Option<Arc<LLMClient>>` field
   - Add `with_llm_client()` builder method
   - Pass `llm_client` to `Agent` in `spawn_agent()` and `recover_agents()`

4. **Modify `KernelAgentBuilder`** (`kernel_integration.rs`)
   - Add `with_llm_client()` and `with_tool_work_dir()`
   - Pass through to Agent constructor

5. **Modify `Gateway main.rs`**
   - Create `llm_client` alongside `llm_interface`
   - Pass both to `GatewayAgentRuntime`
   - Create `/data/workspace/` directory at startup

6. **Modify `handle_llm_task_internal`** (`agent_impl.rs`)
   - When building `native_tools` (from keyword-scored skills), also append底层 tools from `default_tool_set(&self.tool_work_dir)`
   - Convert底层 tools' `parameters_schema()` to `ToolDefinition`
   - If `llm_client` is available and `native_tools` is non-empty:
     - Build `Vec<Box<dyn ToolHandler>>` from both skills (via adapter) and底层 tools (via `SkillToolHandler`)
     - Call `llm_client.chat_with_tools_react()` directly
     - Skip the broken `call_llm_with_tools()` path for底层 tool scenarios
   - Keep existing `SKILL:` text-based path as fallback

7. **File tool sandboxing** (`skills/tool_set.rs`)
   - Each file tool gets a `work_dir: PathBuf` field
   - Path resolution logic:
     ```rust
     fn resolve_path(&self, input_path: &str) -> Result<PathBuf, String> {
         let path = Path::new(input_path);
         let resolved = if path.is_absolute() {
             path.to_path_buf()
         } else {
             self.work_dir.join(path)
         };
         let canonical = std::fs::canonicalize(&resolved)
             .unwrap_or(resolved.clone());
         let work_canonical = std::fs::canonicalize(&self.work_dir)
             .unwrap_or(self.work_dir.clone());
         if !canonical.starts_with(&work_canonical) {
             return Err(format!("Path '{}' is outside working directory '{}'", input_path, self.work_dir.display()));
         }
         Ok(resolved)
     }
     ```

### Pros
- Uses standard OpenAI function calling API — best LLM compatibility
- Multi-turn tool calling already implemented (loop handles tool results automatically)
- Clean separation: tool definition (for LLM) vs tool execution (runtime)
- Extensible: new tools automatically available to LLM

### Cons
- Requires `LLMClient` to be plumbed through Agent → Runtime → Gateway
- `LLMClientAdapter` stub remains (but is bypassed for底层 tool calls)

---

## Option B: Text-Based `TOOL:` Trigger (Lightweight)

Keep native function calling for registered skills only. Add a new text-based trigger for底层 tools, similar to how `SKILL:` works today.

### Architecture

```
User: "写一个1+1的python应用"
  ↓
Agent::handle_llm_task_internal()
  ↓ System prompt now includes both skills AND底层 tools description
  ↓ LLM outputs: "TOOL:file_write|{\"path\":\"app.py\",\"content\":\"print(1+1)\"}"
  ↓
Agent parses TOOL: prefix → finds file_write in tool_set
  ↓
SkillTool::execute() → "File written"
  ↓
Result appended to conversation history
  ↓
One more LLM call to generate final answer
```

### Implementation Steps

1. **Same as Option A steps 1, 2, 7** (working directory + file tool sandboxing)

2. **Enhance system prompt** (`agent_impl.rs`)
   - In `inject_skill_catalog()` or a new method, append底层 tool descriptions:
     ```
     --- AVAILABLE TOOLS ---
     You can also directly call these底层 tools when needed:
     - file_write: Write content to a file. Parameters: {"path": "...", "content": "..."}
     - file_glob: Find files matching a pattern. Parameters: {"pattern": "..."}
     ...
     To call a tool, output: TOOL:<tool_name>|{"param": "value"}
     ```

3. **Add `TOOL:` parser** (`agent_impl.rs`, next to `SKILL:` parser at line ~3848)
   ```rust
   if let Some(tool_part) = trimmed.strip_prefix("TOOL:") {
       let (tool_name, params_json) = tool_part.split_once('|').unwrap_or((tool_part, "{}"));
       if let Some(tool) = self.get_tool(tool_name).await {
           let result = tool.execute(&params).await?;
           // Feed result back to LLM for final answer generation
           let final = self.llm.call_llm_with_history(..., result).await?;
           return Ok((final, vec![]));
       }
   }
   ```

4. **Agent-level tool registry**
   - `Agent` holds `tool_set: HashMap<String, Box<dyn SkillTool>>` (built from `default_tool_set(&self.tool_work_dir)`)
   - `get_tool(name)` lookup method

### Pros
- Minimal changes to LLM calling infrastructure
- Reuses existing `SKILL:` parsing pattern
- No need to pass `LLMClient` through layers

### Cons
- Text parsing is fragile (LLM may not follow exact format)
- Multi-turn tool calling needs manual implementation (chat_with_tools_react already does this)
- LLM may confuse `SKILL:` vs `TOOL:` triggers
- No structured JSON schema validation before LLM generates parameters

---

## Recommendation

**Option A (Fix Native Function Calling)** is strongly recommended because:
1. Kimi k2.6 (the configured LLM) explicitly declares `function_calling: true` and supports OpenAI-compatible `tools`/`tool_choice` API
2. The multi-turn loop (`chat_with_tools_react`) is already battle-tested — we only need to replace the stub executor
3. Text-based parsing (`SKILL:`, `TOOL:`) is inherently less reliable than structured function calling
4. The plumbing cost (passing `LLMClient` through 3 layers) is a one-time fix that enables many future features

## Files to Modify

| File | Changes |
|------|---------|
| `crates/agents/src/llm/skill_tool_handler.rs` | **New** — bridges `SkillTool` to `ToolHandler` |
| `crates/agents/src/agent_impl.rs` | Add `tool_work_dir`, `llm_client`, `tool_set`, `safe_resolve_path`, modify `handle_llm_task_internal`, `spawn_sub_agent` |
| `crates/agents/src/kernel_integration.rs` | Add `with_llm_client()`, `with_tool_work_dir()` to builder |
| `crates/agents/src/runtime/agent_runtime_impl.rs` | Add `llm_client` field, pass to Agent builder |
| `crates/agents/src/skills/tool_set.rs` | File tools: add `work_dir` field + path sandboxing |
| `apps/gateway/src/main.rs` | Create `llm_client`, pass to runtime, create `/data/workspace/` |

## Verification Plan

1. Unit test: `file_write` with relative path resolves to `/data/workspace/`
2. Unit test: `file_write` with absolute path `/etc/passwd` is rejected
3. Integration test: "写一个输出hello world的python文件" → verify `file_write` is called via native function calling
4. Integration test: "查找本机所有的py文件" → verify `file_glob` is called
