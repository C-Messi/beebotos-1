# MCP Skill 参数提取与交互式补全优化方案

> 解决 MCP 工具型 Skill（如 `place_crypto_order`）在自然语言输入下参数缺失、无法完成交互式下单流程的问题。
>
> 版本：v1.0  
> 日期：2026-05-09

---

## 一、执行摘要

### 1.1 问题概述

当前 BeeBotOS 的 MCP Skill 执行链路存在**参数断层**：当用户以自然语言描述操作意图（如"帮我开一单 BTC，100 美元"）时，系统无法将自然语言自动转换为 MCP 工具所需的结构化参数（`symbol`, `notional`, `side` 等），导致：

| 场景 | 用户输入 | 当前结果 | 期望结果 |
|------|----------|----------|----------|
| **BTC 下单** | "帮我开一单 BTC 市场，单笔金额不要超过 100 USD" | Approval Gate 拦截 → 用户确认 → `Missing required parameter` 报错 | 系统提取参数 → 展示订单确认卡片 → 用户确认 → 执行下单 |
| **搜索查询** | "搜索一下中国人口多少" | 已修复（Knowledge Skill 工具检测 + WebFetchTool） | — |
| **股票下单** | "买入 100 股 AAPL" | 参数为空，直接失败 | 提取 `symbol=AAPL`, `qty=100`, `side=buy` → 确认 → 执行 |

### 1.2 核心设计目标

1. **自动参数提取**：当 MCP Skill 被匹配但参数缺失/不完整时，自动调用 LLM 从用户自然语言中提取结构化参数。
2. **交互式补全**：对于无法自动提取或需要用户二次确认的参数（如交易金额、风险等级），渲染交互式表单/卡片，让用户在聊天界面中填写或选择。
3. **两阶段安全确认**：参数提取 → 预览展示 → 用户确认 → 实际执行。高风险操作（交易、转账）始终保持人工确认环节。
4. **向后兼容**：纯代码 Skill（`has_scripts=true`）和纯知识 Skill（无工具引用）的执行链路不受任何影响。

---

## 二、现状与问题深度分析

### 2.1 MCP Skill 执行链路现状

**代码位置**：`crates/agents/src/agent_impl.rs` line ~2824–2913

```rust
// ── MCP Skill Bridge ──
if let Some((server_name, tool_name)) = parse_mcp_skill_id(&skill.id) {
    let client = mcp.get_client(server_name).await.unwrap();

    // Build arguments from input + parameters
    let mut arguments = serde_json::Map::new();
    if !input.is_empty() {
        // Try to parse input as JSON; if fails, wrap in "query"
        match serde_json::from_str::<Map>(input) {
            Ok(map) => arguments = map,
            Err(_) => {
                arguments.insert("query", input.to_string());
            }
        }
    }

    // Security: validate against JSON Schema
    if let Err(e) = validate_tool_arguments(&schema, &arguments) {
        return Err("MCP tool argument validation failed: Missing required parameter");
    }

    // Direct execution
    client.call_tool(tool_name, args).await?
}
```

### 2.2 关键缺陷

#### 缺陷 1：自然语言 → JSON 的转换缺失

当前逻辑只有两种处理路径：
- 输入是合法 JSON → 直接作为参数
- 输入不是 JSON → 整体包装为 `{"query": "用户原始输入"}`

**问题**：绝大多数 MCP 工具（如 Alpaca 的 `place_crypto_order`）不接受 `query` 字段，而是需要 `symbol`、`notional`、`side` 等精确字段。系统没有把"BTC"映射为 `symbol`，没有把"100 美元"映射为 `notional`。

#### 缺陷 2：参数验证失败后直接报错，无补救路径

```rust
if let Err(validation_err) = validate_tool_arguments(&schema, &arguments) {
    return Err(AgentError::InvalidConfig(format!(
        "MCP tool argument validation failed: {}", validation_err
    )));
}
```

**问题**：验证失败后直接抛出 `AgentError`，外层流程只能返回错误文本给用户。没有机会：
- 用 LLM 重新解析参数
- 向用户询问缺失参数
- 展示预览卡片等待确认

#### 缺陷 3：Approval Gate 与参数缺失的时序错位

当前流程：
1. 构建参数（可能为空或不完整）
2. **Approval Gate 评估**（基于 skill_id 和原始参数，此时参数可能为空）
3. 用户看到"关键操作，需要确认"
4. 用户回复"确认"
5. **重新执行时参数仍然为空**
6. JSON Schema 验证失败 → 报错

