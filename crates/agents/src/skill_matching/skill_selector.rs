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
            // 🆕 FIX: Increased to 25s — skill selection needs time for LLM ranking
            // of complex queries (e.g. weather, multi-step tasks).
            // Note: Kimi k2.6 can take 15-20s for 800-token JSON output at peak load.
            timeout: Duration::from_secs(25),
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
        let select_start = Instant::now();
        let query_preview = Self::truncate(query, 100);
        tracing::info!(
            "🔍 SkillSelector::select() START | query_summary='{}' | query_preview='{}'",
            query_summary, query_preview
        );

        // 1. Check cache first
        let cache_key = query_summary.to_string();
        {
            let cache = self.cache.read().await;
            if let Some((cached, timestamp)) = cache.get(&cache_key) {
                if timestamp.elapsed() < self.cache_ttl {
                    tracing::info!(
                        "✅ Skill selection cache hit for: {} (took {:?})",
                        &cache_key[..cache_key.len().min(50)],
                        select_start.elapsed()
                    );
                    return Ok(cached.clone());
                }
            }
        }

        // Step 1: Recall candidates
        let recall_start = Instant::now();
        let candidates = self.recall_candidates(query_summary).await?;
        tracing::info!(
            "📋 SkillSelector::recall_candidates() | count={} | names={:?} | took={:?}",
            candidates.len(),
            candidates.iter().map(|c| c.skill.name.as_str()).collect::<Vec<_>>(),
            recall_start.elapsed()
        );

        if candidates.is_empty() {
            tracing::warn!("⚠️ SkillSelector::select() — no candidates recalled, returning empty selection");
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
        let selection_start = Instant::now();
        let selection = self.make_selection(&ranked, llm_needs_planning).await?;
        tracing::info!(
            "🎯 SkillSelector::make_selection() | selected={:?} | confidence={:.2} | took={:?}",
            selection.selected_skill_name, selection.confidence, selection_start.elapsed()
        );

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

        tracing::info!(
            "🏁 SkillSelector::select() END | total={:?} | selected={:?} | needs_planning={}",
            select_start.elapsed(),
            selection.selected_skill_name,
            selection.needs_planning
        );
        Ok(selection)
    }

    /// Step 1: Recall candidates using registry search (embedding future)
    async fn recall_candidates(
        &self,
        query_summary: &str,
    ) -> Result<Vec<RegisteredSkill>, SkillSelectError> {
        let mut candidates = self.registry.search(query_summary).await;

        // 🆕 FIX: If search returns empty (e.g. English query_summary vs Chinese descriptions),
        // fallback to enabled skills sorted by popularity so ranking still has candidates.
        if candidates.is_empty() {
            candidates = self.registry.list_enabled().await;
        }

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
        let rank_start = Instant::now();
        let prompt = self.build_ranking_prompt(query, query_summary, candidates);
        let prompt_len = prompt.len();
        let prompt_tokens_est = prompt_len / 4; // rough estimate: 1 token ≈ 4 chars

        tracing::info!(
            "🤖 SkillSelector::rank_candidates() | prompt_len={} (~{} tokens) | candidates={} | timeout={}s",
            prompt_len, prompt_tokens_est, candidates.len(), self.timeout.as_secs()
        );
        tracing::debug!("📝 SkillSelector ranking prompt:\n{}", prompt);

        let messages = vec![Message::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            prompt,
        )];

        // 🆕 FIX: Limit max_tokens to 768 — ranking output for ~5 candidates is ~400-600 tokens.
        // This prevents Kimi k2.6 from generating excessive reasoning and reduces latency.
        let mut context = std::collections::HashMap::new();
        context.insert("max_tokens".to_string(), "768".to_string());
        // 🆕 FIX: Request a faster/cheaper model for this stateless ranking task if available.
        // Gateway can override this based on its own config; this is just a hint.
        context.insert("model".to_string(), "kimi-flash".to_string());

        let llm_start = Instant::now();
        let response = tokio::time::timeout(
            self.timeout,
            self.llm.call_llm(messages, Some(context)),
        )
        .await
        .map_err(|_| {
            tracing::error!(
                "⏱️ SkillSelector::rank_candidates() TIMEOUT after {:?} | prompt_len={} | candidates={}",
                self.timeout, prompt_len, candidates.len()
            );
            SkillSelectError::Timeout(self.timeout.as_secs())
        })?
        .map_err(|e| SkillSelectError::LLMError(e.to_string()))?;

        let llm_latency = llm_start.elapsed();
        let response_len = response.len();
        tracing::info!(
            "📥 SkillSelector::rank_candidates() | LLM latency={:?} | response_len={} | total_rank={:?}",
            llm_latency, response_len, rank_start.elapsed()
        );
        tracing::debug!("📄 SkillSelector ranking response:\n{}", response);

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
    /// 🆕 FIX: Truncate all fields to keep prompt concise and reduce LLM latency.
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

            // 🆕 FIX: Only take 1 example each to reduce prompt size
            let pos_examples = manifest
                .activation_examples
                .iter()
                .take(1)
                .cloned()
                .collect::<Vec<_>>();
            let pos_str = if pos_examples.is_empty() {
                "(none)".to_string()
            } else {
                pos_examples.join("
  - ")
            };

            let neg_examples = manifest
                .activation_negative_examples
                .iter()
                .take(1)
                .cloned()
                .collect::<Vec<_>>();
            let neg_str = if neg_examples.is_empty() {
                "(none)".to_string()
            } else {
                neg_examples.join("
  - ")
            };

            // 🆕 FIX: Truncate when_to_use and description to avoid prompt bloat
            let when_to_use = if manifest.when_to_use.is_empty() {
                manifest.description.clone()
            } else {
                manifest.when_to_use.clone()
            };
            let when_truncated = Self::truncate(&when_to_use, 200);
            let desc_truncated = Self::truncate(&manifest.description, 100);

            candidate_sections.push_str(&format!(
                "### [{index}] {name} (id: {id})
\
                 - When to use: {when}
\
                 - Description: {desc}
\
                 - Capabilities: {caps}
\
                 - Examples of CORRECT usage:
   - {pos}
\
                 - Examples of INCORRECT usage (do NOT match these):
   - {neg}

",
                index = i,
                name = manifest.name,
                id = skill.skill.id,
                when = when_truncated,
                desc = desc_truncated,
                caps = caps,
                pos = pos_str,
                neg = neg_str,
            ));
        }

        // 🆕 FIX: Truncate full query to avoid prompt bloat; query_summary is already concise.
        let query_truncated = Self::truncate(query, 300);

        format!(
            "You are a Skill Matching Judge. Evaluate which skill, if any, best matches the query.

\
            ## User Query (truncated)
{}

\
            ## Query Summary
{}

\
            ## Candidate Skills
{}\
            ## Evaluation Criteria (score 0-10 each)
\
            1. **Relevance**: Alignment with query
\
            2. **Specificity**: Is this the MOST specific skill?
\
            3. **Capability Match**: Does it have needed capabilities?
\
            4. **Negative Example Check**: If query resembles a negative example, overall <= 3.

\
            ## Rules
\
            - Select only if overall >= 7.0
\
            - If multiple >= 7.0, pick highest SPECIFICITY
\
            - If none >= 7.0, selected_skill must be null
\
            - NEVER select just because it's the \"closest\" match

\
            ## Output Format
\
            ```json
\
            {{
\
              \"selected_skill\": \"skill_id_or_null\",
\
              \"needs_planning\": true/false,
\
              \"confidence\": 0.0-1.0,
\
              \"scores\": [
\
                {{
\
                  \"skill_id\": \"...\",
\
                  \"relevance\": 0-10,
\
                  \"specificity\": 0-10,
\
                  \"capability_match\": 0-10,
\
                  \"overall_score\": 0-10,
\
                  \"reason\": \"...\"
\
                }}
\
              ],
\
              \"selection_reasoning\": \"Detailed explanation...\"
\
            }}
\
            ```",
            query_truncated, query_summary, candidate_sections
        )
    }

    /// 🆕 FIX: Helper to truncate strings with ellipsis
    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            let mut end = max_len;
            while end > 0 && !s.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &s[..end])
        }
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
