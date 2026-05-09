//! LLM Intent Analyzer
//!
//! Pure LLM-driven intent understanding with zero hardcoded keyword rules.
//! All semantic decisions (direct answer, skill need, planning need) are made by LLM.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::communication::{LLMCallInterface, Message, PlatformType};
use crate::intent::UserIntent;

/// Planning strategy hint determined by LLM
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningStrategyHint {
    SingleShot,
    ReAct,
    Decompose,
    MultiSkill,
}

/// 🆕 V2: Intent analysis result — all fields produced by LLM, zero hardcoded rules
#[derive(Debug, Clone)]
pub struct IntentAnalysisV2 {
    /// Whether this is a direct conversational response (greeting, chit-chat, simple Q&A)
    pub direct_answer: bool,
    /// Whether a specialized skill is needed
    pub needs_skill: bool,
    /// Whether multi-step planning is required
    pub needs_planning: bool,
    /// Hint for planning strategy (if needs_planning)
    pub planning_strategy_hint: Option<PlanningStrategyHint>,
    /// Legacy intent classification (for backward compatibility)
    pub intent: UserIntent,
    /// Extracted entities
    pub entities: HashMap<String, String>,
    /// User constraints
    pub constraints: Vec<String>,
    /// Normalized query summary for retrieval (embedding-friendly)
    pub query_summary: String,
    /// Confidence 0.0-1.0
    pub confidence: f32,
    /// Detected toolsets (empty — no hardcoded toolsets in V2)
    pub active_toolsets: Vec<String>,
}

