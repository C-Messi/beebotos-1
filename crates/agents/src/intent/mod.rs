//! Intent Engine Module
//!
//! Lightweight but precise intent classification layer that guides subsequent
//! processing path selection before the message enters the LLM main loop.
//!
//! Dual-track strategy:
//! - **Rule engine (lightweight)**: High-frequency, pattern-fixed intents
//!   (regex + keywords + negation detection, no LLM needed)
//! - **LLM classifier (precise)**: Complex, ambiguous intents
//!   (call small model or main model's fast classification mode)

use std::collections::HashMap;

/// User intent classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIntent {
    /// Chit-chat/greetings/simple Q&A, no tools needed
    DirectAnswer,
    /// Single-step tool call (weather query, stock price, etc.)
    SingleToolCall,
    /// Multi-step complex task (needs Planning)
    MultiStepPlanning,
    /// Trigger predefined Workflow
    WorkflowTrigger,
    /// Meta-questions about the system itself ("what can you do")
    MetaQuestion,
    /// Negation/correction指令 ("don't query, just place order")
    Correction,
}

/// Result of intent analysis
#[derive(Debug, Clone)]
pub struct IntentAnalysis {
    pub intent: UserIntent,
    /// Extracted entities (e.g. symbol=BTC/USD, side=buy)
    pub entities: HashMap<String, String>,
    /// User-specified constraints (e.g. "don't query first")
    pub constraints: Vec<String>,
    /// Confidence 0.0-1.0
    pub confidence: f32,
    /// Detected toolsets (for tool filtering)
    pub active_toolsets: Vec<String>,
}

impl IntentAnalysis {
    pub fn new(intent: UserIntent, confidence: f32) -> Self {
        Self {
            intent,
            entities: HashMap::new(),
            constraints: Vec::new(),
            confidence,
            active_toolsets: Vec::new(),
        }
    }

    pub fn with_entities(mut self, entities: HashMap<String, String>) -> Self {
        self.entities = entities;
        self
    }

    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_toolsets(mut self, toolsets: Vec<String>) -> Self {
        self.active_toolsets = toolsets;
        self
    }
}

/// Predefined toolsets with trigger keywords
const DEFAULT_TOOLSETS: &[(&str, &[&str])] = &[
    ("account", &["账户", "account", "余额", "balance", "portfolio"]),
    ("trading", &["下单", "购买", "买入", "卖出", "order", "buy", "sell", "place", "交易"]),
    ("watchlists", &["自选", "watchlist", "关注"]),
    ("stock-data", &["股票", "股价", "stock", "AAPL", "TSLA"]),
    ("crypto-data", &["比特币", "BTC", "以太坊", "ETH", "crypto", "加密货币"]),
    ("options-data", &["期权", "option", "call", "put"]),
    ("news", &["新闻", "news", "头条"]),
    ("weather", &["天气", "weather", "temperature", "预报"]),
    // 🆕 FIX (Plan D): Web search toolset — triggers when user asks to search the web
    ("search", &["搜索", "查找", "查一下", "网上", "google", "search", "look up", "find online", "查", "搜", "百度"]),
];

/// Intent Engine with heuristic classification
pub struct IntentEngine;

impl IntentEngine {
    pub fn new() -> Self {
        Self
    }

    /// Classify intent using lightweight heuristic rules (no LLM call)
    pub fn classify_heuristic(query: &str) -> IntentAnalysis {
        let lower = query.to_lowercase();

        // 1. Negation/correction detection
        if Self::is_correction(&lower) {
            let mut analysis = IntentAnalysis::new(UserIntent::Correction, 0.85);
            analysis.constraints = Self::extract_constraints(&lower);
            analysis.active_toolsets = Self::detect_toolsets(&lower);
            return analysis;
        }

        // 2. Meta-question detection
        if Self::is_meta_question(&lower) {
            return IntentAnalysis::new(UserIntent::MetaQuestion, 0.9);
        }

        // 3. Workflow trigger detection (starts with "/" or matches workflow name)
        if query.starts_with('/') || Self::matches_workflow_name(&lower) {
            return IntentAnalysis::new(UserIntent::WorkflowTrigger, 0.9);
        }

        // 4. Multi-step planning detection (sequential words + multiple actions)
        let has_multi_step_indicators = Self::has_multi_step_keywords(&lower);
        let action_count = Self::count_distinct_actions(&lower);
        if has_multi_step_indicators || action_count >= 2 {
            let mut analysis = IntentAnalysis::new(UserIntent::MultiStepPlanning, 0.8);
            analysis.entities = Self::extract_entities(&lower);
            analysis.constraints = Self::extract_constraints(&lower);
            analysis.active_toolsets = Self::detect_toolsets(&lower);
            return analysis;
        }

        // 5. Single tool call detection
        let toolsets = Self::detect_toolsets(&lower);
        if !toolsets.is_empty() {
            let mut analysis = IntentAnalysis::new(UserIntent::SingleToolCall, 0.75);
            analysis.entities = Self::extract_entities(&lower);
            analysis.constraints = Self::extract_constraints(&lower);
            analysis.active_toolsets = toolsets;
            return analysis;
        }

        // Default: direct answer
        IntentAnalysis::new(UserIntent::DirectAnswer, 0.7)
    }

