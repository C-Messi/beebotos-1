//! Builtin Skill Loader
//!
//! Scans the project `skills/` directory and registers markdown-defined skills
//! as lightweight builtins. These skills have no WASM binary; execution falls
//! back to LLM with the skill description as a system prompt.
//!
//! 🆕 FIX: Now parses deep markdown sections (Prompt Template, Examples,
//! Capabilities) so high-quality skills actually deliver their full value.
//! 🆕 FIX: Supports both directory-based skills (SKILL.md) and legacy flat .md
//! files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::skills::discovery::{SkillDiscovery, SkillKind, SkillMetadata};
use crate::skills::loader::{LoadedSkill, SkillManifest};
use crate::skills::registry::SkillRegistry;

/// Parse a markdown file into structured sections.
/// Returns a map: section_name -> content (without the ## heading line)
fn parse_markdown_sections(content: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            // Save previous section
            if let Some(ref name) = current_section {
                let body = current_lines.join("\n").trim().to_string();
                if !body.is_empty() {
                    sections.insert(name.clone(), body);
                }
            }
            // Start new section
            current_section = Some(line[3..].trim().to_lowercase().replace(' ', "_"));
            current_lines.clear();
        } else if current_section.is_some() {
            current_lines.push(line.to_string());
        }
    }

    // Save final section
    if let Some(ref name) = current_section {
        let body = current_lines.join("\n").trim().to_string();
        if !body.is_empty() {
            sections.insert(name.clone(), body);
        }
    }

    sections
}

