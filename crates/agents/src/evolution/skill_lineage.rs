//! Skill Lineage — Version History & Rollback Tracking
//!
//! Every skill carries a genealogy of changes, enabling:
//! - Traceability: "Why does this skill work this way?"
//! - Rollback: revert to any previous version
//! - Attribution: credit/blame per change source

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
// use uuid::Uuid; // not currently needed

/// Source of a lineage node — who/what created this version
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LineageSource {
    /// Auto-distilled from a task trail
    AutoDistilled {
        task_id: String,
        trail_id: String,
    },
    /// User manually edited the skill
    ManualEdit {
        user_id: String,
        reason: String,
    },
    /// CAPO algorithm optimized the prompt/skill
    CapoOptimized {
        generation: u32,
        improvement: f32,
    },
    /// Patch fix for a specific issue
    PatchFix {
        issue_id: String,
        fix_description: String,
    },
    /// Rollback to a previous version
    Rollback {
        target_version: String,
        reason: String,
    },
}

/// A single node in the skill's lineage tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    /// Semantic version string (e.g. "1.2.3")
    pub version: String,
    /// Parent version IDs (empty = root)
    pub parent_ids: Vec<String>,
    /// How this version was created
    pub source: LineageSource,
    /// Human-readable change summary
    pub change_summary: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Quality score at this version (0-10)
    pub quality_score: f32,
    /// Usage statistics snapshot
    pub usage_stats: UsageStats,
    /// Full content hash (for integrity verification)
    pub content_hash: String,
}

/// Usage statistics snapshot at a point in time
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Total executions
    pub execution_count: u64,
    /// Successful executions
    pub success_count: u64,
    /// Failed executions
    pub failure_count: u64,
    /// Average execution time (ms)
    pub avg_execution_time_ms: u64,
    /// Last used timestamp
    pub last_used: Option<DateTime<Utc>>,
}

impl UsageStats {
    pub fn success_rate(&self) -> f32 {
        if self.execution_count == 0 {
            return 0.0;
        }
        self.success_count as f32 / self.execution_count as f32
    }
}

/// Complete lineage tree for a single skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLineage {
    pub skill_id: String,
    /// All nodes indexed by version string
    pub nodes: HashMap<String, LineageNode>,
    /// Current active version
    pub current_version: String,
    /// Root version (initial creation)
    pub root_version: String,
}

impl SkillLineage {
    pub fn new(skill_id: impl Into<String>, initial_version: impl Into<String>) -> Self {
        let skill_id = skill_id.into();
        let version = initial_version.into();
        let mut nodes = HashMap::new();

        let root = LineageNode {
            version: version.clone(),
            parent_ids: vec![],
            source: LineageSource::AutoDistilled {
                task_id: "initial".to_string(),
                trail_id: "initial".to_string(),
            },
            change_summary: "Initial skill creation".to_string(),
            created_at: Utc::now(),
            quality_score: 5.0,
            usage_stats: UsageStats::default(),
            content_hash: String::new(),
        };

        nodes.insert(version.clone(), root);

        Self {
            skill_id,
            nodes,
            current_version: version.clone(),
            root_version: version,
        }
    }

    /// Add a new version to the lineage
    pub fn add_version(&mut self, node: LineageNode) {
        self.current_version = node.version.clone();
        self.nodes.insert(node.version.clone(), node);
    }

    /// Get a specific version
    pub fn get_version(&self, version: &str) -> Option<&LineageNode> {
        self.nodes.get(version)
    }

    /// Get current version node
    pub fn current(&self) -> Option<&LineageNode> {
        self.nodes.get(&self.current_version)
    }

    /// Check if a version exists
    pub fn has_version(&self, version: &str) -> bool {
        self.nodes.contains_key(version)
    }

    /// Get all ancestors of a version (path from root to this version)
    pub fn ancestors(&self, version: &str) -> Vec<&LineageNode> {
        let mut path = Vec::new();
        let mut current = version;

        while let Some(node) = self.nodes.get(current) {
            path.push(node);
            if node.parent_ids.is_empty() {
                break;
            }
            // Follow first parent (mainline)
            current = &node.parent_ids[0];
        }

        path.reverse();
        path
    }

    /// List all versions in chronological order
    pub fn versions_chronological(&self) -> Vec<&LineageNode> {
        let mut versions: Vec<&LineageNode> = self.nodes.values().collect();
        versions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        versions
    }

    /// Generate a lineage tree visualization (Markdown)
    pub fn to_markdown(&self) -> String {
        let mut md = format!("# Skill Lineage: {}\n\n", self.skill_id);
        md.push_str(&format!("**Current**: `{}`\n\n", self.current_version));

        for node in self.versions_chronological() {
            md.push_str(&format!(
                "## `{}` (quality: {:.1})\n- **Source**: {:?}\n- **Summary**: {}\n- **Date**: {}\n\n",
                node.version,
                node.quality_score,
                node.source,
                node.change_summary,
                node.created_at.format("%Y-%m-%d %H:%M")
            ));
        }

        md
    }
}

/// Skill lifecycle manager — handles promotion, archival, and health monitoring
#[derive(Debug, Clone)]
pub struct SkillLifecycleManager;

/// Health status of a skill
#[derive(Debug, Clone)]
pub struct SkillHealth {
    pub skill_id: String,
    pub current_version: String,
    pub quality_score: f32,
    pub success_rate: f32,
    pub execution_count: u64,
    pub days_since_last_use: i64,
    pub recommendation: LifecycleAction,
}