impl IntentAnalysisV2 {
    pub fn new(direct_answer: bool, confidence: f32) -> Self {
        Self {
            direct_answer,
            needs_skill: !direct_answer,
            needs_planning: false,
            planning_strategy_hint: None,
            intent: if direct_answer {
                UserIntent::DirectAnswer
            } else {
                UserIntent::SingleToolCall
            },
            entities: HashMap::new(),
            constraints: Vec::new(),
            query_summary: String::new(),
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
}

/// LLM-driven intent analyzer — zero hardcoded rules
pub struct LLMIntentAnalyzer {
    llm: Arc<dyn LLMCallInterface>,
    timeout: Duration,
    /// 🆕 FIX: Cache for intent analysis results (TTL 5 minutes)
    cache: RwLock<HashMap<String, (IntentAnalysisV2, Instant)>>,
    cache_ttl: Duration,
}

impl LLMIntentAnalyzer {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self {
            llm,
            // 🆕 FIX: Reduced to 5s — intent analysis is a lightweight JSON generation task.
            // If LLM cannot respond within 5s, the system degrades gracefully to legacy path.
            timeout: Duration::from_secs(5),
            cache: RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Set custom timeout for LLM calls (default: 30s)
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Analyze user query using LLM. No keyword rules, no regex, no hardcoded mappings.
    /// 🆕 FIX: Results are cached for 5 minutes to avoid repeated LLM calls for identical queries.
    pub async fn analyze(
        &self,
        user_message: &str,
        history: Option<Vec<(String, String)>>, // (role, content) pairs
    ) -> Result<IntentAnalysisV2, IntentAnalyzeError> {
        // 1. Check cache first
        let cache_key = user_message.to_string();
        {
            let cache = self.cache.read().await;
            if let Some((cached, timestamp)) = cache.get(&cache_key) {
                if timestamp.elapsed() < self.cache_ttl {
                    tracing::debug!("Intent analysis cache hit for: {}", &cache_key[..cache_key.len().min(50)]);
                    return Ok(cached.clone());
                }
            }
        }

        let (system_prompt, user_prompt) = self.build_analysis_prompts(user_message, history);

        // 🆕 FIX: Send system + user as separate messages so gateway correctly identifies roles.
        let messages = vec![
            Message::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                system_prompt,
            ),
            Message::new(
                uuid::Uuid::new_v4(),
                PlatformType::WebChat,
                format!("用户: {}", user_prompt),
            ),
        ];

        // 🆕 FIX: Limit max_tokens to 512 — intent analysis only needs a small JSON output.
        let mut context = std::collections::HashMap::new();
        context.insert("max_tokens".to_string(), "512".to_string());

        let response = tokio::time::timeout(
            self.timeout,
            self.llm.call_llm(messages, Some(context)),
        )
        .await
        .map_err(|_| IntentAnalyzeError::Timeout(self.timeout.as_secs()))?
        .map_err(|e| IntentAnalyzeError::LLMError(e.to_string()))?;

        let result = self.parse_analysis_response(&response, user_message)?;

        // 2. Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, (result.clone(), Instant::now()));
            // Prune old entries if cache grows too large (>100)
            if cache.len() > 100 {
                let now = Instant::now();
                let keys_to_remove: Vec<String> = cache
                    .iter()
                    .filter(|(_, (_, ts))| now.duration_since(*ts) > self.cache_ttl)
                    .map(|(k, _)| k.clone())
                    .collect();
                for k in keys_to_remove {
                    cache.remove(&k);
                }
            }
        }

        Ok(result)
    }

    /// Build system + user prompts separately for correct role separation
    fn build_analysis_prompts(
        &self,
        user_message: &str,
        history: Option<Vec<(String, String)>>,
    ) -> (String, String) {
        let history_text = history
            .map(|h| {
                h.iter()
                    .map(|(role, content)| format!("{}: {}", role, content))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "(no history)".to_string());

        let system_prompt = "You are an Intent Analysis Engine. Analyze the user message and output \
            a structured JSON response. Do not output any text outside the JSON.\n\n\
            ## Instructions\n\
            1. Determine if the user is asking for a direct conversational response \
               (greeting, chit-chat, simple factual question, opinion) OR if they \
               need a specialized skill/tool.\n\
            2. If a skill is needed, extract any entities mentioned (locations, dates, \
               amounts, symbols, etc.).\n\
            3. Identify any constraints (budget limits, time restrictions, preferences).\n\
            4. Determine if the task requires multi-step planning.\n\
            5. Generate a concise summary of the query for retrieval purposes.\n\n\
            ## Output Format\n\
            ```json\n\
            {\n\
              \"direct_answer\": true/false,\n\
              \"needs_skill\": true/false,\n\
              \"needs_planning\": true/false,\n\
              \"planning_strategy_hint\": \"single_shot|react|decompose|multi_skill|null\",\n\
              \"intent\": \"DirectAnswer|SingleToolCall|MultiStepPlanning|WorkflowTrigger|MetaQuestion|Correction\",\n\
              \"entities\": {\"key\": \"value\"},\n\
              \"constraints\": [\"...\"],\n\
              \"query_summary\": \"concise summary for embedding search\",\n\
              \"confidence\": 0.0-1.0\n\
            }\n\
            ```\n\n\
            ## Rules\n\
            - \"direct_answer\": true ONLY for greetings, small talk, simple Q&A, \
              meta-questions about capabilities, or when no specialized skill applies.\n\
            - \"needs_planning\": true when the task has multiple steps, dependencies, \
              or requires sequential tool use.\n\
            - \"query_summary\": Should be a normalized, search-friendly description \
              (e.g., \"travel plan Beijing 5 days budget 5000\").\n\
            - Set \"confidence\" based on how clear the user intent is.\n\
            - NEVER use keyword matching or pattern rules — analyze the SEMANTIC intent."
            .to_string();

        let user_prompt = format!(
            "## User Message\n{}\n\n\
            ## Conversation History (last 3 turns)\n{}",
            user_message, history_text
        );

        (system_prompt, user_prompt)
    }

    /// Parse LLM response into structured IntentAnalysisV2
    fn parse_analysis_response(
        &self,
        response: &str,
        original_query: &str,
    ) -> Result<IntentAnalysisV2, IntentAnalyzeError> {
        // Extract JSON from response
        let json_str = Self::extract_json(response)?;

        let val: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| IntentAnalyzeError::ParseError(e.to_string()))?;

        let direct_answer = val
            .get("direct_answer")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let needs_skill = val
            .get("needs_skill")
            .and_then(|v| v.as_bool())
            .unwrap_or(!direct_answer);

        let needs_planning = val
            .get("needs_planning")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let planning_strategy_hint = val
            .get("planning_strategy_hint")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "react" => PlanningStrategyHint::ReAct,
                "decompose" => PlanningStrategyHint::Decompose,
                "multi_skill" => PlanningStrategyHint::MultiSkill,
                _ => PlanningStrategyHint::SingleShot,
            });

        let intent_str = val
            .get("intent")
            .and_then(|v| v.as_str())
            .unwrap_or("DirectAnswer");

