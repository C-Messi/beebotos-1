//! Patch Engine — Diff-Based Skill Updates with Safety Guarantees
//!
//! Applies updates to existing skills through structured diffs.
//! Safety features: rollback preparation, validation pre-checks, atomic
//! application.

use crate::error::{AgentError, Result};

/// A single atomic change to a skill
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOp {
    /// Insert new lines at position
    Insert { at_line: usize, lines: Vec<String> },
    /// Delete lines at position
    Delete { at_line: usize, count: usize },
    /// Replace a line range
    Replace {
        at_line: usize,
        count: usize,
        lines: Vec<String>,
    },
    /// Rename a section header
    RenameSection { old_name: String, new_name: String },
    /// Update frontmatter key
    UpdateMeta { key: String, value: String },
}

/// A complete patch with preconditions
#[derive(Debug, Clone)]
pub struct SkillPatch {
    /// Unique patch ID
    pub patch_id: String,
    /// Target skill ID
    pub target_skill_id: String,
    /// Target version to patch
    pub target_version: String,
    /// Preconditions that must hold for patch to be valid
    pub preconditions: Vec<Precondition>,
    /// The actual operations
    pub operations: Vec<PatchOp>,
    /// Human-readable change description
    pub description: String,
    /// Whether this patch was auto-generated
    pub auto_generated: bool,
}

/// A precondition check before applying patch
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// File must contain this text at given line
    FileMustContain { line: usize, text: String },
    /// File must NOT contain this text
    FileMustNotContain { text: String },
    /// Version must match
    VersionMustBe { version: String },
    /// Content hash must match (integrity check)
    HashMustMatch { hash: String },
}

/// Result of patch application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchResult {
    /// Patch applied successfully, new content returned
    Applied {
        new_content: String,
        new_version: String,
    },
    /// Patch was already present (idempotent)
    AlreadyApplied,
    /// Preconditions failed, patch not applied
    PreconditionsFailed { failed: Vec<String> },
    /// Patch application failed (conflict)
    Conflict { reason: String },
}

/// Patch engine — applies and validates skill patches
#[derive(Debug, Clone, Default)]
pub struct PatchEngine;

impl PatchEngine {
    pub fn new() -> Self {
        Self
    }

    /// Validate preconditions against current skill content
    pub fn validate_preconditions(&self, patch: &SkillPatch, content: &str) -> Vec<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut failures = Vec::new();

        for pre in &patch.preconditions {
            let failed = match pre {
                Precondition::FileMustContain { line, text } => {
                    if let Some(l) = lines.get(line.saturating_sub(1)) {
                        if !l.contains(text) {
                            Some(format!("Line {} must contain '{}'", line, text))
                        } else {
                            None
                        }
                    } else {
                        Some(format!("Line {} does not exist", line))
                    }
                }
                Precondition::FileMustNotContain { text } => {
                    if content.contains(text) {
                        Some(format!("File must not contain '{}'", text))
                    } else {
                        None
                    }
                }
                Precondition::VersionMustBe { version } => {
                    // Extract version from frontmatter
                    let found_version = Self::extract_version(content);
                    if found_version.as_deref() != Some(version) {
                        Some(format!("Version must be '{}'", version))
                    } else {
                        None
                    }
                }
                Precondition::HashMustMatch { hash } => {
                    let computed = Self::compute_hash(content);
                    if &computed != hash {
                        Some(format!("Hash mismatch: expected {}", hash))
                    } else {
                        None
                    }
                }
            };

            if let Some(msg) = failed {
                failures.push(msg);
            }
        }

