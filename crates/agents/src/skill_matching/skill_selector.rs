//! Skill Selector (V2)
//!
//! Pure LLM-driven skill selection with zero hardcoded rules.
//! Architecture: Recall (Top-K) → LLM Ranking (0-10 scores) → Selection or Rejection

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::communication::{LLMCallInterface, Message, PlatformType};
use crate::skills::registry::{RegisteredSkill, SkillRegistry};

/// Score for a single skill candidate
#[derive(Debug, Clone)]
pub struct SkillScore {
    pub skill_id: String,
    pub skill_name: String,
    pub relevance: f32,       // 0-10: how well skill aligns with query
    pub specificity: f32,     // 0-10: is this the MOST specific skill?
    pub capability_match: f32, // 0-10: does skill have needed capabilities?
    pub overall_score: f32,   // 0-10: composite
    pub reason: String,
}

/// Result of skill selection
#[derive(Debug, Clone)]
pub struct SkillSelection {
    pub selected_skill: Option<String>,
    pub selected_skill_name: Option<String>,
    pub needs_planning: bool,
    pub confidence: f32,
    pub scores: Vec<SkillScore>,
    pub selection_reasoning: String,
    /// 🆕 Whether to load L3 full content or just L2 summary
    pub disclosure_level: crate::skills::registry::SkillDisclosureLevel,
}

impl SkillSelection {
    /// Threshold for selecting a skill
    pub const SELECTION_THRESHOLD: f32 = 7.0;

    pub fn is_rejected(&self) -> bool {
        self.selected_skill.is_none()
    }
}

/// Skill selector — pure LLM-driven, zero hardcoded rules
pub struct SkillSelector {
    llm: Arc<dyn LLMCallInterface>,
    registry: Arc<SkillRegistry>,
    /// Max candidate skills to send to LLM for ranking
    max_candidates: usize,
    /// Timeout for LLM ranking call
    timeout: Duration,
    /// 🆕 FIX: Cache for skill selection results (TTL 5 minutes)
    cache: RwLock<HashMap<String, (SkillSelection, Instant)>>,
    cache_ttl: Duration,
}