/// Extract bullet-point capabilities from a capabilities text block.
fn parse_capabilities(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                Some(trimmed[2..].trim().to_string())
            } else if trimmed.starts_with("• ") {
                Some(trimmed[2..].trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 🆕 SKILL MATCHING V2: Extract bullet-point examples from an examples text
/// block.
fn parse_examples(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                Some(trimmed[2..].trim().to_string())
            } else if trimmed.starts_with("• ") {
                Some(trimmed[2..].trim().to_string())
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Build tags from content keywords.
fn build_tags(content: &str) -> Vec<String> {
    let mut tags = vec![];
    let lower = content.to_lowercase();
    if lower.contains("search") || lower.contains("搜索") {
        tags.push("search".to_string());
    }
    if lower.contains("analysis") || lower.contains("分析") {
        tags.push("analysis".to_string());
    }
    if lower.contains("plan") || lower.contains("计划") || lower.contains("规划") {
        tags.push("planning".to_string());
    }
    if lower.contains("code") || lower.contains("代码") || lower.contains("开发") {
        tags.push("coding".to_string());
    }
    if lower.contains("write") || lower.contains("写") || lower.contains("邮件") {
        tags.push("writing".to_string());
    }
    if lower.contains("data") || lower.contains("数据") {
        tags.push("data".to_string());
    }
    if lower.contains("travel")
        || lower.contains("tour")
        || lower.contains("旅游")
        || lower.contains("旅行")
    {
        tags.push("travel".to_string());
    }
    if lower.contains("translate") || lower.contains("翻译") || lower.contains("语言") {
        tags.push("translation".to_string());
    }
    if lower.contains("cook")
        || lower.contains("recipe")
        || lower.contains("食谱")
        || lower.contains("做菜")
    {
        tags.push("cooking".to_string());
    }
    if lower.contains("fitness")
        || lower.contains("workout")
        || lower.contains("健身")
        || lower.contains("运动")
    {
        tags.push("fitness".to_string());
    }
    if lower.contains("movie")
        || lower.contains("film")
        || lower.contains("电影")
        || lower.contains("剧集")
    {
        tags.push("entertainment".to_string());
    }
    if lower.contains("news") || lower.contains("新闻") || lower.contains("资讯") {
        tags.push("news".to_string());
    }
    if lower.contains("weather") || lower.contains("天气") || lower.contains("气温") {
        tags.push("weather".to_string());
    }
    if lower.contains("calculat")
        || lower.contains("math")
        || lower.contains("计算")
        || lower.contains("房贷")
        || lower.contains("投资")
    {
        tags.push("calculator".to_string());
    }
    tags
}

/// Build a LoadedSkill from SkillMetadata and markdown content.
/// Shared logic used by both load_builtin_skills and
/// load_markdown_skill_from_dir.
fn build_loaded_skill(meta: &SkillMetadata, content: &str) -> LoadedSkill {
    // Parse deep markdown sections
    let sections = parse_markdown_sections(content);

    let description = sections.get("description").cloned().unwrap_or_else(|| {
        content
            .lines()
            .skip(1)
            .skip_while(|l| l.trim().is_empty() || l.trim().starts_with('#'))
            .take_while(|l| !l.trim().starts_with('#') && !l.trim().starts_with("```"))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    });

    let description = if description.is_empty() {
        meta.description.clone()
    } else {
        description
    };

    let prompt_template = sections.get("prompt_template").cloned().unwrap_or_default();
    let examples = sections.get("examples").cloned().unwrap_or_default();
    let capabilities = sections
        .get("capabilities")
        .map(|text| parse_capabilities(text))
        .unwrap_or_default();

    // 🆕 SKILL MATCHING V2: Parse activation examples from markdown sections
    let when_to_use = sections
        .get("when_to_use")
        .cloned()
        .unwrap_or_else(|| description.clone());
    let when_not_to_use = sections.get("when_not_to_use").cloned();
    let activation_examples = sections
        .get("activation_examples")
        .map(|text| parse_examples(text))
        .unwrap_or_default();
    let activation_negative_examples = sections
        .get("activation_negative_examples")
        .map(|text| parse_examples(text))
        .unwrap_or_default();
    let dependencies = sections
        .get("dependencies")
        .map(|text| parse_capabilities(text))
        .unwrap_or_default();

    let tags = if meta.tags.is_empty() {
        build_tags(content)
    } else {
        meta.tags.clone()
    };

    // 🆕 FIX: Set wasm_path for Wasm-kind skills so agent_impl.rs routes correctly
    let wasm_path = if meta.kind == SkillKind::Wasm {
        meta.path.join("skill.wasm")
    } else {
        PathBuf::new()
    };

    LoadedSkill {
        id: meta.id.clone(),
        name: meta.name.clone(),
        version: meta.version.clone(),
        wasm_path,
        source_path: meta.path.clone(),
        manifest: SkillManifest {
            id: meta.id.clone(),
            name: meta.name.clone(),
            version: meta.version.clone(),
            description: description.clone(),
            author: "BeeBotOS".to_string(),
            capabilities: if capabilities.is_empty() {
                tags.clone()
            } else {
                capabilities
            },
            permissions: vec!["llm:chat".to_string()],
            entry_point: "run".to_string(),
            license: "MIT".to_string(),
            functions: vec![],
            prompt_template,
            examples,
            when_to_use,
            when_not_to_use,
            activation_examples,
            activation_negative_examples,
            dependencies,
        },
    }
}

/// Load a single markdown-defined skill from a directory.
/// The directory must contain a SKILL.md file (for directory-based skills)
/// or be a flat .md file.
/// Supports all three skill kinds: Knowledge, Code (with scripts), and Wasm.
pub async fn load_markdown_skill_from_dir(path: &Path) -> Option<LoadedSkill> {
    let meta = SkillDiscovery::inspect_directory(path).await?;

    let content = if meta.path.is_dir() {
        let md_path = meta.path.join("SKILL.md");
        if md_path.exists() {
            tokio::fs::read_to_string(&md_path).await.ok()?
        } else {
            return None;
        }
    } else {
        tokio::fs::read_to_string(&meta.path).await.ok()?
    };

    Some(build_loaded_skill(&meta, &content))
}

/// Scan `skills/` directory and register all skills into the given registry.
pub async fn load_builtin_skills(registry: &Arc<SkillRegistry>) {
    let mut discovery = SkillDiscovery::new();
    discovery.add_path("skills");

    let metas = discovery.scan().await;
    let mut registered = 0;

    for meta in metas {
        let content = if meta.path.is_dir() {
            let md_path = meta.path.join("SKILL.md");
            if md_path.exists() {
                tokio::fs::read_to_string(&md_path)
                    .await
                    .unwrap_or_default()
            } else {
                continue;
            }
        } else {
            tokio::fs::read_to_string(&meta.path)
                .await
                .unwrap_or_default()
        };

        let skill = build_loaded_skill(&meta, &content);

        let category = if meta.category.is_empty() {
            if meta.path.is_dir() {
                meta.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("general")
            } else {
                meta.path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("general")
            }
        } else {
            &meta.category
        };

        let tags = if meta.tags.is_empty() {
            build_tags(&content)
        } else {
            meta.tags.clone()
        };

        registry.register(skill, category, tags).await;
        registered += 1;
    }

    if registered > 0 {
        tracing::info!(
            "✅ Registered {} built-in skills from skills/ directory",
            registered
        );
    }
}