**问题**：Approval Gate 拦截太早。用户还没看到自己要下什么单（BTC？多少金额？），就被要求确认。确认后系统才发现缺少参数。

#### 缺陷 4：Planning 对交互式任务分解不足

当前 Planning 引擎擅长分解"多步骤操作型任务"（查天气 → 规划路线 → 订酒店），但不擅长分解"单步骤但需交互确认的任务"（下单）。

当日志中出现 `Created plan with 1 steps` 时，Planning 实际上没有为交互式参数收集创建任何子步骤。

---

## 三、核心设计方案

### 3.1 总体架构：两阶段执行模型

```
┌─────────────────────────────────────────────────────────────┐
│                    MCP Skill Execution                        │
│                      (Two-Stage Model)                        │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Stage 1: Parameter Resolution (参数解析阶段)                 │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  1.1 尝试直接从 input/parameters 解析 JSON 参数       │    │
│  │       ✓ 成功 → 进入 Stage 2                          │    │
│  │       ✗ 失败/缺失 → 1.2 LLM 参数提取                 │    │
│  │                                                     │    │
│  │  1.2 LLM Parameter Extraction                       │    │
│  │       输入：用户原始文本 + MCP Tool JSON Schema       │    │
│  │       输出：提取的参数 JSON 或 "need_user_input"      │    │
│  │       ✓ 成功 → 进入 Stage 2                          │    │
│  │       ✗ 无法提取 → 1.3 交互式参数收集                 │    │
│  │                                                     │    │
│  │  1.3 Interactive Parameter Collection               │    │
│  │       渲染缺失参数表单 → 用户填写 → 回到 1.1          │    │
│  └─────────────────────────────────────────────────────┘    │
│                          ↓                                    │
│  Stage 2: Confirmation & Execution (确认执行阶段)             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  2.1 参数预览（高风险操作展示确认卡片）                │    │
│  │       ┌─────────────────────────────┐               │    │
│  │       │ 📋 订单预览                   │               │    │
│  │       │ 品种: BTC                    │               │    │
│  │       │ 方向: 买入                   │               │    │
│  │       │ 金额: $100                   │               │    │
│  │       │                              │               │    │
│  │       │ [确认下单]  [取消]           │               │    │
│  │       └─────────────────────────────┘               │    │
│  │                                                     │    │
│  │  2.2 Approval Gate（基于完整参数评估风险等级）        │    │
│  │                                                     │    │
│  │  2.3 用户确认 → 执行 MCP Tool Call                  │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 模块 1：LLM 参数提取引擎

#### 3.2.1 触发条件

在 `execute_registered_skill` 的 MCP 分支中，当以下任一条件满足时触发：

```rust
let needs_extraction = arguments.is_empty() 
    || validate_tool_arguments(&schema, &arguments).is_err();
```

#### 3.2.2 LLM Prompt 设计

```text
You are a parameter extraction assistant. Your job is to extract structured 
parameters from the user's natural language request for a specific tool.

Tool Name: {tool_name}
Tool Description: {tool_description}

Required Parameters:
- symbol (string): Trading pair symbol, e.g. "BTC/USD", "ETHUSD"
- notional (number): Dollar amount to trade, e.g. 100
- side (string): "buy" or "sell"

User Request: "帮我开一单 BTC 市场，单笔金额不要超过 100 USD"

Extract the parameters as a JSON object. If a required parameter cannot be 
determined from the text, output: {"_missing": ["param_name"]}.
If the user's intent is unclear, output: {"_unclear": true, "_reason": "..."}.