        failures
    }

    /// Apply a patch to skill content (non-destructive, returns new content)
    pub fn apply(&self, patch: &SkillPatch, content: &str) -> Result<PatchResult> {
        // Check preconditions
        let failures = self.validate_preconditions(patch, content);
        if !failures.is_empty() {
            return Ok(PatchResult::PreconditionsFailed { failed: failures });
        }

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

        // Apply operations in order
        for op in &patch.operations {
            match op {
                PatchOp::Insert {
                    at_line,
                    lines: insert_lines,
                } => {
                    let pos = at_line.saturating_sub(1);
                    for (i, line) in insert_lines.iter().enumerate() {
                        if pos + i > lines.len() {
                            lines.push(line.clone());
                        } else {
                            lines.insert(pos + i, line.clone());
                        }
                    }
                }
                PatchOp::Delete { at_line, count } => {
                    let start = at_line.saturating_sub(1);
                    let end = (start + count).min(lines.len());
                    if start >= lines.len() {
                        return Err(AgentError::Execution(format!(
                            "Delete at line {} out of range (file has {} lines)",
                            at_line,
                            lines.len()
                        )));
                    }
                    lines.drain(start..end);
                }
                PatchOp::Replace {
                    at_line,
                    count,
                    lines: replace_lines,
                } => {
                    let start = at_line.saturating_sub(1);
                    let end = (start + count).min(lines.len());
                    if start >= lines.len() {
                        return Err(AgentError::Execution(format!(
                            "Replace at line {} out of range (file has {} lines)",
                            at_line,
                            lines.len()
                        )));
                    }
                    lines.splice(start..end, replace_lines.iter().cloned());
                }
                PatchOp::RenameSection { old_name, new_name } => {
                    for line in &mut lines {
                        if line.contains(old_name) && line.starts_with("#") {
                            *line = line.replace(old_name, new_name);
                        }
                    }
                }
                PatchOp::UpdateMeta { key, value } => {
                    let mut found = false;
                    for line in &mut lines {
                        if line.trim_start().starts_with(&format!("- **{}**:", key)) {
                            *line = format!("- **{}**: {}", key, value);
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        // Insert after metadata header or at top
                        if let Some(pos) = lines.iter().position(|l| l.trim() == "## 元信息") {
                            lines.insert(pos + 1, format!("- **{}**: {}", key, value));
                        }
                    }
                }
            }
        }

        let new_content = lines.join("\n");
        let new_version = Self::bump_version(patch.target_version.clone());

        Ok(PatchResult::Applied {
            new_content,
            new_version,
        })
    }

    /// Create a patch that reverts a skill to a previous version
    pub fn create_rollback_patch(
        &self,
        skill_id: &str,
        current_version: &str,
        target_version: &str,
        current_content: &str,
        target_content: &str,
    ) -> SkillPatch {
        let diff = Self::compute_diff(current_content, target_content);

        SkillPatch {
            patch_id: format!(
                "rollback-{}-{}-{}",
                skill_id, current_version, target_version
            ),
            target_skill_id: skill_id.to_string(),
            target_version: current_version.to_string(),
            preconditions: vec![Precondition::VersionMustBe {
                version: current_version.to_string(),
            }],
            operations: diff,
            description: format!("Rollback from {} to {}", current_version, target_version),
            auto_generated: true,
        }
    }

    /// Compute a simple line-based diff between two contents
    fn compute_diff(old: &str, new: &str) -> Vec<PatchOp> {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        let mut ops = Vec::new();
        let mut i = 0;
        let mut j = 0;

        while i < old_lines.len() || j < new_lines.len() {
            if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
                i += 1;
                j += 1;
            } else if j < new_lines.len()
                && (i >= old_lines.len() || !old_lines.contains(&new_lines[j]))
            {
                ops.push(PatchOp::Insert {
                    at_line: i + 1,
                    lines: vec![new_lines[j].to_string()],
                });
                j += 1;
            } else if i < old_lines.len() {
                ops.push(PatchOp::Delete {
                    at_line: i + 1,
                    count: 1,
                });
                i += 1;
            } else {
                break;
            }
        }

        ops
    }

    /// Bump a semantic version (patch level)
    fn bump_version(version: String) -> String {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() == 3 {
            if let Ok(patch) = parts[2].parse::<u32>() {
                format!("{}.{}.{}", parts[0], parts[1], patch + 1)
            } else {
                version
            }
        } else {
            version
        }
    }

    /// Extract version from skill markdown frontmatter
    fn extract_version(content: &str) -> Option<String> {
        for line in content.lines() {
            if line.contains("**version**") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    return Some(parts[1].trim().trim_matches('`').trim().to_string());
                }
            }
        }
        None
    }

    /// Compute a simple content hash for integrity checks
    fn compute_hash(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_insert() {
        let engine = PatchEngine::new();
        let patch = SkillPatch {
            patch_id: "p1".to_string(),
            target_skill_id: "s1".to_string(),
            target_version: "1.0.0".to_string(),
            preconditions: vec![],
            operations: vec![PatchOp::Insert {
                at_line: 2,
                lines: vec!["NEW LINE".to_string()],
            }],
            description: "Insert test".to_string(),
            auto_generated: false,
        };

        let result = engine.apply(&patch, "line1\nline2\nline3").unwrap();
        if let PatchResult::Applied { new_content, .. } = result {
            assert!(new_content.contains("NEW LINE"));
        } else {
            panic!("Expected Applied, got {:?}", result);
        }
    }

    #[test]
    fn test_apply_delete() {
        let engine = PatchEngine::new();
        let patch = SkillPatch {
            patch_id: "p1".to_string(),
            target_skill_id: "s1".to_string(),
            target_version: "1.0.0".to_string(),
            preconditions: vec![],
            operations: vec![PatchOp::Delete {
                at_line: 2,
                count: 1,
            }],
            description: "Delete test".to_string(),
            auto_generated: false,
        };

        let result = engine.apply(&patch, "line1\nline2\nline3").unwrap();
        if let PatchResult::Applied { new_content, .. } = result {
            assert!(!new_content.contains("line2"));
            assert!(new_content.contains("line1"));
            assert!(new_content.contains("line3"));
        } else {
            panic!("Expected Applied");
        }
    }

    #[test]
    fn test_precondition_version() {
        let engine = PatchEngine::new();
        let patch = SkillPatch {
            patch_id: "p1".to_string(),
            target_skill_id: "s1".to_string(),
            target_version: "2.0.0".to_string(),
            preconditions: vec![Precondition::VersionMustBe {
                version: "1.0.0".to_string(),
            }],
            operations: vec![],
            description: "test".to_string(),
            auto_generated: false,
        };

        let content = "- **version**: 2.0.0";
        let result = engine.apply(&patch, content).unwrap();
        assert!(matches!(result, PatchResult::PreconditionsFailed { .. }));
    }

    #[test]
    fn test_bump_version() {
        assert_eq!(PatchEngine::bump_version("1.0.0".to_string()), "1.0.1");
        assert_eq!(PatchEngine::bump_version("1.2.3".to_string()), "1.2.4");
        assert_eq!(PatchEngine::bump_version("2.0".to_string()), "2.0");
    }

    #[test]
    fn test_diff_compute() {
        let old = "A\nB\nC";
        let new = "A\nX\nC";
        let ops = PatchEngine::compute_diff(old, new);

        assert!(!ops.is_empty());
        // Should delete B and insert X
        let has_delete = ops.iter().any(|op| matches!(op, PatchOp::Delete { .. }));
        let has_insert = ops.iter().any(|op| matches!(op, PatchOp::Insert { .. }));
        assert!(has_delete || has_insert);
    }
}