impl SkillSelector {
    pub fn new(
        llm: Arc<dyn LLMCallInterface>,
        registry: Arc<SkillRegistry>,
    ) -> Self {
        Self {
            llm,
            registry,
            // 🆕 FIX: Reduced from 8 to 5 candidates — fewer candidates = faster LLM ranking.
            max_candidates: 5,
            // 🆕 FIX: Reduced to 5s — skill selection is a lightweight JSON generation task.
            timeout: Duration::from_secs(5),
            cache: RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    pub fn with_max_candidates(mut self, max: usize) -> Self {
        self.max_candidates = max;
        self
    }

    /// Set custom timeout for LLM ranking calls (default: 30s)
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Full selection pipeline: Recall → Rank → Select
    /// 🆕 FIX: Results are cached for 5 minutes to avoid repeated LLM calls.
    pub async fn select(
        &self,
        query: &str,
        query_summary: &str,
    ) -> Result<SkillSelection, SkillSelectError> {
        // 1. Check cache first
        let cache_key = query_summary.to_string();
        {
            let cache = self.cache.read().await;
            if let Some((cached, timestamp)) = cache.get(&cache_key) {
                if timestamp.elapsed() < self.cache_ttl {
                    tracing::debug!("Skill selection cache hit for: {}", &cache_key[..cache_key.len().min(50)]);
                    return Ok(cached.clone());
                }
            }
        }

        // Step 1: Recall candidates
        let candidates = self.recall_candidates(query_summary).await?;

        if candidates.is_empty() {
            return Ok(SkillSelection {
                selected_skill: None,
                selected_skill_name: None,
                needs_planning: false,
                confidence: 0.0,
                scores: Vec::new(),
                selection_reasoning: "No skills available in registry".to_string(),
                disclosure_level: crate::skills::registry::SkillDisclosureLevel::L0,
            });
        }

        // Step 2: LLM Ranking
        let (ranked, llm_needs_planning) = self.rank_candidates(query, query_summary, &candidates).await?;

        // Step 3: Selection
        let selection = self.make_selection(&ranked, llm_needs_planning).await?;

        // 2. Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, (selection.clone(), Instant::now()));
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

        Ok(selection)
    }

    /// Step 1: Recall candidates using registry search (embedding future)
    async fn recall_candidates(
        &self,
        query_summary: &str,
    ) -> Result<Vec<RegisteredSkill>, SkillSelectError> {
        let mut candidates = self.registry.search(query_summary).await;

        // Sort by usage count (popularity) as secondary sort
        candidates.sort_by(|a, b| {
            let usage_cmp = b.usage_count.cmp(&a.usage_count);
            if usage_cmp != std::cmp::Ordering::Equal {
                return usage_cmp;
            }
            a.skill.name.cmp(&b.skill.name)
        });

        // Limit to max_candidates
        candidates.truncate(self.max_candidates);

        Ok(candidates)
    }

    /// Step 2: LLM Ranking — let LLM score each candidate on multiple dimensions
    async fn rank_candidates(
        &self,
        query: &str,
        query_summary: &str,
        candidates: &[RegisteredSkill],
    ) -> Result<(Vec<SkillScore>, bool), SkillSelectError> {
        let prompt = self.build_ranking_prompt(query, query_summary, candidates);

        let messages = vec![Message::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            prompt,
        )];

        // 🆕 FIX: Limit max_tokens to 1024 — ranking output for ~8 candidates is ~400-800 tokens.
        // This prevents Kimi k2.6 thinking mode from generating excessive reasoning tokens.
        let mut context = std::collections::HashMap::new();
        context.insert("max_tokens".to_string(), "1024".to_string());

        let response = tokio::time::timeout(
            self.timeout,
            self.llm.call_llm(messages, Some(context)),
        )
        .await
        .map_err(|_| SkillSelectError::Timeout(self.timeout.as_secs()))?
        .map_err(|e| SkillSelectError::LLMError(e.to_string()))?;

        self.parse_ranking_response(&response, candidates)
    }

    /// Step 3: Make final selection based on scores
    async fn make_selection(
        &self,
        scores: &[SkillScore],
        llm_needs_planning: bool,
    ) -> Result<SkillSelection, SkillSelectError> {
        // Find highest scoring skill
        let best = scores.iter().max_by(|a, b| {
            a.overall_score
                .partial_cmp(&b.overall_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let (selected, selected_name, reasoning, disclosure) = match best {
            Some(best_score) if best_score.overall_score >= SkillSelection::SELECTION_THRESHOLD => {
                let skill = self
                    .registry
                    .get(&best_score.skill_id)
                    .await
                    .ok_or_else(|| SkillSelectError::SkillNotFound(best_score.skill_id.clone()))?;

                let reason = format!(
                    "Selected '{}' with overall score {:.1}/10. {}",
                    best_score.skill_name, best_score.overall_score, best_score.reason
                );

                // Determine disclosure level based on score confidence
                let disclosure = if best_score.overall_score >= 9.0 {
                    crate::skills::registry::SkillDisclosureLevel::L2
                } else {
                    crate::skills::registry::SkillDisclosureLevel::L3
                };

                (
                    Some(best_score.skill_id.clone()),
                    Some(skill.skill.name.clone()),
                    reason,
                    disclosure,
                )
            }
            Some(best_score) => {
                let reason = format!(
                    "No skill met the threshold ({}). Best candidate '{}' scored {:.1}/10. \
                     Falling back to direct answer.",
                    SkillSelection::SELECTION_THRESHOLD,
                    best_score.skill_name,
                    best_score.overall_score
                );
                (None, None, reason, crate::skills::registry::SkillDisclosureLevel::L0)
            }
            None => (
                None,
                None,
                "No candidate skills to evaluate".to_string(),
                crate::skills::registry::SkillDisclosureLevel::L0,
            ),
        };

        // Trust LLM's judgment on planning need — zero hardcoded keyword rules
        let needs_planning = llm_needs_planning;

        let confidence = best.map(|s| s.overall_score / 10.0).unwrap_or(0.0);

        Ok(SkillSelection {
            selected_skill: selected,
            selected_skill_name: selected_name,
            needs_planning,
            confidence,
            scores: scores.to_vec(),
            selection_reasoning: reasoning,
            disclosure_level: disclosure,
        })
    }

    /// Build the ranking prompt — pure semantic evaluation, zero keyword rules
    fn build_ranking_prompt(
        &self,
        query: &str,
        query_summary: &str,
        candidates: &[RegisteredSkill],
    ) -> String {
        let mut candidate_sections = String::new();

        for (i, skill) in candidates.iter().enumerate() {
            let manifest = &skill.skill.manifest;

            // L1 disclosure: name, when_to_use, description (first sentence), capabilities (top 3)
            let caps = manifest
                .capabilities
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");

            let pos_examples = manifest
                .activation_examples
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>();
            let pos_str = if pos_examples.is_empty() {
                "(none provided)".to_string()
            } else {
                pos_examples.join("\n  - ")
            };

            let neg_examples = manifest
                .activation_negative_examples
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>();
            let neg_str = if neg_examples.is_empty() {
                "(none provided)".to_string()
            } else {
                neg_examples.join("\n  - ")
            };

            candidate_sections.push_str(&format!(
                "### [{index}] {name} (id: {id})\n\
                 - When to use: {when}\n\
                 - Description: {desc}\n\
                 - Capabilities: {caps}\n\
                 - Examples of CORRECT usage:\n   - {pos}\n\
                 - Examples of INCORRECT usage (do NOT match these):\n   - {neg}\n\n",
                index = i,
                name = manifest.name,
                id = skill.skill.id,
                when = if manifest.when_to_use.is_empty() {
                    manifest.description.clone()
                } else {
                    manifest.when_to_use.clone()
                },
                desc = manifest.description.chars().take(150).collect::<String>(),
                caps = caps,
                pos = pos_str,
                neg = neg_str,
            ));
        }

        format!(
            "You are a Skill Matching Judge. Your task is to evaluate which skill, \
            if any, best matches the user's query.\n\n\
            ## User Query\n{}\n\n\
            ## Query Summary\n{}\n\n\
            ## Candidate Skills\n{}\n\
            ## Evaluation Criteria (score 0-10 each)\n\
            1. **Relevance**: How well does the skill's purpose align with the query?\n\
            2. **Specificity**: Is this the MOST specific skill for the task, or is it too general?\n\
            3. **Capability Match**: Does the skill actually have the capabilities needed?\n\
            4. **Negative Example Check**: If the query resembles a negative example, \
               the overall score must be <= 3.\n\n\
            ## Rules\n\
            - A skill is \"selected\" only if its overall score >= 7.0\n\
            - If multiple skills score >= 7.0, select the one with highest SPECIFICITY\n\
            - If NO skill scores >= 7.0, selected_skill must be null\n\
            - NEVER select a skill just because it's the \"closest\" match — \
              if nothing truly fits, reject all\n\
            - Consider negative examples as strong signals of non-match\n\n\
            ## Output Format\n\
            ```json\n\
            {{\n\
              \"selected_skill\": \"skill_id_or_null\",\n\
              \"needs_planning\": true/false,\n\
              \"confidence\": 0.0-1.0,\n\
              \"scores\": [\n\
                {{\n\
                  \"skill_id\": \"...\",\n\
                  \"relevance\": 0-10,\n\
                  \"specificity\": 0-10,\n\
                  \"capability_match\": 0-10,\n\
                  \"overall_score\": 0-10,\n\
                  \"reason\": \"...\"\n\
                }}\n\
              ],\n\
              \"selection_reasoning\": \"Detailed explanation...\"\n\
            }}\n\
            ```",
            query, query_summary, candidate_sections
        )
    }

    fn parse_ranking_response(
        &self,
        response: &str,
        candidates: &[RegisteredSkill],
    ) -> Result<(Vec<SkillScore>, bool), SkillSelectError> {
        let json_str = Self::extract_json(response)?;

        let val: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| SkillSelectError::ParseError(e.to_string()))?;

        let scores_array = val
            .get("scores")
            .and_then(|v| v.as_array())
            .ok_or_else(|| SkillSelectError::ParseError("Missing scores array".to_string()))?;

        // Parse LLM's needs_planning judgment (trust LLM, zero hardcoded rules)
        let llm_needs_planning = val
            .get("needs_planning")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut scores = Vec::new();

        for score_val in scores_array {
            let skill_id = score_val
                .get("skill_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Find skill name from candidates
            let skill_name = candidates
                .iter()
                .find(|s| s.skill.id == skill_id)
                .map(|s| s.skill.name.clone())
                .unwrap_or_else(|| skill_id.clone());

            let relevance = score_val
                .get("relevance")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            let specificity = score_val
                .get("specificity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            let capability_match = score_val
                .get("capability_match")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            let overall_score = score_val
                .get("overall_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            let reason = score_val
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            scores.push(SkillScore {
                skill_id,
                skill_name,
                relevance,
                specificity,
                capability_match,
                overall_score,
                reason,
            });
        }

        // If LLM didn't score all candidates, add zero scores for missing ones
        for candidate in candidates {
            if !scores.iter().any(|s| s.skill_id == candidate.skill.id) {
                scores.push(SkillScore {
                    skill_id: candidate.skill.id.clone(),
                    skill_name: candidate.skill.name.clone(),
                    relevance: 0.0,
                    specificity: 0.0,
                    capability_match: 0.0,
                    overall_score: 0.0,
                    reason: "Not scored by LLM".to_string(),
                });
            }
        }

        Ok((scores, llm_needs_planning))
    }

    fn extract_json(response: &str) -> Result<&str, SkillSelectError> {
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

        Err(SkillSelectError::ParseError(
            "No JSON found in LLM response".to_string(),
        ))
    }
}

/// Skill selection errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillSelectError {
    #[error("LLM call failed: {0}")]
    LLMError(String),
    #[error("Failed to parse LLM response: {0}")]
    ParseError(String),
    #[error("Skill not found: {0}")]
    SkillNotFound(String),
    #[error("Skill selection timed out after {0}s")]
    Timeout(u64),
}