Rules:
1. "BTC" → symbol: "BTC/USD" (use the tool's default pair format)
2. "100 美元" / "100 USD" / "不要超过 100 USD" → notional: 100
3. "开一单"/"买入"/"买" → side: "buy"; "卖出"/"卖" → side: "sell"
4. Do NOT add fields that are not in the tool schema.
5. Do NOT guess optional parameters unless explicitly mentioned.

Output ONLY the JSON object, no explanation:
```

#### 3.2.3 实现位置与接口

**新增文件**：`crates/agents/src/skills/mcp_parameter_extractor.rs`

```rust
pub struct McpParameterExtractor {
    llm: Arc<dyn LLMCallInterface>,
}

impl McpParameterExtractor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Extract parameters from natural language input.
    /// 
    /// Returns:
    /// - `Ok(ExtractedParams::Complete(json))` — all required params extracted
    /// - `Ok(ExtractedParams::Partial { partial, missing })` — some params need user input
    /// - `Ok(ExtractedParams::Unclear { reason })` — user intent is ambiguous
    /// - `Err(e)` — LLM call failed
    pub async fn extract(
        &self,
        user_input: &str,
        tool_schema: &serde_json::Value,
        tool_description: &str,
    ) -> Result<ExtractedParams, AgentError>;
}
```

#### 3.2.4 JSON Schema → Prompt 的自动转换

为了避免为每个 MCP 工具手写 prompt，需要实现 schema 到 prompt 的自动转换：

```rust
fn schema_to_parameter_description(schema: &Value) -> String {
    let properties = schema.get("properties").and_then(|p| p.as_object())?;
    let required: Vec<String> = schema.get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    
    let mut lines = vec![];
    for (name, prop) in properties {
        let ty = prop.get("type").and_then(|t| t.as_str()).unwrap_or("string");
        let desc = prop.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let is_req = required.contains(name);
        lines.push(format!("- {} ({}{}): {}", name, ty, if is_req { ", required" } else { "" }, desc));
    }
    lines.join("\n")
}
```

### 3.3 模块 2：交互式参数补全

#### 3.3.1 适用场景

LLM 参数提取后，如果仍有缺失参数，或者某些参数需要用户显式确认（如交易金额），进入交互式补全流程。

#### 3.3.2 交互协议设计

系统向用户发送**结构化卡片**（JSON 格式，由前端渲染）：

```json
{
  "type": "parameter_form",
  "skill_id": "mcp:alpaca/place_crypto_order",
  "title": "📋 请确认订单参数",
  "fields": [
    {
      "name": "symbol",
      "label": "交易品种",
      "type": "select",
      "value": "BTC/USD",
      "options": ["BTC/USD", "ETH/USD", "SOL/USD"],
      "required": true,
      "description": "请选择要交易的加密货币"
    },
    {
      "name": "side",
      "label": "交易方向",
      "type": "radio",
      "value": "buy",
      "options": [
        {"label": "买入", "value": "buy"},
        {"label": "卖出", "value": "sell"}
      ],
      "required": true
    },
    {
      "name": "notional",
      "label": "交易金额 (USD)",
      "type": "number",
      "value": 100,
      "min": 1,
      "max": 10000,
      "required": true,
      "description": "单笔金额不超过 100 USD"
    }
  ],
  "actions": [
    {"label": "确认下单", "action": "submit", "style": "primary"},
    {"label": "取消", "action": "cancel", "style": "danger"}
  ],
  "extracted_from": "根据您说的'帮我开一单 BTC 市场，单笔金额不要超过 100 USD'提取"
}
```

前端渲染为：
- WebChat：自适应卡片（下拉选择、单选按钮、数字输入框）
- CLI：文本表单（逐行提示输入）

#### 3.3.3 状态机：Pending Parameter Collection

**新增状态**：`PendingParameterCollection`

```rust
pub struct PendingParameterForm {
    pub request_id: String,
    pub skill_id: String,
    pub user_input: String,
    pub partial_params: serde_json::Map<String, Value>,
    pub missing_fields: Vec<FieldSchema>,
    pub submitted_at: Instant,
    pub expires_at: Instant,  // TTL: 5 minutes
}

// Agent 状态中新增
pub(crate) pending_parameter_forms: Arc<RwLock<HashMap<String, PendingParameterForm>>>,
```

状态流转：

```
Idle → Working (task started)
     → PendingParameterCollection (form sent to user)
     → Working (user submitted form → re-execute skill)
     → Idle (success / error / timeout)
