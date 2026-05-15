# LLM 加密货币交易决策系统 — 全链路 Prompt 工程 v2（ReAct 自主规划版）

> **文档定位**：基于 `LLM-trade-selfdecision-v1.md` 设计愿景，采用**纯 ReAct 模式**——LLM 完全自主决定 Planning、工具调用序列、数据获取策略和终止时机，构建自由灵活的投资决策分析平台。所有涉及多步推理的任务统一采用此模式。
>
> **核心原则**：
> - **不预设数据获取路径**——LLM 自主决定先获取什么、再获取什么
> - **不人为编排并行/串行**——ReAct 循环中 LLM 每轮自主决策
> - **最多 10 轮**——上限保护，由 LLM 决定何时终止（`final_answer`）
> - **统一 ReAct 引擎**——交易分析、表单提交、下单确认等全部复用同一执行框架
>
> **适用模型**：`kimi-k2.6`（temperature 严格 `0.6`，thinking `disabled`，tool_choice `auto/none`）
> **版本**：v2.1-ReAct | 2026-05-08

---

## 目录

1. [系统架构与定位](#1-系统架构与定位)
2. [统一 ReAct 执行引擎](#2-统一-react-执行引擎)
3. [投资决策分析 ReAct System Prompt（核心）](#3-投资决策分析-react-system-prompt核心)
4. [其他场景的 ReAct 统一应用](#4-其他场景的-react-统一应用)
5. [各阶段 Prompt 详细模板](#5-各阶段-prompt-详细模板)
6. [上下文组装与记忆管理](#6-上下文组装与记忆管理)
7. [安全合规与风控护栏](#7-安全合规与风控护栏)
8. [实现路线图](#8-实现路线图)

---

## 1. 系统架构与定位

### 1.1 架构演进：从预设编排到 ReAct 自主规划

**旧模式（已废弃）**：
```
User Input → Intent → Skill Selector → [并行获取price/ohlcv/orderbook/funding] 
                                              ↓
                                   [单次LLM综合分析] → 输出报告
```
问题：人为预设了"获取哪些数据"，LLM 没有自主决策权，分析维度固化。

**新模式（ReAct 自主规划）**：
```
User Input → Intent Analyzer → Skill Selector → [ReAct 执行引擎]
                                                      ↑↓
                                              LLM 自主 Planning 循环
                                              ├─ Round 1: LLM 决定先获取什么
                                              ├─ Round 2: 观察结果，决定下一步
                                              ├─ Round 3: 继续或调整策略
                                              ├─ ...
                                              └─ Round N (≤10): LLM 输出 final_answer
```

LLM 像一位**真正的分析师**：拿到用户问题后，自主思考"我需要哪些数据？"→"先查价格看看大趋势"→"再查 RSI 判断是否超买"→"还需要订单簿确认支撑"→"数据够了，出报告"。每一步都是 LLM 自主决定，系统只负责执行工具调用并把结果回注。

### 1.2 统一 ReAct 执行框架

所有需要多步推理的任务，统一进入 **ReActExecutor**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Unified ReAct Execution Framework                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Input: user_request + intent_result + available_tools + user_context      │
│                                ↓                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                    ReAct Loop (max 10 rounds)                         │   │
│   │                                                                     │   │
│   │  Round 1: System Prompt + User Request                              │   │
│   │      → LLM: "我需要先获取 BTC 实时价格"                             │   │
│   │      → Action: call_tool(crypto_price, {"symbol":"BTC"})            │   │
│   │      → Observation: {"price": 67234, "change_24h": -5.2%}           │   │
│   │                                                                     │   │
│   │  Round 2: System Prompt + Round 1 History + Observation             │   │
│   │      → LLM: "价格下跌5.2%，我需要看RSI判断是否超卖"                 │   │
│   │      → Action: call_tool(calculate_rsi, {"symbol":"BTC","period":14})│   │
│   │      → Observation: {"rsi": 32.4}                                   │   │
│   │                                                                     │   │
│   │  Round 3: ...                                                       │   │
│   │      → LLM: "还需要看订单簿和资金费率确认底部结构"                  │   │
│   │      → Action: call_tool(...)                                       │   │
│   │                                                                     │   │
│   │  Round N:                                                           │   │
│   │      → LLM: "数据已足够，输出分析报告"                              │   │
│   │      → Action: final_answer({structured_report})                    │   │
│   │                                                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                ↓                                             │
│   Output: final_answer (结构化 JSON / Markdown 报告 / 表单 / 确认卡片)      │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.3 适用范围（统一 ReAct 模式）

| 场景 | 说明 | 典型工具链 |
|------|------|-----------|
| **加密货币投资决策分析** | 用户问"BTC 能买吗" | crypto_price, ohlcv, rsi, macd, orderbook, funding_rate |
| **交易表单提交** | 用户说"买 100 美元 BTC" | 参数提取 → 表单渲染 → 收集缺失参数 → 预览确认 |
| **下单确认流程** | 高价值/高风险操作 | 预览卡片 → 用户确认/取消 → 执行 → 结果反馈 |
| **多币种对比分析** | "ETH 和 SOL 哪个更值得买" | 多币种价格、指标逐一获取后对比 |
| **复杂查询** | "找出过去一周涨幅最大且RSI不超买的币" | 筛选 → 逐一获取 → 排序 → 推荐 |
| **通用多步任务** | 任何需要 >1 个工具调用的任务 | 由 Skill Selector 标记 needs_planning: yes |

---

## 2. 统一 ReAct 执行引擎

### 2.1 执行引擎设计

```rust
/// 统一 ReAct 执行引擎
/// 所有多步推理任务共用此引擎，通过不同的 System Prompt 区分行为
pub struct UnifiedReActExecutor {
    llm: Arc<dyn LLMCallInterface>,
    config: ReActConfig,
}

pub struct ReActConfig {
    /// 最大轮数（硬上限）
    pub max_rounds: usize,        // default: 10
    /// 每轮 LLM 调用超时
    pub round_timeout_sec: u64,   // default: 30
    /// 是否启用反射（让 LLM 回顾之前步骤的正确性）
    pub enable_reflection: bool,  // default: true
    /// 最终答案必须是结构化 JSON
    pub require_structured_output: bool, // default: true for analysis
}

/// 单轮结果
pub struct ReActRound {
    pub round_number: usize,
    pub llm_thought: String,      // LLM 的推理过程（从 response 解析）
    pub action: ReActAction,      // call_tool 或 final_answer
    pub observation: Option<String>, // 工具执行结果（仅 call_tool）
    pub timestamp: Instant,
}

pub enum ReActAction {
    CallTool {
        tool_name: String,
        arguments: serde_json::Map<String, Value>,
        reasoning: String,
    },
    FinalAnswer {
        content: String,          // JSON 或 Markdown
    },
}
```

### 2.2 ReAct 循环伪代码

```rust
async fn execute_react_loop(
    &self,
    system_prompt: &str,           // 场景专用 System Prompt
    user_request: &str,            // 用户原始输入
    available_tools: &[ToolDef],   // 该场景可用的 MCP tools
    user_context: &UserContext,    // 用户画像、持仓、记忆
) -> Result<String, AgentError> {
    
    let mut rounds: Vec<ReActRound> = vec![];
    let mut messages = vec![];
    
    // Round 0: 注入 System Prompt + 可用工具列表 + 用户请求
    let initial_prompt = build_initial_prompt(system_prompt, user_request, available_tools, user_context);
    messages.push(Message::system(initial_prompt));
    
    for round in 1..=self.config.max_rounds {
        
        // 构建当前轮次的 prompt（包含之前所有轮次的 thought + action + observation）
        let round_prompt = build_round_prompt(&rounds, user_request);
        messages.push(Message::user(round_prompt));
        
        // 调用 LLM
        let llm_response = self.llm.call_llm(messages.clone(), None)
            .await
            .map_err(|e| AgentError::Execution(format!("Round {} LLM call failed: {}", round, e)))?;
        
        // 解析 LLM 输出
        let parsed = parse_react_response(&llm_response)?;
        
        match parsed.action {
            ReActAction::CallTool { tool_name, arguments, reasoning } => {
                
                // 执行工具
                let observation = execute_mcp_tool(&tool_name, &arguments).await;
                
                // 记录本轮
                rounds.push(ReActRound {
                    round_number: round,
                    llm_thought: parsed.thought,
                    action: ReActAction::CallTool { tool_name, arguments, reasoning },
                    observation: Some(observation.clone()),
                    timestamp: Instant::now(),
                });
                
                // 将 observation 回注到 messages（成为下一轮上下文的一部分）
                messages.push(Message::assistant(llm_response));
                messages.push(Message::user(format!(
                    "[Observation] 工具执行结果:\n{}\n\n请基于以上结果，决定下一步操作。",
                    observation
                )));
                
                // 继续下一轮
                continue;
            }
            
            ReActAction::FinalAnswer { content } => {
                // LLM 自主决定终止
                rounds.push(ReActRound {
                    round_number: round,
                    llm_thought: parsed.thought,
                    action: ReActAction::FinalAnswer { content: content.clone() },
                    observation: None,
                    timestamp: Instant::now(),
                });
                
                info!("ReAct loop terminated by LLM at round {}/{}, rounds_used: {}", 
                      round, self.config.max_rounds, rounds.len());
                
                return Ok(content);
            }
        }
    }
    
    // 达到 max_rounds 仍未终止，强制要求 LLM 输出 final_answer
    warn!("ReAct reached max_rounds ({}), forcing final_answer", self.config.max_rounds);
    messages.push(Message::user(
        "[System] 已达到最大思考轮数。请基于已收集的所有数据，立即输出最终分析结论（final_answer），不允许再调用工具。"
    ));
    let forced_response = self.llm.call_llm(messages, None).await?;
    Ok(forced_response)
}
```

### 2.3 关键设计决策

| 决策 | 说明 |
|------|------|
| **为什么 max_rounds=10？** | 足够 LLM 完成"价格→RSI→MACD→订单簿→资金费率→链上→综合→报告"完整链路，同时防止无限循环。 |
| **为什么不由系统预设步骤？** | LLM 根据实时结果动态调整策略。比如 Round 1 发现价格暴涨 20%，LLM 可能直接跳过常规指标，先查爆仓数据和交易所流入。 |
| **LLM 如何知道有哪些工具？** | System Prompt 中注入完整工具列表（名称+描述+参数+返回说明）。 |
| **LLM 如何避免重复调用？** | System Prompt 中要求"维护已调用工具列表"，每轮检查避免重复。 |
| **工具返回错误怎么办？** | Observation 标注错误信息，LLM 在下一轮自主决定：重试/换工具/跳过/报错终止。 |
| **是否支持条件分支？** | 自然支持。LLM 在 Round 2 看到 RSI>70 后，Round 3 可能直接决定"超买，不买了"，提前 final_answer。 |

---

## 3. 投资决策分析 ReAct System Prompt（核心）

这是本方案最核心的设计。该 System Prompt 注入到 **每一轮** ReAct 循环中，指导 LLM 自主完成从数据获取到分析报告的完整流程。

### 3.1 设计目标

1. LLM 扮演**专业加密货币投资分析师**，自主决定需要什么数据、按什么顺序获取
2. 支持**条件性规划**：根据中间结果动态调整后续步骤
3. 输出**结构化 JSON 分析报告**作为 final_answer
4. 融合**用户画像**到分析过程中（不是事后拼接）
5. **分析 ≠ 执行**：final_answer 只包含分析结论，不包含交易指令

### 3.2 System Prompt 全文

```markdown
# ROLE: BeeAgentOS Autonomous Investment Analyst v2.0-ReAct

你是 BeeAgentOS 智能体框架中的**自主投资决策分析引擎**。你的核心能力是通过多轮工具调用自主收集市场数据，进行多维度分析，最终生成结构化投资报告。

## 你的工作模式（ReAct 循环）

每轮你只能做一件事：
1. **思考**（Thought）：分析当前已掌握的信息，判断还需要什么数据
2. **行动**（Action）：要么调用一个工具获取数据，要么输出最终报告

你将在下一轮收到工具返回的结果，然后继续思考下一步。这个循环最多进行 10 轮，由你自主决定何时终止。

## 可用工具列表

以下是你可以调用的 MCP 工具。你的任务不是全部调用，而是**根据分析需要选择性调用**。

### 市场数据工具
- `crypto_price(symbol: string)` → 返回：当前价格、24h涨跌幅、24h成交量、最高/最低价
- `fetch_ohlcv(symbol: string, timeframe: "1m|5m|15m|1h|4h|1d", limit: int)` → 返回：K线数据数组 [时间,开盘,最高,最低,收盘,成交量]
- `calculate_rsi(symbol: string, period: int)` → 返回：RSI 值 (0-100)
- `calculate_macd(symbol: string, fast: int, slow: int, signal: int)` → 返回：MACD线、信号线、柱状图
- `calculate_bollinger(symbol: string, period: int, std_dev: float)` → 返回：上轨、中轨、下轨
- `calculate_atr(symbol: string, period: int)` → 返回：ATR 值（波动率）

### 情绪与资金工具
- `get_orderbook(symbol: string, depth: int)` → 返回：买卖盘深度、价差、大单分布
- `get_funding_rate(symbol: string)` → 返回：资金费率、下次结算时间
- `get_fear_greed_index()` → 返回：恐惧贪婪指数 (0-100) 及分类
- `get_long_short_ratio(symbol: string)` → 返回：多空持仓比

### 链上与宏观工具
- `get_exchange_flow(symbol: string, exchange: string)` → 返回：交易所净流入/流出
- `get_btc_dominance()` → 返回：BTC 市值占比及趋势
- `get_stablecoin_inflow()` → 返回：稳定币交易所储备变化

## 分析框架（供你参考，不强制全部执行）

你应基于用户问题，自主决定需要哪些分析维度：

### A. 技术面
- 当前价格位置 vs 关键支撑/阻力
- RSI、MACD、布林带等动量指标
- K线形态和成交量特征
- 趋势方向（多时间框架）

### B. 情绪面
- 恐惧贪婪指数
- 多空比和资金费率
- 订单簿买卖压力

### C. 资金面
- 交易所净流入/流出
- 稳定币流入情况
- 鲸鱼地址动向

### D. 宏观面
- BTC 主导率变化
- 与美股相关性
- 宏观事件影响

## 用户画像融合（每轮思考时参考）

当前用户信息：
- 风险等级: {user_risk_level}
- 持仓情况: {user_positions}
- 情绪状态: {user_emotional_state}
- 历史偏好: {user_preferences}
- 心理价位: {psychological_prices}

融合规则：
- 用户焦虑时：语气安抚，强调历史数据和风险控制
- 用户 FOMO 时：冷静提醒，拒绝追高建议
- 用户保守型：避免激进建议，强调止损
- 用户已有重仓：分析需包含"加仓对现有持仓的影响"

## 输出格式

### 中间轮次（调用工具时）

```json
{
  "thought": "你的思考过程：当前已知什么、还需要什么、为什么选择这个工具",
  "action": "call_tool",
  "tool_name": "工具名",
  "arguments": {"参数": "值"},
  "reasoning": "调用该工具的目的和预期获取什么信息"
}
```

### 最终轮次（输出报告时）

当你认为数据已足够、可以给出完整分析时，输出：

```json
{
  "thought": "综合所有收集的数据，我认为已足够做出判断。关键发现：...",
  "action": "final_answer",
  "content": {
    "version": "2.0",
    "symbol": "BTC-USDT",
    "analysis_summary": "一句话总结",
    
    "technical_analysis": {
      "price": 67234.50,
      "change_24h_pct": -5.2,
      "key_indicators": [
        {"name": "RSI(14)", "value": 32.4, "signal": "接近超卖"},
        {"name": "MACD", "value": "-45.2/-38.1", "signal": "死叉延续"}
      ],
      "support_levels": [66800, 65000],
      "resistance_levels": [68500, 69500],
      "trend_assessment": "短期偏空，中期震荡"
    },
    
    "sentiment_analysis": {
      "fear_greed_index": 22,
      "fear_greed_label": "极度恐惧",
      "funding_rate": "0.01% (多头付费)",
      "orderbook_pressure": "卖盘占优 1.2x"
    },
    
    "onchain_macro": {
      "exchange_netflow": "净流出 (利好)",
      "btc_dominance": "52.3% (上升)"
    },
    
    "verdict": {
      "action": "hold",
      "confidence": 0.65,
      "time_horizon": "swing",
      "reasoning": "RSI接近超卖但尚未触底，资金费率偏空，建议等待更明确信号"
    },
    
    "suggested_actions": [
      {
        "action": "观望",
        "rationale": "信号混杂",
        "conditions": []
      },
      {
        "action": "小仓位试多",
        "rationale": "若回踩65000强支撑",
        "conditions": ["价格触及65000", "RSI<30且回升"]
      }
    ],
    
    "key_levels": {
      "entry_zone": [67000, 67200],
      "stop_loss": 65800,
      "take_profit": [69500, 72000],
      "risk_reward": "1:1.8"
    },
    
    "user_specific": {
      "portfolio_impact": "当前BTC浮亏4.3%，加仓可降低均价",
      "emotional_guidance": "极度恐惧后1-2周常反弹，但非买入唯一依据",
      "risk_reminder": "整体风险敞口偏高，优先控制风险"
    },
    
    "data_sources": [
      {"tool": "crypto_price", "round": 1},
      {"tool": "calculate_rsi", "round": 2},
      {"tool": "get_orderbook", "round": 3}
    ],
    
    "risk_warnings": [
      "加密货币市场波动极大，本分析不构成投资建议",
      "技术指标具有滞后性，不能预测未来价格",
      "杠杆交易可能放大亏损"
    ],
    
    "disclaimer": "本分析由AI生成，仅供参考，不构成任何投资建议。请根据自身风险承受能力做出独立判断。"
  }
}
```

## 关键规则

1. **自主决策**：不需要调用所有工具。根据用户问题和中间结果，自主判断还需要什么数据。
2. **避免重复**：维护已调用工具列表，不要重复调用相同工具（除非参数不同）。
3. **条件分支**：如果某轮结果已足够做出判断（如价格暴涨20%且RSI>80），可以提前终止并给出结论。
4. **错误处理**：如果工具返回错误，不要 panic。尝试替代方案或跳过该维度，在报告中说明。
5. **数据新鲜度**：关注工具返回的时间戳， stale 数据（>5分钟）在报告中标注。
6. **禁止确定性预测**：使用"可能"、"概率较高"、"建议关注"，不得使用"一定会"。
7. **风险优先**：高风险场景下（波动率>10%、用户情绪恐慌），谨慎给出买入建议。
8. **最多 10 轮**：你可以在 1-10 轮之间的任意时刻终止，由你判断何时数据足够。

## 思考示例

用户问："BTC 现在能买吗？"

Round 1 thought: "用户想知道 BTC 是否适合买入。首先需要了解当前价格基本情况。"
→ Action: call_tool(crypto_price, {"symbol": "BTC"})

Round 2 thought: "BTC 24h 下跌 5.2%，价格 67234。需要判断是否是抄底机会。先看 RSI 是否超卖。"
→ Action: call_tool(calculate_rsi, {"symbol": "BTC", "period": 14})

Round 3 thought: "RSI 32.4 接近超卖但未到。需要更多确认信号。看看 MACD 和订单簿。"
→ Action: call_tool(calculate_macd, {"symbol": "BTC", "fast": 12, "slow": 26, "signal": 9})

Round 4 thought: "MACD 死叉延续，空头仍在。需要看订单簿确认下方支撑强度。"
→ Action: call_tool(get_orderbook, {"symbol": "BTC", "depth": 50})

Round 5 thought: "订单簿显示卖盘占优 1.2x，但 66800 附近有大买单支撑。再看资金费率和情绪。"
→ Action: call_tool(get_funding_rate, {"symbol": "BTC"})

Round 6 thought: "资金费率 0.01% 多头付费，偏空。恐惧贪婪指数也很关键。"
→ Action: call_tool(get_fear_greed_index)

Round 7 thought: "恐惧贪婪指数 22（极度恐惧），历史数据显示极度恐惧后常有反弹。但技术面仍偏空。综合判断：短期偏空，不宜追高，可小仓位试多或观望。数据已足够，输出报告。"
→ Action: final_answer({...})
```

### 3.3 与现有代码的衔接

现有代码已有 `ReActExecutor`（`skills/react_executor.rs`）和 `ReActAgent`（`runtime/react_framework.rs`），但存在以下差距：

| 现有实现 | 需要调整 |
|---------|---------|
| `ReActExecutor` 使用纯文本 `ACTION: / PARAMETERS:` 格式 | 改为 JSON 格式（thought + action + tool_name + arguments） |
| `ReActConfig.max_steps = 10` | 改为 10 |
| `ReActAgent` 有预定义步骤模板（`build_react_steps`） | 删除预定义步骤，完全由 LLM 每轮自主决定 |
| 无结构化 final_answer 约束 | 增加 JSON Schema 校验 |
| 无用户画像注入 | 每轮 System Prompt 中注入用户上下文 |
| 无工具调用历史去重 | 增加已调用工具列表到 prompt |

---

## 4. 其他场景的 ReAct 统一应用

### 4.1 交易表单提交（ReAct 模式）

**场景**：用户说"买 100 美元 BTC"，但缺少 side/qty 等参数，或参数不完整。

**传统做法**：McpParameterExtractor 一次性提取 → 缺失则渲染表单。

**ReAct 改进做法**：

```markdown
# ROLE: BeeAgentOS Transaction Form Assistant

你是交易表单处理助手。你的工作是通过多轮对话收集用户下单所需的完整参数。

## 可用工具
- `check_parameters(params: object)` → 返回：缺失参数列表 + 格式错误提示
- `render_form(missing_fields: array)` → 返回：渲染后的表单文本
- `preview_order(symbol, side, qty, price, type)` → 返回：订单预览卡片
- `submit_order(symbol, side, qty, price, type)` → 返回：下单结果

## 工作规则
1. 收到用户请求后，先检查参数完整性
2. 如有缺失，以自然语言询问（而非一次性抛出所有字段）
3. 每轮只问 1-2 个关键缺失字段，保持对话流畅
4. 参数齐全后，生成预览卡片供用户确认
5. 用户确认后，执行下单

## 输出格式
```json
{
  "thought": "用户想买入BTC，但只说了金额，缺少交易方向（虽然'买'暗示了buy）。先检查参数。",
  "action": "call_tool",
  "tool_name": "check_parameters",
  "arguments": {"symbol": "BTC/USD", "side": "buy", "notional": 100},
  "reasoning": "验证参数完整性"
}
```
```

**ReAct 流程示例**：

```
Round 1: 用户说"买 100 美元 BTC"
  → LLM: "参数可能不完整，先检查"
  → Action: check_parameters({symbol:"BTC/USD", side:"buy", notional:100})
  → Observation: {"missing": ["type"], "warnings": ["未指定订单类型，默认market"]}

Round 2: 
  → LLM: "用户没指定订单类型，问一下"
  → Action: final_answer({"message": "好的，为您买入 100 美元 BTC。请问使用市价单还是限价单？"})
  
用户回复: "市价单"
→ 进入新一轮 ReAct（或直接进入执行流程）
```

### 4.2 下单确认流程（ReAct 模式）

**场景**：高价值/高风险交易需要用户二次确认。

```markdown
# ROLE: BeeAgentOS High-Risk Transaction Confirmer

你是高风险交易确认助手。你需要生成清晰的订单预览，等待用户确认。

## 可用工具
- `generate_preview(skill_id, tool_name, params)` → 返回：格式化预览
- `wait_user_confirmation(timeout_sec)` → 返回：用户回复（确认/取消/修改）
- `execute_order(skill_id, params)` → 返回：执行结果
- `cancel_order(reason)` → 返回：取消确认

## 工作规则
1. 先生成中文预览卡片，包含所有关键参数
2. 明确告知风险（资金不可逆）
3. 等待用户回复
4. 用户说"确认"/"执行"/"下单" → 执行
5. 用户说"取消"/"不" → 取消并告知
6. 用户要求修改 → 重新进入参数收集循环
```

### 4.3 通用多步任务（ReAct 模式）

**场景**：Skill Selector 返回 `needs_planning: yes` 的任何任务。

```rust
// 在 agent_impl.rs 中的统一路由逻辑
match (intent, skill_result) {
    // 单步任务：直接执行
    (SingleToolCall, Some(skill)) => execute_single_step(skill).await,
    
    // 多步任务：进入统一 ReAct 引擎
    (MultiStepPlanning, Some(skill)) => {
        let tools = load_mcp_tools_for_skill(&skill);
        let system_prompt = build_system_prompt_for_domain(&skill.domain); // 交易/天气/股票等
        unified_react_executor.execute(system_prompt, user_input, &tools, &user_context).await
    }
    
    // 交易分析专用路由（即使 intent 是 SingleToolCall，但涉及交易分析）
    (_, Some(skill)) if skill.domain == "crypto-trading" && user_input.contains("分析") => {
        let tools = load_all_crypto_data_tools();
        let system_prompt = build_investment_analysis_prompt();
        unified_react_executor.execute(system_prompt, user_input, &tools, &user_context).await
    }
}
```

---

## 5. 各阶段 Prompt 详细模板

### 5.1 初始轮次 Prompt 构建

```rust
fn build_initial_prompt(
    system_prompt: &str,           // 场景专用（投资分析 / 表单 / 确认）
    user_request: &str,
    available_tools: &[ToolDef],
    user_context: &UserContext,
) -> String {
    let tools_desc = available_tools.iter()
        .map(|t| format!("- `{}({})` → {}\n  参数: {}\n  返回: {}",
            t.name, t.param_schema, t.description, t.param_desc, t.return_desc))
        .collect::<Vec<_>>()
        .join("\n");
    
    format!(r#"{system_prompt}

## 可用工具
{tools_desc}

## 用户信息
- 风险等级: {risk_level}
- 持仓: {positions}
- 情绪状态: {emotional_state}
- 历史偏好: {preferences}

## 当前任务
用户输入: "{user_request}"

请输出你的思考过程和下一步行动（call_tool 或 final_answer）。
"#)
}
```

### 5.2 中间轮次 Prompt 构建

```rust
fn build_round_prompt(rounds: &[ReActRound], user_request: &str) -> String {
    let mut history = String::new();
    
    for round in rounds {
        history.push_str(&format!(
            "### Round {}\n" +
            "Thought: {}\n" +
            "Action: {}({})\n" +
            "Observation: {}\n\n",
            round.round_number,
            round.llm_thought,
            match &round.action {
                ReActAction::CallTool { tool_name, .. } => tool_name,
                ReActAction::FinalAnswer { .. } => "final_answer",
            },
            round.observation.as_deref().unwrap_or("N/A")
        ));
    }
    
    format!(r#"## 历史执行记录
{history}

## 当前状态
基于以上已执行的工具调用和返回结果，请决定下一步：
- 如果还需要更多数据：调用一个工具（call_tool）
- 如果数据已足够：输出最终分析（final_answer）
- 如果已达最大轮数限制：必须输出 final_answer

请输出 JSON 格式。"#)
}
```

### 5.3 LLM 响应解析

```rust
fn parse_react_response(response: &str) -> Result<ParsedReAct, ParseError> {
    // 尝试从 markdown code block 中提取 JSON
    let json_str = if response.contains("```json") {
        extract_codeblock(response, "json")
    } else if response.contains("```") {
        extract_codeblock(response, "")
    } else {
        response.to_string()
    };
    
    let value: serde_json::Value = serde_json::from_str(&json_str)?;
    
    let thought = value["thought"].as_str().unwrap_or("").to_string();
    let action = value["action"].as_str().ok_or(ParseError::MissingAction)?;
    
    match action {
        "call_tool" => {
            let tool_name = value["tool_name"].as_str().ok_or(ParseError::MissingToolName)?.to_string();
            let arguments = value["arguments"].as_object().cloned().unwrap_or_default();
            let reasoning = value["reasoning"].as_str().unwrap_or("").to_string();
            Ok(ParsedReAct {
                thought,
                action: ReActAction::CallTool { tool_name, arguments, reasoning },
            })
        }
        "final_answer" => {
            let content = value["content"].to_string();
            Ok(ParsedReAct {
                thought,
                action: ReActAction::FinalAnswer { content },
            })
        }
        _ => Err(ParseError::UnknownAction(action.to_string())),
    }
}
```

### 5.4 错误恢复 Prompt

当工具执行失败时，回注给 LLM 的 observation：

```markdown
[Observation] 工具执行结果:
ERROR: {error_message}

该工具调用失败。请决定：
1. 使用不同参数重试同一工具
2. 调用替代工具获取类似数据
3. 跳过该维度，在最终报告中标注"数据缺失"
4. 如果关键数据缺失导致无法分析，输出 final_answer 说明限制
```

---

## 6. 上下文组装与记忆管理

### 6.1 每轮上下文结构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  SYSTEM PROMPT (固定，每轮都注入)                                            │
│  - 角色定义、分析框架、可用工具列表、约束规则                                │
│  - 用户信息（风险等级、持仓、情绪）                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  USER: 初始任务描述                                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  ASSISTANT: Round 1 输出 → call_tool(...)                                    │
│  USER: [Observation] Round 1 工具结果                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ASSISTANT: Round 2 输出 → call_tool(...)                                    │
│  USER: [Observation] Round 2 工具结果                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ...                                                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│  ASSISTANT: Round N 输出 → final_answer({...})                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 上下文窗口管理

kimi-k2.6 上下文 256K，10 轮 ReAct 的 token 预估：

| 内容 | 每轮预估 tokens | 10 轮累计 |
|------|----------------|---------|
| System Prompt（含工具列表） | ~2,000 | 2,000（只注入一次） |
| 用户信息 | ~300 | 300 |
| 每轮 thought + action + observation | ~1,500 | ~12,000 |
| final_answer | ~2,500 | 2,500 |
| **总计** | - | **~17,000** |

256K 上下文绰绰有余。无需历史截断。

### 6.3 已调用工具去重

每轮在 System Prompt 中追加：
```markdown
## 已调用工具（请勿重复）
- Round 1: crypto_price({"symbol":"BTC"}) → 已获取价格数据
- Round 2: calculate_rsi({"symbol":"BTC","period":14}) → 已获取 RSI
```

LLM 在 thought 中应检查该列表，避免重复调用。



---

## 7. 安全合规与风控护栏

### 7.1 ReAct 循环中的安全控制

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        ReAct 安全控制层                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│  1. 工具权限校验                                                             │
│     - 只暴露当前场景允许的工具（交易分析场景不暴露 execute_trade）           │
│     - 高危险工具（execute_trade/transfer）需额外权限标记                     │
├─────────────────────────────────────────────────────────────────────────────┤
│  2. 输出格式校验                                                             │
│     - 每轮 LLM 输出必须可解析为 JSON                                         │
│     - final_answer 必须是合法 JSON（投资决策场景）                           │
│     - 解析失败时要求 LLM 重试                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│  3. 内容安全过滤                                                             │
│     - final_answer 中检测违规词汇（"稳赚"、"肯定会"）                         │
│     - 若检测到，要求 LLM 修正后重新输出                                      │
│     - 强制追加 disclaimer（即使 LLM 未输出）                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  4. 分析 vs 执行隔离                                                         │
│     - ReAct 投资分析引擎的工具列表**不包含** execute_trade                   │
│     - 分析结果只输出建议，不执行任何交易                                     │
│     - 交易执行走独立的 MCP Execution + Approval Gate 流程                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  5. 审计追踪                                                                 │
│     - 每轮 ReActRound 记录到审计日志                                         │
│     - 包含：thought、action、observation、timestamp                          │
│     - 全链路 request_id 关联                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 7.2 final_answer 后处理

```rust
fn post_process_final_answer(content: &str, user_risk_level: &str) -> Result<String, PostProcessError> {
    // 1. JSON 校验
    let json: serde_json::Value = serde_json::from_str(content)?;
    
    // 2. 必填字段检查
    let required = ["verdict", "risk_warnings", "disclaimer"];
    for field in required {
        if json.get(field).is_none() {
            return Err(PostProcessError::MissingField(field));
        }
    }
    
    // 3. 风险评分门控
    let risk_score = json["risk_assessment"]["overall_risk_score"].as_f64().unwrap_or(5.0);
    let verdict_action = json["verdict"]["action"].as_str().unwrap_or("hold");
    if risk_score >= 7.0 && (verdict_action == "buy" || verdict_action == "strong_buy") {
        // 强制修正 verdict
        let mut fixed = json.clone();
        fixed["verdict"]["action"] = json!("hold");
        fixed["verdict"]["reasoning"] = json!(
            format!("{} 风险评分 {:.1} 超过阈值，系统已将建议修正为观望。", 
                    fixed["verdict"]["reasoning"].as_str().unwrap_or(""), risk_score)
        );
        return Ok(fixed.to_string());
    }
    
    // 4. 合规词汇过滤
    let content_lower = content.to_lowercase();
    let banned_words = ["稳赚", "肯定会", "绝对", "100%", "零风险", "保证"];
    for word in banned_words {
        if content_lower.contains(word) {
            return Err(PostProcessError::BannedWord(word));
        }
    }
    
    // 5. 免责声明兜底
    let mut final_json = json.clone();
    if final_json["disclaimer"].as_str().unwrap_or("").is_empty() {
        final_json["disclaimer"] = json!("本分析由AI生成，仅供参考，不构成任何投资建议。加密货币投资具有高风险，可能导致本金全部损失。");
    }
    
    Ok(final_json.to_string())
}
```

---

## 8. 实现路线图

### Phase 1: 基础设施（已完成 ✅）
- [x] MCP Parameter Extractor
- [x] MCP 两阶段执行（预览 + 确认）
- [x] Approval Gate
- [x] Skill Selector 简化
- [x] ReActExecutor（基础版，纯文本格式）
- [x] ReActAgent（runtime 框架）

### Phase 2: 统一 ReAct 引擎升级（当前 🔄）
- [ ] 重构 `ReActExecutor`：支持 JSON 格式输入输出
- [ ] 删除 `ReActPlanner.build_react_steps()` 预定义步骤，改为 LLM 自主决策
- [ ] 实现 `UnifiedReActExecutor`：统一执行引擎
- [ ] 实现上下文逐轮累积（thought + action + observation）
- [ ] 实现已调用工具去重机制
- [ ] 实现错误恢复（工具失败后 LLM 自主决策重试/跳过/终止）
- [ ] 实现强制终止（达到 max_rounds 后强制 final_answer）

### Phase 3: 投资决策分析场景（下一步 🎯）
- [ ] 设计并固化 3.2 节 System Prompt
- [ ] 实现 MCP crypto data tools（price, ohlcv, rsi, macd, bollinger, atr, orderbook, funding, fear_greed, long_short, exchange_flow, btc_dominance）
- [ ] 实现工具描述自动注入（每个工具的参数、返回、示例）
- [ ] 实现 final_answer JSON Schema 校验
- [ ] 实现用户报告格式化器（JSON → Markdown）
- [ ] 集成用户画像系统到 System Prompt

### Phase 4: 表单与确认场景（统一应用）
- [ ] 交易表单提交 ReAct System Prompt
- [ ] 下单确认 ReAct System Prompt
- [ ] 多币种对比分析 ReAct System Prompt
- [ ] 通用多步任务路由（Skill Selector `needs_planning: yes` → ReAct）

### Phase 5: 意图层与路由（后续）
- [ ] 增强 Intent Analyzer：识别需要 ReAct 的复杂意图
- [ ] 增强 Skill Selector：为 ReAct 场景注入可用工具列表
- [ ] 实现场景自动路由（交易分析 → 投资分析 Prompt / 表单 → 表单 Prompt）

### Phase 6: 安全与审计（持续）
- [ ] final_answer 后处理（风险评分门控、合规词汇过滤、免责声明兜底）
- [ ] ReAct 全链路审计日志（每轮 thought/action/observation）
- [ ] 工具调用权限矩阵（不同场景暴露不同工具子集）
- [ ] 异常检测（LLM 进入死循环时人工介入）

### Phase 7: 优化迭代
- [ ] A/B 测试不同 System Prompt 对分析质量的影响
- [ ] 基于用户反馈的 prompt 微调
- [ ] 热门分析场景缓存（相同 query 直接复用最近结果）
- [ ] 前端 ReAct 过程可视化（展示 LLM 的思考过程和工具调用链）

---

## 附录 A: 快速参考

### ReAct 输出格式速查

**调用工具：**
```json
{
  "thought": "为什么需要这个工具",
  "action": "call_tool",
  "tool_name": "crypto_price",
  "arguments": {"symbol": "BTC"},
  "reasoning": "获取当前价格作为分析基础"
}
```

**最终答案：**
```json
{
  "thought": "数据已足够，可以输出报告",
  "action": "final_answer",
  "content": { /* 结构化分析结果 */ }
}
```

### 场景与 System Prompt 映射

| 场景 | System Prompt 文件 | 可用工具 | max_rounds |
|------|-------------------|---------|-----------|
| 加密货币投资分析 | `prompts/investment_analysis.md` | 全部 data tools（不含 execute） | 8 |
| 交易表单提交 | `prompts/transaction_form.md` | check_parameters, render_form | 5 |
| 下单确认 | `prompts/order_confirmation.md` | preview_order, wait_confirmation, execute_order | 4 |
| 通用多步任务 | `prompts/generic_react.md` | 由 Skill 动态决定 | 6 |

### 与现有模块对接

| 新组件 | 现有基础 | 改动点 |
|--------|---------|--------|
| `UnifiedReActExecutor` | `skills/react_executor.rs` | JSON 格式、10轮上限、去重、强制终止 |
| 投资分析 Prompt | `docs/evolution/LLM-trade/` | 新增 3.2 节完整 System Prompt |
| final_answer 校验 | `agent_impl.rs` 解析逻辑 | 新增 JSON Schema 校验 + 后处理 |
| 工具权限控制 | `security/approval.rs` | 新增场景-工具映射矩阵 |
