//! Skill Loader
//!
//! Loads and manages WASM-based skills from ClawHub.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::skills::registry::Version;

/// Skill loader
pub struct SkillLoader {
    skill_paths: Vec<PathBuf>,
    loaded_skills: HashMap<String, LoadedSkill>,
}

/// Loaded skill info
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub wasm_path: PathBuf,
    /// Path to the skill source directory or file (for lazy loading and script
    /// resolution)
    pub source_path: PathBuf,
    pub manifest: SkillManifest,
}

/// Function parameter definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionParameter {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: String,
    #[serde(default)]
    pub default_value: String,
}

/// Function definition exported by a skill
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<FunctionParameter>,
    #[serde(default)]
    pub outputs: Vec<FunctionParameter>,
    #[serde(default)]
    pub example: String,
}

/// Skill runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillRuntime {
    /// WASM-based skill (default)
    #[serde(alias = "wasm")]
    Wasm,
    /// Python script skill
    #[serde(alias = "python")]
    Python,
    /// Node.js script skill
    #[serde(alias = "nodejs", alias = "node")]
    NodeJs,
}

impl Default for SkillRuntime {
    fn default() -> Self {
        SkillRuntime::Wasm
    }
}

impl std::fmt::Display for SkillRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillRuntime::Wasm => write!(f, "wasm"),
            SkillRuntime::Python => write!(f, "python"),
            SkillRuntime::NodeJs => write!(f, "nodejs"),
        }
    }
}

/// Sandbox requirements declared by a skill
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SkillSandboxRequirements {
    /// Minimum memory required in MB
    #[serde(default)]
    pub min_memory_mb: usize,
    /// Maximum memory allowed in MB
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: usize,
    /// Whether network access is required
    #[serde(default)]
    pub network_allowed: bool,
    /// Allowed domains if network is enabled
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// Whether GPU access is required
    #[serde(default)]
    pub requires_gpu: bool,
    /// Additional filesystem paths to mount
    #[serde(default)]
    pub filesystem_paths: Vec<String>,
}

fn default_max_memory() -> usize {
    256
}

/// Skill manifest
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SkillManifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub entry_point: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub functions: Vec<FunctionDef>,
    /// 🆕 FIX: Markdown skill prompt template for LLM fallback execution
    #[serde(default)]
    pub prompt_template: String,
    /// 🆕 FIX: Few-shot examples for LLM fallback execution
    #[serde(default)]
    pub examples: String,
    /// 🆕 SKILL MATCHING V2: When to use this skill (REQUIRED for matching)
    #[serde(default)]
    pub when_to_use: String,
    /// 🆕 SKILL MATCHING V2: When NOT to use this skill
    #[serde(default)]
    pub when_not_to_use: Option<String>,
    /// 🆕 SKILL MATCHING V2: Positive examples that should activate this skill
    #[serde(default)]
    pub activation_examples: Vec<String>,
    /// 🆕 SKILL MATCHING V2: Negative examples that should NOT activate this
    /// skill
    #[serde(default)]
    pub activation_negative_examples: Vec<String>,
    /// 🆕 SKILL MATCHING V2: Skill dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// 🆕 FOREIGN RUNTIME: Runtime type for non-WASM skills
    #[serde(default)]
    pub runtime: SkillRuntime,
    /// 🆕 FOREIGN RUNTIME: Sandbox requirements for script execution
    #[serde(default)]
    pub sandbox: SkillSandboxRequirements,
    /// 🆕 FOREIGN RUNTIME: Runtime-specific dependencies
    #[serde(default)]
    pub runtime_dependencies: Vec<String>,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self {
            skill_paths: vec![],
            loaded_skills: HashMap::new(),
        }
    }

    /// Add skill search path
    pub fn add_path(&mut self, path: impl AsRef<Path>) {
        self.skill_paths.push(path.as_ref().to_path_buf());
    }

    /// Load skill from path
    pub async fn load_skill(&mut self, skill_id: &str) -> Result<LoadedSkill, SkillLoadError> {
        // Check if already loaded
        if let Some(skill) = self.loaded_skills.get(skill_id) {
            return Ok(skill.clone());
        }

        // Search for skill in paths
        for path in &self.skill_paths {
            let skill_path = path.join(skill_id);
            if skill_path.exists() {
                let manifest = self.load_manifest(&skill_path).await?;

                // WASM skills require skill.wasm; foreign runtime skills require entry_point file
                let wasm_path = skill_path.join("skill.wasm");
                let entry_path = skill_path.join(&manifest.entry_point);

                let artifact_path = match manifest.runtime {
                    SkillRuntime::Wasm => {
                        if !wasm_path.exists() {
                            return Err(SkillLoadError::InvalidManifest(format!(
                                "WASM file not found at {:?}",
                                wasm_path
                            )));
                        }
                        wasm_path
                    }
                    SkillRuntime::Python | SkillRuntime::NodeJs => {
                        if !entry_path.exists() {
                            return Err(SkillLoadError::InvalidManifest(format!(
                                "Entry point not found at {:?} for {:?} skill",
                                entry_path, manifest.runtime
                            )));
                        }
                        entry_path
                    }
                };

                let skill = LoadedSkill {
                    id: skill_id.to_string(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    wasm_path: artifact_path,
                    source_path: skill_path.to_path_buf(),
                    manifest,
                };

                self.loaded_skills
                    .insert(skill_id.to_string(), skill.clone());
                return Ok(skill);
            }
        }

        Err(SkillLoadError::SkillNotFound(skill_id.to_string()))
    }

    /// Load manifest from skill directory
    async fn load_manifest(&self, path: &Path) -> Result<SkillManifest, SkillLoadError> {
        let manifest_path = path.join("skill.yaml");
        let content = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| SkillLoadError::IoError(e.to_string()))?;

        let manifest: SkillManifest = serde_yaml::from_str(&content)
            .map_err(|e| SkillLoadError::ParseError(e.to_string()))?;

        Ok(manifest)
    }

    /// Get loaded skill
    pub fn get_skill(&self, skill_id: &str) -> Option<&LoadedSkill> {
        self.loaded_skills.get(skill_id)
    }

    /// List loaded skills
    pub fn list_skills(&self) -> Vec<&LoadedSkill> {
        self.loaded_skills.values().collect()
    }

    /// Unload skill
    pub fn unload_skill(&mut self, skill_id: &str) {
        self.loaded_skills.remove(skill_id);
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Skill load errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillLoadError {
    #[error("Skill not found: {0}")]
    SkillNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
}
