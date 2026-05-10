//! Skill Registry
//!
//! Central registry for skill discovery and management.
//!
//! Thread-safe with RwLock for concurrent access.

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::evolution::skill_lineage::SkillLineage;
use crate::skills::loader::LoadedSkill;

/// Skill registry
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, RegisteredSkill>>,
    categories: RwLock<HashMap<String, Vec<String>>>,
}

/// Semantic version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl serde::Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Version::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(version: &str) -> Result<Self, VersionError> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::InvalidFormat(version.to_string()));
        }

        let major = parts[0]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[0].to_string()))?;
        let minor = parts[1]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[1].to_string()))?;
        let patch = parts[2]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[2].to_string()))?;

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Version errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),
    #[error("Invalid version number: {0}")]
    InvalidNumber(String),
}

/// Skill definition for registry
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
}

/// Skill disclosure level for progressive loading
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDisclosureLevel {
    L0, // ~10 tokens — skill list index (name only)
    L1, // ~30 tokens — name + one-liner
    L2, // ~200 tokens — summary
    L3, // ~2000 tokens — full doc
}

/// Registered skill
#[derive(Debug, Clone)]
pub struct RegisteredSkill {
    pub skill: LoadedSkill,
    pub category: String,
    pub tags: Vec<String>,
    pub installed_at: u64,
    pub usage_count: u64,
    pub enabled: bool,
    /// 🆕 OPTIMIZATION: L1/L2/L3 progressive disclosure content
    pub l1_index: Option<String>,
    pub l2_summary: Option<String>,
    pub l3_full_doc: Option<String>,
    /// 🆕 PHASE 5: Skill lineage tracking
    pub lineage: Option<SkillLineage>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            categories: RwLock::new(HashMap::new()),
        }
    }

    /// Register a skill
    pub async fn register(
        &self,
        skill: LoadedSkill,
        category: impl Into<String>,
        tags: Vec<String>,
    ) {
        let skill_id = skill.id.clone();
        let category = category.into();

        let registered = RegisteredSkill {
            skill,
            category: category.clone(),
            tags,
            installed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs(),
            usage_count: 0,
            enabled: true,
            l1_index: None,
            l2_summary: None,
            l3_full_doc: None,
            lineage: Some(SkillLineage::new(&skill_id, "1.0.0")),
        };

        // Lock order: skills first, then categories to avoid deadlocks
        {
            let mut skills = self.skills.write().await;
            skills.insert(skill_id.clone(), registered);
        }

        {
            let mut categories = self.categories.write().await;
            categories
                .entry(category)
                .or_insert_with(Vec::new)
                .push(skill_id);
        }
    }

    /// Get skill by ID
    pub async fn get(&self, skill_id: &str) -> Option<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.get(skill_id).cloned()
    }

    /// Find skills by category
    pub async fn by_category(&self, category: &str) -> Vec<RegisteredSkill> {
        // Lock order: categories first, then skills (both read locks, so order is less
        // critical)
        let categories = self.categories.read().await;
        let skills = self.skills.read().await;

        categories
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| skills.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find skills by tag
    pub async fn by_tag(&self, tag: &str) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        let tag = tag.to_string();
        skills
            .values()
            .filter(|s| s.tags.contains(&tag))
            .cloned()
            .collect()
    }

    /// 🆕 OPTIMIZATION: Get skill description at specified disclosure level
    pub async fn get_skill_description(
        &self,
        skill_id: &str,
        level: SkillDisclosureLevel,
    ) -> Option<String> {
        let skills = self.skills.read().await;
        let skill = skills.get(skill_id)?;

        match level {
            SkillDisclosureLevel::L0 => Some(skill.skill.name.clone()),
            SkillDisclosureLevel::L1 => skill.l1_index.clone().or_else(|| {
                Some(format!(
                    "{}: {}",
                    skill.skill.name,
                    skill
                        .skill
                        .manifest
                        .description
                        .chars()
                        .take(50)
                        .collect::<String>()
                ))
            }),
            SkillDisclosureLevel::L2 => skill
                .l2_summary
                .clone()
                .or_else(|| Some(skill.skill.manifest.description.clone())),
            SkillDisclosureLevel::L3 => skill
                .l3_full_doc
                .clone()
                .or_else(|| Some(skill.skill.manifest.prompt_template.clone())),
        }
    }

    /// 🆕 OPTIMIZATION: Register skill with progressive disclosure levels
    pub async fn register_with_levels(
        &self,
        skill: LoadedSkill,
        category: impl Into<String>,
        tags: Vec<String>,
        l1_index: Option<String>,
        l2_summary: Option<String>,
        l3_full_doc: Option<String>,
    ) {
        let skill_id = skill.id.clone();
        let category = category.into();

        let registered = RegisteredSkill {
            skill,
            category: category.clone(),
            tags,
            installed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs(),
            usage_count: 0,
            enabled: true,
            l1_index,
            l2_summary,
            l3_full_doc,
            lineage: Some(SkillLineage::new(&skill_id, "1.0.0")),
        };

        {
            let mut skills = self.skills.write().await;
            skills.insert(skill_id.clone(), registered);
        }

        {
            let mut categories = self.categories.write().await;
            categories
                .entry(category)
                .or_insert_with(Vec::new)
                .push(skill_id);
        }
    }

    /// Search skills by name or description with semantic keyword overlap
    /// scoring. 🆕 FIX: Uses keyword overlap instead of simple substring
    /// match for better relevance.
    /// Search skills with relevance scores.
    /// Returns Vec of (score, skill) tuples sorted by score descending.
    pub async fn search_scored(&self, query: &str) -> Vec<(usize, RegisteredSkill)> {
        let skills = self.skills.read().await;
        let query_lower = query.to_lowercase();
        let query_words: std::collections::HashSet<String> = query_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 3)
            .map(|w| w.to_string())
            .collect();

        let mut scored: Vec<(usize, RegisteredSkill)> = skills
            .values()
            .filter_map(|s| {
                let name_lower = s.skill.name.to_lowercase();
                let desc_lower = s.skill.manifest.description.to_lowercase();
                let caps_lower = s.skill.manifest.capabilities.join(" ").to_lowercase();

                // Direct substring match gets highest priority
                if name_lower.contains(&query_lower) || desc_lower.contains(&query_lower) {
                    return Some((100, s.clone()));
                }

                // Keyword overlap scoring
                let text = format!("{} {} {}", name_lower, desc_lower, caps_lower);
                let text_words: std::collections::HashSet<String> = text
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() >= 3)
                    .map(|w| w.to_string())
                    .collect();

                let overlap = query_words.intersection(&text_words).count();
                if overlap > 0 {
                    Some((overlap, s.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored
    }

    pub async fn search(&self, query: &str) -> Vec<RegisteredSkill> {
        self.search_scored(query)
            .await
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    /// List all skills
    pub async fn list_all(&self) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.values().cloned().collect()
    }

    /// List only enabled skills
    pub async fn list_enabled(&self) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.values().filter(|s| s.enabled).cloned().collect()
    }

    /// Increment usage count
    pub async fn record_usage(&self, skill_id: &str) {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_id) {
            skill.usage_count += 1;
        }
    }

    /// Enable a skill
    pub async fn enable(&self, skill_id: &str) -> bool {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_id) {
            skill.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a skill
    pub async fn disable(&self, skill_id: &str) -> bool {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_id) {
            skill.enabled = false;
            true
        } else {
            false
        }
    }

    /// Unregister skill
    pub async fn unregister(&self, skill_id: &str) -> Option<RegisteredSkill> {
        // Lock order: skills first, then categories
        let mut skills = self.skills.write().await;
        let removed = skills.remove(skill_id);
        drop(skills);

        if removed.is_some() {
            let mut categories = self.categories.write().await;
            for ids in categories.values_mut() {
                ids.retain(|id| id != skill_id);
            }
            // Clean up empty categories
            categories.retain(|_, ids| !ids.is_empty());
        }

        removed
    }

    /// Get categories
    pub async fn categories(&self) -> Vec<String> {
        let categories = self.categories.read().await;
        categories.keys().cloned().collect()
    }

    /// 🆕 PHASE 5: Get skill lineage
    pub async fn get_lineage(&self, skill_id: &str) -> Option<SkillLineage> {
        let skills = self.skills.read().await;
        skills.get(skill_id).and_then(|s| s.lineage.clone())
    }

    /// 🆕 PHASE 5: Rollback skill to a specific version
    ///
    /// Creates a rollback lineage node to preserve immutable history,
    /// then updates current_version to the target.
    pub async fn rollback(&self, skill_id: &str, target_version: &str) -> Result<(), String> {
        use chrono::Utc;

        use crate::evolution::skill_lineage::{LineageNode, LineageSource};

        let mut skills = self.skills.write().await;
        let skill = skills.get_mut(skill_id).ok_or("Skill not found")?;
        let lineage = skill.lineage.as_mut().ok_or("No lineage for skill")?;

        if !lineage.has_version(target_version) {
            return Err(format!("Version {} not found in lineage", target_version));
        }

        let current_version = lineage.current_version.clone();
        let rollback_version = format!("{}-rollback-{}", current_version, uuid::Uuid::new_v4());

        lineage.add_version(LineageNode {
            version: rollback_version.clone(),
            parent_ids: vec![current_version.clone()],
            source: LineageSource::Rollback {
                target_version: target_version.to_string(),
                reason: format!("Rollback from {} to {}", current_version, target_version),
            },
            change_summary: format!("Rollback to version {}", target_version),
            created_at: Utc::now(),
            quality_score: 0.0,
            usage_stats: Default::default(),
            content_hash: String::new(),
        });

        lineage.current_version = target_version.to_string();
        Ok(())
    }

    /// 🆕 PHASE 5: Update skill lineage with a new version node
    pub async fn update_lineage(
        &self,
        skill_id: &str,
        lineage: SkillLineage,
    ) -> Result<(), String> {
        let mut skills = self.skills.write().await;
        let skill = skills.get_mut(skill_id).ok_or("Skill not found")?;
        skill.lineage = Some(lineage);
        Ok(())
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
