//! Investment Analysis Post-Processor
//!
//! Validates and sanitizes the final_answer output from the ReAct loop.

use tracing::{info, warn};

use crate::error::AgentError;
use crate::skills::investment_analysis::types::{
    InvestmentAnalysisReport, HIGH_RISK_THRESHOLD, VERDICT_ACTIONS,
};

/// Post-process the LLM's final_answer content
pub fn post_process_final_answer(
    content: &str,
    user_risk_level: &str,
) -> Result<String, AgentError> {
    // 1. JSON parse validation
    let mut json: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            warn!("final_answer JSON parse failed: {}", e);
            let extracted = extract_json_from_codeblock(content);
            match serde_json::from_str(&extracted) {
                Ok(v) => v,
                Err(e2) => {
                    return Err(AgentError::Execution(format!(
                        "final_answer is not valid JSON: {} (extraction: {})",
                        e, e2
                    )));
                }
            }
        }
    };

    // 2. Required field presence check
    let required_fields = ["verdict", "risk_warnings", "disclaimer"];
    for field in required_fields {
        if json.get(field).is_none() {
            warn!("final_answer missing required field: {}", field);
            return Err(AgentError::Execution(format!(
                "final_answer missing required field: {}",
                field
            )));
        }
    }

    // 3. Risk score gating
    let risk_score = json
        .get("risk_assessment")
        .and_then(|r| r.get("overall_risk_score"))
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

    let verdict_action: String = json
        .get("verdict")
        .and_then(|v| v.get("action"))
        .and_then(|v| v.as_str())
        .unwrap_or("hold")
        .to_string();

    if risk_score >= HIGH_RISK_THRESHOLD
        && (verdict_action == "buy" || verdict_action == "strong_buy")
    {
        warn!(
            "Risk score {:.1} >= threshold, downgrading '{}' to 'hold'",
            risk_score, verdict_action
        );
        if let Some(verdict) = json.get_mut("verdict") {
            verdict["action"] = serde_json::json!("hold");
            let original = verdict
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            verdict["reasoning"] = serde_json::json!(format!(
                "{} 风险评分 {:.1} 超过阈值，系统已将建议修正为观望。",
                original, risk_score
            ));
        }
        if let Some(warnings) = json.get_mut("risk_warnings").and_then(|w| w.as_array_mut()) {
            warnings.push(serde_json::json!(format!(
                "[系统修正] 当前风险评分 {:.1} 偏高，已自动将买入建议调整为观望。",
                risk_score
            )));
        }
    }

    if user_risk_level == "conservative"
        && (verdict_action == "buy" || verdict_action == "strong_buy")
    {
        info!(
            "Conservative user, downgrading '{}' to 'hold'",
            verdict_action
        );
        if let Some(verdict) = json.get_mut("verdict") {
            verdict["action"] = serde_json::json!("hold");
        }
    }

    // 4. Banned word filtering
    let content_lower = json.to_string().to_lowercase();
    let banned_words = [
        "稳赚",
        "肯定会",
        "绝对",
        "100%赚",
        "零风险",
        "保证",
        "稳赢",
        "一定涨",
        "必涨",
        "铁定",
        "绝对会",
        "不可能跌",
    ];
    for word in banned_words {
        if content_lower.contains(word) {
            warn!("Banned word detected: {}", word);
            return Err(AgentError::Execution(format!(
                "final_answer contains banned word: '{}'",
                word
            )));
        }
    }

    // 5. Disclaimer fallback
    let disclaimer = json
        .get("disclaimer")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if disclaimer.is_empty() {
        warn!("Disclaimer missing, injecting fallback");
        json["disclaimer"] = serde_json::json!(
            "本分析由AI生成，仅供参考，不构成任何投资建议。加密货币投资具有高风险，\
             可能导致本金全部损失。请根据自身风险承受能力做出独立判断。"
        );
    }

    if let Some(warnings) = json.get_mut("risk_warnings").and_then(|w| w.as_array_mut()) {
        while warnings.len() < 3 {
            warnings.push(serde_json::json!("加密货币市场波动极大，投资需谨慎。"));
        }
    }

    if !VERDICT_ACTIONS.contains(&verdict_action.as_str()) {
        warn!("Invalid verdict.action: {}", verdict_action);
        if let Some(verdict) = json.get_mut("verdict") {
            verdict["action"] = serde_json::json!("uncertain");
        }
    }

    Ok(json.to_string())
}

fn extract_json_from_codeblock(content: &str) -> String {
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if end > start {
                return content[start..=end].to_string();
            }
        }
    }
    content.to_string()
}