    // ── Internal helpers ──

    fn is_correction(lower: &str) -> bool {
        // 🆕 FIX: "不要" is often a constraint (e.g. "不要超过100USD"), not a cancellation.
        // Exclude cases where "不要" is followed by a numeric constraint pattern.
        let excluded_patterns = [
            "不要超过", "不要低于", "不要多于", "不要少于", "不要大于", "不要小于",
            "不要超过", "不要超出", "不要过", "不要低过", "不要高过",
        ];
        let has_excluded = excluded_patterns.iter().any(|p| lower.contains(p));
        if has_excluded {
            return false;
        }

        let correction_markers = ["不要", "别", "直接", "不用", "无需", "取消", "撤销", "别管"];
        correction_markers.iter().any(|m| lower.contains(m))
    }

    fn is_meta_question(lower: &str) -> bool {
        let meta_patterns = [
            "你会什么", "有哪些技能", "你能做什么", "你有什么功能",
            "what can you do", "what are your skills", "help",
            "show me your capabilities", "list skills",
        ];
        meta_patterns.iter().any(|p| lower.contains(p))
    }

    fn matches_workflow_name(lower: &str) -> bool {
        // Simplified: check for common workflow keywords
        let workflow_keywords = ["workflow", "流程", "自动化", "auto"];
        workflow_keywords.iter().any(|k| lower.contains(k))
            && (lower.contains("运行") || lower.contains("执行") || lower.contains("start") || lower.contains("run"))
    }

    fn has_multi_step_keywords(lower: &str) -> bool {
        let step_keywords = ["先", "再", "然后", "接着", "最后", "第一步", "第二步"];
        let then_keywords = ["first", "then", "next", "after", "finally", "step 1", "step 2"];
        let has_step = step_keywords.iter().any(|k| lower.contains(k));
        let has_then = then_keywords.iter().any(|k| lower.contains(k));
        (has_step && lower.chars().count() > 10) || (has_then && lower.len() > 20)
    }

    fn count_distinct_actions(lower: &str) -> usize {
        // Group synonymous actions so "下单购买" counts as ONE action, not two
        let action_groups: &[&[&str]] = &[
            // Group 1: query/search
            &["查", "查询", "搜索", "找", "看", "search", "find", "look"],
            // Group 2: trade/order/buy/sell (all synonyms)
            &["买", "卖", "下单", "交易", "买入", "卖出", "购买", "order", "buy", "sell", "place"],
            // Group 3: send/create/write
            &["发", "发送", "写", "创建", "send", "create", "write"],
            // Group 4: analyze/summarize/compare
            &["分析", "总结", "对比", "analyze", "compare", "summary", "summarize"],
        ];
        action_groups.iter().filter(|group| group.iter().any(|k| lower.contains(*k))).count()
    }

    fn detect_toolsets(lower: &str) -> Vec<String> {
        let mut active = Vec::new();
        for (name, keywords) in DEFAULT_TOOLSETS {
            if keywords.iter().any(|k| lower.contains(&k.to_lowercase())) {
                active.push(name.to_string());
            }
        }
        active
    }

    fn extract_entities(lower: &str) -> HashMap<String, String> {
        let mut entities = HashMap::new();

        // Symbol extraction (stock/crypto)
        let symbols = Self::extract_symbols(lower);
        if let Some(sym) = symbols.first() {
            entities.insert("symbol".to_string(), sym.clone());
        }

        // Side extraction (buy/sell)
        if lower.contains("买") || lower.contains("买入") || lower.contains("buy") || lower.contains("purchase") {
            entities.insert("side".to_string(), "buy".to_string());
        } else if lower.contains("卖") || lower.contains("卖出") || lower.contains("sell") {
            entities.insert("side".to_string(), "sell".to_string());
        }

        // Quantity extraction
        if let Some(qty) = Self::extract_quantity(lower) {
            entities.insert("qty".to_string(), qty);
        }

        entities
    }