        let intent = match intent_str {
            "SingleToolCall" => UserIntent::SingleToolCall,
            "MultiStepPlanning" => UserIntent::MultiStepPlanning,
            "WorkflowTrigger" => UserIntent::WorkflowTrigger,
            "MetaQuestion" => UserIntent::MetaQuestion,
            "Correction" => UserIntent::Correction,
            _ => UserIntent::DirectAnswer,
        };

        let entities = val
            .get("entities")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let constraints = val
            .get("constraints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let query_summary = val
            .get("query_summary")
            .and_then(|v| v.as_str())
            .unwrap_or(original_query)
            .to_string();

        let confidence = val
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;

        Ok(IntentAnalysisV2 {
            direct_answer,
            needs_skill,
            needs_planning,
            planning_strategy_hint,
            intent,
            entities,
            constraints,
            query_summary,
            confidence,
            active_toolsets: Vec::new(), // V2: no hardcoded toolsets
        })
    }

    fn extract_json(response: &str) -> Result<&str, IntentAnalyzeError> {
        let trimmed = response.trim();

        // Try JSON code block
        if let Some(start) = trimmed.find("```json") {
            let after_tag = &trimmed[start + 7..];
            if let Some(end) = after_tag.find("```") {
                return Ok(after_tag[..end].trim());
            }
        }

        // Try raw JSON
        if trimmed.starts_with('{') {
            return Ok(trimmed);
        }

        // Find balanced braces using brace counting (handles nested JSON)
        if let Some(start) = trimmed.find('{') {
            let mut depth = 0;
            for (i, ch) in trimmed[start..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        let end = start + i;
                        return Ok(&trimmed[start..=end]);
                    }
                }
            }
        }

        Err(IntentAnalyzeError::ParseError(
            "No JSON found in LLM response".to_string(),
        ))
    }
}

/// Intent analysis errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum IntentAnalyzeError {
    #[error("LLM call failed: {0}")]
    LLMError(String),
    #[error("Failed to parse LLM response: {0}")]
    ParseError(String),
    #[error("Intent analysis timed out after {0}s")]
    Timeout(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_raw() {
        let response = r#"{"direct_answer": true, "confidence": 0.95}"#;
        let result = LLMIntentAnalyzer::extract_json(response).unwrap();
        assert_eq!(result, response);
    }

    #[test]
    fn test_extract_json_block() {
        let response = "Some text\n```json\n{\"direct_answer\": true}\n```\nMore text";
        let result = LLMIntentAnalyzer::extract_json(response).unwrap();
        assert_eq!(result, r#"{"direct_answer": true}"#);
    }

    #[test]
    fn test_parse_analysis_response() {
        // We can't test the full analyze() without a mock LLM, but we can test parsing
        let analyzer = LLMIntentAnalyzer::new(Arc::new(MockLLM));
        let response = r#"{"direct_answer": false, "needs_skill": true, "needs_planning": true, "planning_strategy_hint": "react", "intent": "MultiStepPlanning", "entities": {"city": "Beijing"}, "constraints": ["budget < 5000"], "query_summary": "travel plan Beijing 5 days budget 5000", "confidence": 0.92}"#;

        let analysis = analyzer
            .parse_analysis_response(response, "original query")
            .unwrap();

        assert!(!analysis.direct_answer);
        assert!(analysis.needs_skill);
        assert!(analysis.needs_planning);
        assert_eq!(analysis.planning_strategy_hint, Some(PlanningStrategyHint::ReAct));
        assert_eq!(analysis.entities.get("city"), Some(&"Beijing".to_string()));
        assert_eq!(analysis.query_summary, "travel plan Beijing 5 days budget 5000");
        assert!((analysis.confidence - 0.92).abs() < 0.01);
    }

    struct MockLLM;

    #[async_trait::async_trait]
    impl LLMCallInterface for MockLLM {
        async fn call_llm(
            &self,
            _messages: Vec<Message>,
            _context: Option<HashMap<String, String>>,
        ) -> Result<String, crate::error::AgentError> {
            Ok(String::new())
        }

        async fn call_llm_stream(
            &self,
            _messages: Vec<Message>,
            _context: Option<HashMap<String, String>>,
        ) -> Result<tokio::sync::mpsc::Receiver<String>, crate::error::AgentError> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.send(String::new()).await;
            Ok(rx)
        }
    }
}