```

#### 3.3.4 用户提交后的处理

用户提交表单后，Gateway 将表单数据封装为新的 task：

```json
{
  "message": "参数表单提交",
  "form_submission": {
    "request_id": "uuid",
    "values": {
      "symbol": "BTC/USD",
      "side": "buy",
      "notional": "100"
    }
  }
}
```

Agent 识别到 `form_submission` 后：
1. 查找对应的 `PendingParameterForm`
2. 合并 `partial_params` + `values` → 完整参数
3. 重新调用 `execute_registered_skill`，此时参数完整
4. 进入 Stage 2（Approval Gate → 执行）

### 3.4 模块 3：参数预览卡片（Stage 2）

对于高风险操作（交易、转账、删除），即使参数已完整，也**必须**先展示预览卡片，让用户确认。

#### 3.4.1 预览卡片格式

```json
{
  "type": "action_preview",
  "risk_level": "high",
  "title": "🔴 高风险操作确认",
  "description": "您即将执行以下操作，请仔细核对：",
  "details": [
    {"label": "操作", "value": "加密货币下单"},
    {"label": "品种", "value": "BTC/USD"},
    {"label": "方向", "value": "买入"},
    {"label": "金额", "value": "$100.00"},
    {"label": "交易所", "value": "Alpaca Paper Trading"}
  ],
  "warning": "⚠️ 此操作涉及真实资金，确认后无法撤销。",
  "confirm_text": "确认下单",
  "cancel_text": "取消",
  "request_id": "uuid"
}
```

#### 3.4.2 与现有 Approval Gate 的整合

现有 Approval Gate（`crates/agents/src/security/approval_gate.rs`）基于 rule 匹配自动批准或拒绝。本方案不改变 Approval Gate 的规则匹配逻辑，而是**在 Approval Gate 之前插入参数预览**。

新流程：

```rust
// Stage 1: 参数解析
let params = resolve_parameters(input, schema).await?;

// Stage 2: 预览 + Approval
if is_high_risk_skill(&skill_id) {
    // 生成预览卡片
    let preview = generate_action_preview(&skill_id, &params);
    
    // 检查是否有 auto-approval rule
    match gate.evaluate(&skill_id, &params, &env) {
        AutoApproved { rule } => {
            // 即使 auto-approved，高风险操作仍展示预览
            // 但不需要用户手动回复"确认"
            // （或者：paper trading auto-approve，live trading 始终确认）
        }
        Rejected { reason } => return Err(...),
        NeedsConfirm => {
            // 发送预览卡片，等待用户确认
            return Ok(preview_card_response);
        }
    }
}
```

---

## 四、详细设计

### 4.1 修改文件清单

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `crates/agents/src/skills/mcp_parameter_extractor.rs` | **新增** | LLM 参数提取引擎 |
| `crates/agents/src/skills/mod.rs` | 修改 | 暴露新模块 |
| `crates/agents/src/agent_impl.rs` | 修改 | MCP 分支改为两阶段执行 |
| `crates/agents/src/agent_impl.rs` | 修改 | 新增 `pending_parameter_forms` 状态 |
| `crates/agents/src/security/approval_gate.rs` | 修改 | 支持基于完整参数的风险评估 |
| `apps/gateway/src/services/message_processor.rs` | 修改 | 识别 `form_submission` 消息类型 |
| `apps/gateway/src/handlers/http/channels.rs` | 修改 | 前端卡片渲染协议支持 |

### 4.2 核心代码逻辑：MCP 分支改造

**当前逻辑**（`agent_impl.rs` line 2824–2913）：

```rust
// Build arguments from input + parameters
let mut arguments = ...;
// Validate
if let Err(e) = validate_tool_arguments(&schema, &arguments) {
    return Err("validation failed");
}
// Execute
client.call_tool(tool_name, args).await
```

**新逻辑**：

```rust
// ── MCP Skill Bridge: Two-Stage Execution ──
if let Some((server_name, tool_name)) = parse_mcp_skill_id(&skill.id) {
    let client = ...;
    
    // ===== STAGE 1: Parameter Resolution =====
    let mut arguments = build_initial_arguments(input, parameters);
    
    // Try validation; if fails, attempt LLM extraction
    let schema = fetch_tool_schema(&client, tool_name).await?;
    let validation = validate_tool_arguments(&schema, &arguments);
    
    let final_params = if arguments.is_empty() || validation.is_err() {
        // 1.2 LLM Parameter Extraction
        let extractor = McpParameterExtractor::new(self.llm_interface.clone().unwrap());
        match extractor.extract(input, &schema, &tool_desc).await? {
            ExtractedParams::Complete(params) => params,
            ExtractedParams::Partial { partial, missing } => {
                // 1.3 Interactive Parameter Collection
                let form = ParameterForm::new(skill_id, input, partial, missing);
                let req_id = form.request_id.clone();
                self.pending_parameter_forms.write().await.insert(req_id.clone(), form);
                return Ok(SkillExecutionResult {
                    task_id: skill_id,
                    success: false,
                    output: render_parameter_form(&req_id, &missing, &partial),
                    structured_output: Some(form.to_json()),
                    execution_time_ms: ...,
                });
            }
            ExtractedParams::Unclear { reason } => {
                return Ok(SkillExecutionResult {
                    success: false,
                    output: format!("无法从您的描述中提取操作参数。{} 请提供更具体的信息。", reason),
                    ...
                });
            }
        }
    } else {
        arguments
    };
    
    // ===== STAGE 2: Confirmation & Execution =====
    if is_high_risk_skill(&skill_id) {
        let preview = generate_action_preview(&skill_id, &final_params);
        let approval = self.approval_gate.evaluate(&skill_id, &final_params, &env);
        match approval {
            ApprovalResult::AutoApproved { rule } if is_paper_trading(&rule) => {
                // Paper trading: skip confirmation, execute directly
            }
            _ => {
                // Send preview card and wait for user confirmation
                return Ok(SkillExecutionResult {
                    success: false,
                    output: preview.to_markdown(),
                    structured_output: Some(preview.to_json()),
                    ...
                });
            }
        }
    }
    
    // Execute MCP tool call
    let result = client.call_tool(tool_name, Some(final_params)).await?;
    // ...
}
```

### 4.3 表单提交处理流程

**新增处理路径**：`process_task_v2` 中检测 `form_submission`

```rust
async fn process_task_v2(&self, task: Task) -> Result<(String, Vec<Artifact>), AgentError> {
    // Check for form submission
    if let Ok(json) = serde_json::from_str::<Value>(&task.input) {
        if let Some(form) = json.get("form_submission") {
            return self.handle_form_submission(form).await;
        }
    }
    // ... existing logic
}

