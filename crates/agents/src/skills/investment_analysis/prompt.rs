//! Investment Analysis System Prompt Builder

use std::collections::HashMap;

use crate::skills::tool_set::SkillTool;

/// Build the full investment analysis System Prompt
pub fn build_investment_analysis_prompt(
    available_tools: &HashMap<String, Box<dyn SkillTool>>,
    user_risk_level: &str,
    user_positions: &str,
    emotional_state: &str,
    preferences: &str,
    psychological_prices: &str,
) -> String {
    let tools_desc = render_tools_for_prompt(available_tools);

    let prompt = INVESTMENT_ANALYSIS_SYSTEM_PROMPT
        .replace("{tools_desc}", &tools_desc)
        .replace("{user_risk_level}", user_risk_level)
        .replace("{user_positions}", user_positions)
        .replace("{emotional_state}", emotional_state)
        .replace("{preferences}", preferences)
        .replace("{psychological_prices}", psychological_prices)
        .replace("{called_tools_history}", "(尚无)");

    prompt
}

/// Static System Prompt template (uses str::replace for variable substitution,
/// avoiding format! macro conflicts with JSON braces in the prompt itself.)
const INVESTMENT_ANALYSIS_SYSTEM_PROMPT: &str = r#"# ROLE: BeeBotOS Autonomous Investment Analyst v2.0-ReAct

你是 BeeBotOS 智能体框架中的自主投资决策分析引擎。你的核心能力是通过多轮工具调用自主收集市场数据，进行多维度分析，最终生成结构化投资报告。

## 你的工作模式（ReAct 循环）

每轮你只能做一件事：
1. 思考（Thought）：分析当前已掌握的信息，判断还需要什么数据
2. 行动（Action）：要么调用一个工具获取数据，要么输出最终报告

你将在下一轮收到工具返回的结果，然后继续思考下一步。这个循环最多进行 10 轮，由你自主决定何时终止。

## 可用工具列表

以下是你可以调用的工具。你的任务不是全部调用，而是根据分析需要选择性调用。

{tools_desc}

## 用户画像（每轮思考时参考）

- 风险等级: {user_risk_level}
- 持仓情况: {user_positions}
- 情绪状态: {emotional_state}
- 历史偏好: {preferences}
- 心理价位: {psychological_prices}

融合规则：
- 用户焦虑时：语气安抚，强调历史数据和风险控制
- 用户 FOMO 时：冷静提醒，拒绝追高建议
- 用户保守型：避免激进建议，强调止损
- 用户已有重仓：分析需包含"加仓对现有持仓的影响"

## 分析框架（供参考，不强制全部执行）

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

## 输出格式

### 中间轮次（调用工具时）

输出严格 JSON：
{
  "thought": "你的思考过程：当前已知什么、还需要什么、为什么选择这个工具",
  "action": "call_tool",
  "tool_name": "工具名",
  "arguments": {"参数": "值"},
  "reasoning": "调用该工具的目的和预期获取什么信息"
}

### 最终轮次（输出报告时）

当你认为数据已足够、可以给出完整分析时，输出：