    fn extract_symbols(lower: &str) -> Vec<String> {
        let known_symbols = [
            ("btc", "BTC/USD"),
            ("bitcoin", "BTC/USD"),
            ("eth", "ETH/USD"),
            ("ethereum", "ETH/USD"),
            ("aapl", "AAPL"),
            ("tsla", "TSLA"),
            ("goog", "GOOGL"),
            ("msft", "MSFT"),
        ];
        let mut results = Vec::new();
        for (keyword, symbol) in &known_symbols {
            if lower.contains(keyword) {
                results.push(symbol.to_string());
            }
        }
        results
    }

    fn extract_quantity(lower: &str) -> Option<String> {
        // Simple regex-like extraction: look for numbers followed by units
        let words: Vec<&str> = lower.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            if let Ok(_num) = word.parse::<f64>() {
                if i + 1 < words.len() {
                    let next = words[i + 1].to_lowercase();
                    if next.contains("股") || next.contains("份") || next.contains("个")
                        || next.contains("btc") || next.contains("eth")
                        || next.contains("share") || next.contains("unit")
                    {
                        return Some(format!("{} {}", word, words[i + 1]));
                    }
                }
                // Just a number
                return Some(word.to_string());
            }
        }
        None
    }

    fn extract_constraints(lower: &str) -> Vec<String> {
        let mut constraints = Vec::new();
        let constraint_markers = [
            ("不要先查询", "skip_query_first"),
            ("直接下单", "direct_order"),
            ("必须在今天完成", "deadline_today"),
            ("尽快", "urgent"),
            ("urgent", "urgent"),
            ("asap", "urgent"),
        ];
        for (marker, constraint) in &constraint_markers {
            if lower.contains(marker) {
                constraints.push(constraint.to_string());
            }
        }
        constraints
    }

    /// 🆕 OPTIMIZATION: Dual-track classification
    ///
    /// 1. Run heuristic classifier first (fast, no LLM call)
    /// 2. If confidence < threshold, construct LLM classification prompt
    ///    for the caller to execute
    pub fn classify_dual_track(query: &str, confidence_threshold: f32) -> (IntentAnalysis, Option<String>) {
        let heuristic = Self::classify_heuristic(query);
        if heuristic.confidence >= confidence_threshold {
            return (heuristic, None);
        }

        // Low confidence — provide LLM prompt for caller to execute
        let llm_prompt = Self::build_llm_classification_prompt(query);
        (heuristic, Some(llm_prompt))
    }

    /// Build LLM classification prompt for ambiguous intents
    pub fn build_llm_classification_prompt(query: &str) -> String {
        format!(
            "分析以下用户输入，输出 JSON（只输出 JSON，不要其他内容）：\n\
            {{\n\
              \"intent\": \"DirectAnswer|SingleToolCall|MultiStepPlanning|WorkflowTrigger|MetaQuestion|Correction\",\n\
              \"entities\": {{\"symbol\":\"...\",\"side\":\"...\"}},\n\
              \"constraints\": [\"不要先查询价格\"],\n\
              \"confidence\": 0.95\n\
            }}\n\
            \n\
            规则：\n\
            - 包含\"下单/购买/buy/sell/place order\" → SingleToolCall（除非有\"先...再...\"）\n\
            - 包含\"先...再...然后...\" → MultiStepPlanning\n\
            - 以\"/\"开头 → WorkflowTrigger\n\
            - \"你会什么/有哪些技能\" → MetaQuestion\n\
            - \"不要/别/直接\" → Correction\n\
            \n\
            用户输入：{}\n",
            query
        )
    }

    /// Parse LLM classification response into IntentAnalysis
    pub fn parse_llm_classification_response(response: &str) -> IntentAnalysis {
        // Try to extract JSON from response
        let json_str = if response.trim().starts_with('{') {
            response.trim()
        } else if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response.trim()
            }
        } else {
            response.trim()
        };

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(val) => {
                let intent_str = val.get("intent").and_then(|v| v.as_str()).unwrap_or("DirectAnswer");
                let intent = match intent_str {
                    "SingleToolCall" => UserIntent::SingleToolCall,
                    "MultiStepPlanning" => UserIntent::MultiStepPlanning,
                    "WorkflowTrigger" => UserIntent::WorkflowTrigger,
                    "MetaQuestion" => UserIntent::MetaQuestion,
                    "Correction" => UserIntent::Correction,
                    _ => UserIntent::DirectAnswer,
                };
                let confidence = val.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
                let mut analysis = IntentAnalysis::new(intent, confidence);

                if let Some(entities) = val.get("entities").and_then(|v| v.as_object()) {
                    for (k, v) in entities {
                        if let Some(s) = v.as_str() {
                            analysis.entities.insert(k.clone(), s.to_string());
                        }
                    }
                }
                if let Some(constraints) = val.get("constraints").and_then(|v| v.as_array()) {
                    analysis.constraints = constraints.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                }
                analysis.active_toolsets = Self::detect_toolsets(&response.to_lowercase());
                analysis
            }
            Err(_) => {
                // Fallback to heuristic on parse failure
                Self::classify_heuristic(response)
            }
        }
    }
}