async fn handle_form_submission(&self, form: &Value) -> Result<(String, Vec<Artifact>), AgentError> {
    let request_id = form.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
    let values = form.get("values").and_then(|v| v.as_object()).ok_or("Invalid form values")?;
    
    let pending = self.pending_parameter_forms.write().await.remove(request_id)
        .ok_or("表单已过期或不存在，请重新发起请求。")?;
    
    // Merge partial params with submitted values
    let mut final_params = pending.partial_params.clone();
    for (k, v) in values {
        final_params.insert(k.clone(), v.clone());
    }
    
    // Re-execute the skill with complete parameters
    let registry = self.skill_registry.as_ref().unwrap();
    if let Some(skill) = registry.get(&pending.skill_id).await {
        let params_str_map: HashMap<String, String> = final_params.iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();
        let result = self.execute_registered_skill(
            &skill, 
            &pending.user_input, 
            Some(params_str_map)
        ).await?;
        return Ok((result.output, vec![]));
    }
    
    Err(AgentError::Execution("Skill not found".to_string()))
}
```

### 4.4 参数提取 Prompt 模板（完整版）

```rust
const PARAMETER_EXTRACTION_PROMPT: &str = r#"
You are a precise parameter extraction engine. Extract structured parameters 
from the user's natural language request for the specified tool.

## Tool Information
Name: {tool_name}
Description: {tool_description}

## Parameter Schema
{parameter_descriptions}

## User Request
"{user_input}"

## Extraction Rules
1. Extract EXACTLY the fields defined in the schema. Do NOT invent new fields.
2. For required fields:
   - If present in the user request, extract the value.
   - If NOT present, include the field name in the `_missing` array.
3. For optional fields:
   - Only include if explicitly mentioned or clearly implied.
   - If not mentioned, omit the field entirely.
4. Type conversions:
   - "100 美元", "$100", "100 USD" → number: 100
   - "买入", "买", "开多" → "buy"; "卖出", "卖", "开空" → "sell"
   - "BTC" → follow tool's expected format (usually "BTC/USD" or "BTCUSD")
5. If the user's intent is completely unclear, set `_unclear` to true and explain why.

## Output Format
Return ONLY a JSON object. No markdown, no explanation.