/// Recommended action for a skill
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleAction {
    Keep,
    Promote { reason: String },
    NeedsRepair { reason: String },
    Archive { reason: String },
}

impl SkillLifecycleManager {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate skill health based on lineage and usage
    pub fn evaluate(&self, lineage: &SkillLineage) -> SkillHealth {
        let current = lineage.current().cloned().unwrap_or_else(|| LineageNode {
            version: lineage.current_version.clone(),
            parent_ids: vec![],
            source: LineageSource::AutoDistilled { task_id: "unknown".to_string(), trail_id: "unknown".to_string() },
            change_summary: "Unknown".to_string(),
            created_at: Utc::now(),
            quality_score: 0.0,
            usage_stats: UsageStats::default(),
            content_hash: String::new(),
        });

        let stats = &current.usage_stats;
        let success_rate = stats.success_rate();
        let days_since_use = stats.last_used
            .map(|last| (Utc::now() - last).num_days())
            .unwrap_or(365);

        let recommendation = if success_rate < 0.3 && stats.execution_count > 5 {
            LifecycleAction::NeedsRepair {
                reason: format!("Low success rate: {:.0}%", success_rate * 100.0),
            }
        } else if days_since_use > 90 && stats.execution_count > 0 {
            LifecycleAction::Archive {
                reason: "Stale skill (90+ days unused)".to_string(),
            }
        } else if success_rate > 0.9 && stats.execution_count > 10 {
            LifecycleAction::Promote {
                reason: "High performance skill".to_string(),
            }
        } else {
            LifecycleAction::Keep
        };

        SkillHealth {
            skill_id: lineage.skill_id.clone(),
            current_version: lineage.current_version.clone(),
            quality_score: current.quality_score,
            success_rate,
            execution_count: stats.execution_count,
            days_since_last_use: days_since_use,
            recommendation,
        }
    }
}

impl Default for SkillLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_creation() {
        let lineage = SkillLineage::new("test-skill", "1.0.0");
        assert_eq!(lineage.current_version, "1.0.0");
        assert!(lineage.has_version("1.0.0"));
    }

    #[test]
    fn test_add_version() {
        let mut lineage = SkillLineage::new("test", "1.0.0");
        lineage.add_version(LineageNode {
            version: "1.1.0".to_string(),
            parent_ids: vec!["1.0.0".to_string()],
            source: LineageSource::PatchFix { issue_id: "fix-1".to_string(), fix_description: "Fixed bug".to_string() },
            change_summary: "Bug fix".to_string(),
            created_at: Utc::now(),
            quality_score: 7.0,
            usage_stats: UsageStats::default(),
            content_hash: "abc".to_string(),
        });

        assert_eq!(lineage.current_version, "1.1.0");
        assert!(lineage.has_version("1.1.0"));
    }

    #[test]
    fn test_ancestors() {
        let mut lineage = SkillLineage::new("test", "1.0.0");
        lineage.add_version(LineageNode {
            version: "1.1.0".to_string(),
            parent_ids: vec!["1.0.0".to_string()],
            source: LineageSource::AutoDistilled { task_id: "t".to_string(), trail_id: "t".to_string() },
            change_summary: "Update".to_string(),
            created_at: Utc::now(),
            quality_score: 6.0,
            usage_stats: UsageStats::default(),
            content_hash: "b".to_string(),
        });

        let ancestors = lineage.ancestors("1.1.0");
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].version, "1.0.0");
        assert_eq!(ancestors[1].version, "1.1.0");
    }

    #[test]
    fn test_lifecycle_evaluate() {
        let manager = SkillLifecycleManager::new();
        let mut lineage = SkillLineage::new("test", "1.0.0");

        // High success, high usage → Promote
        lineage.add_version(LineageNode {
            version: "1.1.0".to_string(),
            parent_ids: vec!["1.0.0".to_string()],
            source: LineageSource::AutoDistilled { task_id: "t".to_string(), trail_id: "t".to_string() },
            change_summary: "Update".to_string(),
            created_at: Utc::now(),
            quality_score: 8.0,
            usage_stats: UsageStats {
                execution_count: 20,
                success_count: 19,
                failure_count: 1,
                avg_execution_time_ms: 100,
                last_used: Some(Utc::now()),
            },
            content_hash: "b".to_string(),
        });

        let health = manager.evaluate(&lineage);
        assert_eq!(health.recommendation, LifecycleAction::Promote { reason: "High performance skill".to_string() });
    }

    #[test]
    fn test_lifecycle_archive() {
        let manager = SkillLifecycleManager::new();
        let mut lineage = SkillLineage::new("old-skill", "1.0.0");

        lineage.add_version(LineageNode {
            version: "1.0.0".to_string(),
            parent_ids: vec![],
            source: LineageSource::AutoDistilled { task_id: "t".to_string(), trail_id: "t".to_string() },
            change_summary: "Old".to_string(),
            created_at: Utc::now() - chrono::Duration::days(100),
            quality_score: 5.0,
            usage_stats: UsageStats {
                execution_count: 5,
                success_count: 5,
                failure_count: 0,
                avg_execution_time_ms: 100,
                last_used: Some(Utc::now() - chrono::Duration::days(95)),
            },
            content_hash: "a".to_string(),
        });

        let health = manager.evaluate(&lineage);
        assert!(matches!(health.recommendation, LifecycleAction::Archive { .. }));
    }
}