/// Format the JSON report as a user-friendly Markdown message
pub fn format_report_for_user(report_json: &str) -> Result<String, AgentError> {
    let report: InvestmentAnalysisReport = serde_json::from_str(report_json)
        .map_err(|e| AgentError::Execution(format!("Failed to parse report: {}", e)))?;

    let mut lines = Vec::new();
    lines.push(format!("📊 **{} 投资分析报告**\n", report.symbol));
    lines.push(format!(
        "🎯 **综合判断：{}**\n",
        report.verdict.action.to_uppercase()
    ));
    if let Some(reasoning) = &report.verdict.reasoning {
        lines.push(format!("{}", reasoning));
    }
    lines.push(String::new());

    if let Some(tech) = &report.technical_analysis {
        lines.push("📈 **技术指标**".to_string());
        lines.push(format!(
            "- 当前价格: ${:.2} ({:.1}% 24h)",
            tech.price, tech.change_24h_pct
        ));
        for indicator in &tech.key_indicators {
            lines.push(format!(
                "- {}: {} ({})",
                indicator.name, indicator.value, indicator.signal
            ));
        }
        if !tech.support_levels.is_empty() {
            let ls: Vec<String> = tech
                .support_levels
                .iter()
                .map(|p| format!("${:.0}", p))
                .collect();
            lines.push(format!("- 支撑位: {}", ls.join(", ")));
        }
        if !tech.resistance_levels.is_empty() {
            let ls: Vec<String> = tech
                .resistance_levels
                .iter()
                .map(|p| format!("${:.0}", p))
                .collect();
            lines.push(format!("- 阻力位: {}", ls.join(", ")));
        }
        if let Some(trend) = &tech.trend_assessment {
            lines.push(format!("- 趋势: {}", trend));
        }
        lines.push(String::new());
    }

    if let Some(sent) = &report.sentiment_analysis {
        lines.push("😰 **市场情绪**".to_string());
        if let Some(fgi) = sent.fear_greed_index {
            if let Some(label) = &sent.fear_greed_label {
                lines.push(format!("- 恐惧贪婪指数: {} ({})", fgi, label));
            } else {
                lines.push(format!("- 恐惧贪婪指数: {}", fgi));
            }
        }
        if let Some(fr) = &sent.funding_rate {
            lines.push(format!("- 资金费率: {}", fr));
        }
        if let Some(ob) = &sent.orderbook_pressure {
            lines.push(format!("- 订单簿: {}", ob));
        }
        lines.push(String::new());
    }

    if let Some(levels) = &report.key_levels {
        lines.push("💰 **关键价位**".to_string());
        if let Some(entry) = &levels.entry_zone {
            let e: Vec<String> = entry.iter().map(|p| format!("${:.0}", p)).collect();
            lines.push(format!("- 入场区间: {}", e.join(" - ")));
        }
        if let Some(sl) = levels.stop_loss {
            lines.push(format!("- 止损位: ${:.0}", sl));
        }
        if let Some(tp) = &levels.take_profit {
            let t: Vec<String> = tp.iter().map(|p| format!("${:.0}", p)).collect();
            lines.push(format!("- 目标位: {}", t.join(", ")));
        }
        if let Some(rr) = &levels.risk_reward {
            lines.push(format!("- 盈亏比: {}", rr));
        }
        lines.push(String::new());
    }

    if !report.suggested_actions.is_empty() {
        lines.push("💡 **建议操作**".to_string());
        for (i, action) in report.suggested_actions.iter().enumerate() {
            lines.push(format!("{}. {}", i + 1, action.action));
            lines.push(format!("   理由: {}", action.rationale));
            if !action.conditions.is_empty() {
                lines.push(format!("   条件: {}", action.conditions.join(", ")));
            }
        }
        lines.push(String::new());
    }

    if let Some(user) = &report.user_specific {
        lines.push("👤 **个性化建议**".to_string());
        if let Some(impact) = &user.portfolio_impact {
            lines.push(format!("- {}", impact));
        }
        if let Some(guidance) = &user.emotional_guidance {
            lines.push(format!("- {}", guidance));
        }
        if let Some(reminder) = &user.risk_reminder {
            lines.push(format!("- {}", reminder));
        }
        lines.push(String::new());
    }

    if !report.risk_warnings.is_empty() {
        lines.push("⚠️ **风险提示**".to_string());
        for warning in &report.risk_warnings {
            lines.push(format!("- {}", warning));
        }
        lines.push(String::new());
    }

    lines.push("📝 **免责声明**".to_string());
    lines.push(report.disclaimer.clone());

    Ok(lines.join("\n"))
}