Examples:
- Complete: {"symbol":"BTC/USD","side":"buy","notional":100}
- Partial: {"symbol":"BTC/USD","_missing":["side","notional"]}
- Unclear: {"_unclear":true,"_reason":"User did not specify what to trade"}
"#;
```

---

## 五、实施计划

### Phase 1：LLM 参数提取引擎（1–2 天）

| 任务 | 文件 | 工作量 |
|------|------|--------|
| 创建 `McpParameterExtractor` | `skills/mcp_parameter_extractor.rs` | 中 |
| Schema → Prompt 自动转换 | `mcp_parameter_extractor.rs` | 小 |
| 单元测试：参数提取准确率 | `tests/` | 中 |

### Phase 2：MCP 分支两阶段改造（2–3 天）

| 任务 | 文件 | 工作量 |
|------|------|--------|
| MCP 分支改为 Stage 1 + Stage 2 | `agent_impl.rs` | 中 |
| 参数预览卡片生成 | `agent_impl.rs` | 小 |
| 与 Approval Gate 整合 | `security/approval_gate.rs` | 小 |

### Phase 3：交互式参数表单（3–5 天）

| 任务 | 文件 | 工作量 |
|------|------|--------|
| `PendingParameterForm` 数据结构 | `agent_impl.rs` | 小 |
| 表单渲染协议（JSON Schema） | `agent_impl.rs` | 中 |
| Gateway 表单提交处理 | `message_processor.rs` | 中 |
| WebChat 前端卡片渲染 | `channels.rs` + 前端 | 大 |

### Phase 4：端到端测试（2–3 天）

| 测试场景 | 期望行为 |
|----------|----------|
| "买入 100 美元 BTC" | 自动提取参数 → 预览卡片 → 用户确认 → 执行 |
| "帮我下单"（信息不全） | 参数表单（品种/金额/方向）→ 用户填写 → 预览 → 确认 → 执行 |
| "查询 BTC 价格"（非交易 skill） | 不受影响，正常执行 |
| 表单超时（5 分钟未提交） | 返回"表单已过期，请重新发起请求" |

---

## 六、风险与回退策略

### 6.1 风险矩阵

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| LLM 参数提取错误（如把 ETH 当成 BTC） | 中 | 高（错误交易） | 预览卡片强制二次确认；高风险操作始终人工确认 |
| 表单提交后 skill 已被卸载 | 低 | 中 | 表单 pending 状态保存 skill_id，卸载时清理 pending forms |
| LLM 提取耗时增加（+1–3s） | 高 | 低 | 提取结果缓存（相同 input + schema → 复用）；异步预提取 |
| 前端不支持表单渲染 | 低 | 中 | fallback 到纯文本问答式参数收集 |

### 6.2 回退策略

如果参数提取引擎出现故障或准确率不达标，可以通过配置开关回退到当前行为：

```toml
# config/beebotos.toml
[skill_execution]
mcp_parameter_extraction = "auto"   # auto / disabled / interactive_only
# auto: 先尝试提取，失败则交互式收集
# disabled: 当前行为（直接报错）
# interactive_only: 不尝试 LLM 提取，直接展示表单
```

---

## 七、与现有系统的兼容性

| 系统组件 | 影响 | 说明 |
|----------|------|------|
| **V2 Intent Analyzer** | 无影响 | 意图识别阶段不感知参数提取 |
| **Skill Selector** | 无影响 | Skill 匹配逻辑不变 |
| **Planning Engine** | 无影响 | Planning step 执行仍调用 `execute_registered_skill`，内部改造对其透明 |
| **Approval Gate** | 增强 | 基于完整参数评估更准确 |
| **Knowledge Executor** | 无影响 | 纯知识 skill 不走 MCP 分支 |
| **Code Skill Executor** | 无影响 | Code skill 有 scripts，不走 MCP 分支 |
| **Trace Store** | 增强 | 新增 `parameter_extraction` 和 `form_interaction` trace 类型 |

---

## 八、附录

### A. 相关代码引用

- MCP Skill Bridge: `crates/agents/src/agent_impl.rs:2824–2913`
- Approval Gate: `crates/agents/src/security/approval_gate.rs`
- Tool Validation: `crates/agents/src/mcp/skill_bridge.rs:279–320`
- ReAct Executor: `crates/agents/src/skills/react_executor.rs`
- Knowledge Executor: `crates/agents/src/skills/knowledge_executor.rs`

### B. 生产日志参考

```
// 参数缺失导致 approval 后仍失败
ERROR: Task failed: Invalid agent configuration: 
  MCP tool argument validation failed: Missing required parameter

// Planning 1 step 但无参数分解
INFO: Created plan with 1 steps
INFO: P2 PLANNING: matched skill 'place_crypto_order'
WARN: Approval required but not granted: No auto-approval rule matched
```