impl Default for IntentEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Toolset definition for intent-driven tool filtering
#[derive(Debug, Clone)]
pub struct Toolset {
    pub name: String,
    pub description: String,
    pub tool_ids: Vec<String>,
    /// Keywords that trigger this toolset
    pub trigger_keywords: Vec<String>,
}

impl Toolset {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            tool_ids: Vec::new(),
            trigger_keywords: Vec::new(),
        }
    }

    pub fn with_tools(mut self, tool_ids: Vec<String>) -> Self {
        self.tool_ids = tool_ids;
        self
    }

    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.trigger_keywords = keywords;
        self
    }
}

/// Build default toolsets from the predefined keyword mapping
pub fn build_default_toolsets() -> Vec<Toolset> {
    DEFAULT_TOOLSETS
        .iter()
        .map(|(name, keywords)| {
            Toolset::new(
                name.to_string(),
                format!("{} related tools", name),
            )
            .with_keywords(keywords.iter().map(|k| k.to_string()).collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_answer() {
        let analysis = IntentEngine::classify_heuristic("Hello, how are you?");
        assert_eq!(analysis.intent, UserIntent::DirectAnswer);
    }

    #[test]
    fn test_meta_question() {
        let analysis = IntentEngine::classify_heuristic("你会什么");
        assert_eq!(analysis.intent, UserIntent::MetaQuestion);
    }

    #[test]
    fn test_multi_step_planning() {
        let analysis = IntentEngine::classify_heuristic("先查AAPL价格，然后买入10股");
        assert_eq!(analysis.intent, UserIntent::MultiStepPlanning);
        assert!(analysis.active_toolsets.contains(&"trading".to_string()));
        assert!(analysis.active_toolsets.contains(&"stock-data".to_string()));
    }

    #[test]
    fn test_single_tool_call() {
        let analysis = IntentEngine::classify_heuristic("查一下北京天气");
        assert_eq!(analysis.intent, UserIntent::SingleToolCall);
        assert!(analysis.active_toolsets.contains(&"weather".to_string()));
    }

    #[test]
    fn test_correction() {
        let analysis = IntentEngine::classify_heuristic("不要查询了，直接下单");
        assert_eq!(analysis.intent, UserIntent::Correction);
        assert!(analysis.constraints.contains(&"direct_order".to_string()));
    }

    #[test]
    fn test_correction_false_positive_budget_constraint() {
        // "不要" followed by a numeric constraint should NOT be a correction intent
        let analysis = IntentEngine::classify_heuristic("不要超过100USD");
        assert_ne!(analysis.intent, UserIntent::Correction);

        let analysis2 = IntentEngine::classify_heuristic("不要低于50");
        assert_ne!(analysis2.intent, UserIntent::Correction);

        let analysis3 = IntentEngine::classify_heuristic("不要大于200");
        assert_ne!(analysis3.intent, UserIntent::Correction);
    }

    #[test]
    fn test_entity_extraction() {
        let analysis = IntentEngine::classify_heuristic("买入 0.01 BTC");
        assert_eq!(analysis.entities.get("symbol"), Some(&"BTC/USD".to_string()));
        assert_eq!(analysis.entities.get("side"), Some(&"buy".to_string()));
    }
}