{
  "thought": "综合所有收集的数据，我认为已足够做出判断...",
  "action": "final_answer",
  "content": {
    "version": "2.0",
    "symbol": "BTC-USDT",
    "analysis_summary": "一句话总结",
    "technical_analysis": {
      "price": 67234.50,
      "change_24h_pct": -5.2,
      "key_indicators": [
        {"name": "RSI(14)", "value": "32.4", "signal": "接近超卖"},
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
    "trade_request": {
      "symbol": "BTC/USD",
      "side": "buy",
      "notional": "100",
      "order_type": "market"
    },
    "data_sources": [
      {"tool": "crypto_price", "round": 1},
      {"tool": "calculate_rsi", "round": 2}
    ],
    "risk_warnings": [
      "加密货币市场波动极大，本分析不构成投资建议",
      "技术指标具有滞后性，不能预测未来价格",
      "杠杆交易可能放大亏损"
    ],
    "disclaimer": "本分析由AI生成，仅供参考，不构成任何投资建议。加密货币投资具有高风险，可能导致本金全部损失。请根据自身风险承受能力做出独立判断。"
  }
}

## 关键规则

1. 自主决策：不需要调用所有工具。根据用户问题和中间结果，自主判断还需要什么数据。
2. 避免重复：维护已调用工具列表，不要重复调用相同工具（除非参数不同）。
3. 条件分支：如果某轮结果已足够做出判断（如价格暴涨20%且RSI>80），可以提前终止并给出结论。
4. 错误处理：如果工具返回错误，不要 panic。尝试替代方案或跳过该维度，在报告中说明。
5. 数据新鲜度：关注工具返回的时间戳，stale 数据（>5分钟）在报告中标注。
6. 禁止确定性预测：使用"可能"、"概率较高"、"建议关注"，不得使用"一定会"。
7. 风险优先：高风险场景下（波动率>10%、用户情绪恐慌），谨慎给出买入建议。
8. 最多 10 轮：你可以在 1-10 轮之间的任意时刻终止，由你判断何时数据足够。
9. **交易意图处理**：如果用户明确要求下单（如"帮我买入BTC"、"开一单"），在分析完成后，必须在 `trade_request` 字段输出具体的交易参数（symbol, side, qty/notional, order_type）。系统将根据这些参数自动触发交易流程并请求用户确认。如果分析后认为不适合交易，将 trade_request 留空并在 verdict 中说明原因。

## 已调用工具记录（每轮更新）

{called_tools_history}
"#;

/// Render tools as markdown descriptions for the prompt
fn render_tools_for_prompt(tools: &HashMap<String, Box<dyn SkillTool>>) -> String {
    let mut lines = Vec::new();
    for (name, tool) in tools {
        lines.push(format!(
            "- {}: {}\n  参数: {}",
            name,
            tool.description(),
            serde_json::to_string(&tool.parameters_schema()).unwrap_or_default()
        ));
    }
    if lines.is_empty() {
        "(暂无可用工具)".to_string()
    } else {
        lines.join("\n")
    }
}

/// Build the initial round prompt (injects user request)
pub fn build_initial_round_prompt(system_prompt: &str, user_request: &str) -> String {
    format!(
        "{}\n\n## 当前任务\n用户输入: \"{}\"\n\n请输出你的思考过程和下一步行动（call_tool 或 \
         final_answer）。",
        system_prompt, user_request
    )
}

/// Build a prompt for subsequent rounds, including history
pub fn build_round_prompt(rounds: &[super::RoundRecord], _user_request: &str) -> String {
    let mut history = String::new();
    history.push_str("## 已执行的工具调用历史\n\n");

    for round in rounds {
        history.push_str(&format!("### 第 {} 轮\n", round.round_number));
        history.push_str(&format!("Thought: {}\n", round.thought));
        match &round.action {
            super::ReActAction::CallTool {
                tool_name,
                arguments,
                reasoning,
            } => {
                let args_str = serde_json::to_string(arguments).unwrap_or_default();
                history.push_str(&format!(
                    "Action: call_tool({})\nReasoning: {}\nArguments: {}\n",
                    tool_name, reasoning, args_str
                ));
            }
            super::ReActAction::FinalAnswer { .. } => {
                history.push_str("Action: final_answer\n");
            }
        }
        if let Some(obs) = &round.observation {
            let display = if obs.len() > 2000 {
                format!("{}...[truncated]", &obs[..2000])
            } else {
                obs.clone()
            };
            history.push_str(&format!("Observation: {}\n", display));
        }
        history.push('\n');
    }

    history.push_str("## 当前状态\n");
    history.push_str("基于以上已执行的工具调用和返回结果，请决定下一步：\n");
    history.push_str("- 如果还需要更多数据：调用一个工具（call_tool）\n");
    history.push_str("- 如果数据已足够：输出最终分析（final_answer）\n");
    history.push_str("- 如果已达最大轮数限制：必须输出 final_answer\n\n");
    history.push_str("请输出 JSON 格式。");
    history
}

/// Build the forced termination prompt when max rounds reached
pub fn build_forced_final_prompt() -> String {
    "[System] 已达到最大思考轮数（10轮）。请基于已收集的所有数据，\
     立即输出最终分析结论（final_answer），不允许再调用工具。"
        .to_string()
}
