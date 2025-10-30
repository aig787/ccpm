//! Skills-specific resolution logic for pattern matching and dependency handling.
//!
//! This module contains specialized logic for resolving skill dependencies, which are
//! directory-based resources requiring special handling compared to file-based resources.

use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::utils::normalize_path_for_storage;
use anyhow::{Result, anyhow};
use glob::Pattern;
use std::path::Path;

/// Match skill directories in a base path that conform to a pattern.
///
/// Skills are directory-based resources that must contain a SKILL.md file.
/// This function finds all directories matching the given pattern that are valid skills.
///
/// Supports full glob pattern syntax:
/// - `*` - matches all skills
/// - Exact name - matches single skill (e.g., `my-skill`)
/// - Glob patterns - e.g., `ai-*`, `*-helper`, `test-[0-9]*`
///
/// # Arguments
///
/// * `base_path` - The base directory containing the skills/ subdirectory
/// * `pattern` - The glob pattern to match (e.g., "*", "my-skill", "ai-*")
/// * `strip_prefix` - Optional prefix to strip from matched paths (for Git sources)
///
/// # Returns
///
/// A vector of tuples containing (resource_name, absolute_path) for each matched skill
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::resolver::skills::match_skill_directories;
/// use std::path::Path;
///
/// // Match all skills
/// let all = match_skill_directories(Path::new("/repo"), "*", None)?;
///
/// // Match AI-related skills
/// let ai = match_skill_directories(Path::new("/repo"), "ai-*", None)?;
///
/// // Match specific skill
/// let one = match_skill_directories(Path::new("/repo"), "my-skill", None)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn match_skill_directories(
    base_path: &Path,
    pattern: &str,
    strip_prefix: Option<&Path>,
) -> Result<Vec<(String, String)>> {
    let mut matches = Vec::new();

    // Extract the skill-specific pattern (remove "skills/" prefix if present)
    let skill_pattern = pattern.strip_prefix("skills/").unwrap_or(pattern);

    let skills_base_path = base_path.join("skills");
    if !skills_base_path.exists() || !skills_base_path.is_dir() {
        tracing::debug!("Skills directory not found at {}", skills_base_path.display());
        return Ok(matches);
    }

    // Compile the glob pattern
    let glob_pattern = Pattern::new(skill_pattern)
        .map_err(|e| anyhow!("Invalid skill pattern '{}': {}", skill_pattern, e))?;

    let entries = std::fs::read_dir(&skills_base_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();

        // Check if this directory matches the glob pattern
        if !glob_pattern.matches(dir_name) {
            continue;
        }

        // Check if it contains SKILL.md
        let skill_md_path = path.join("SKILL.md");
        if !skill_md_path.exists() {
            tracing::warn!("Skipping directory {} - does not contain SKILL.md", path.display());
            continue;
        }

        let resource_name = dir_name.to_string();

        // Compute the path, optionally stripping a prefix
        // Use normalized paths (forward slashes) for cross-platform compatibility
        let concrete_path = if let Some(prefix) = strip_prefix {
            normalize_path_for_storage(path.strip_prefix(prefix).unwrap_or(&path))
        } else {
            normalize_path_for_storage(&path)
        };

        matches.push((resource_name, concrete_path));
    }

    Ok(matches)
}

/// Create a detailed dependency for a skill.
///
/// This helper creates a properly formatted DetailedDependency for a skill resource,
/// inheriting settings from the parent dependency if provided.
///
/// # Arguments
///
/// * `resource_name` - The name of the skill resource
/// * `path` - The path to the skill directory
/// * `source` - Optional source name for Git-based skills
/// * `parent_dep` - Optional parent dependency to inherit tool/target/flatten settings
///
/// # Returns
///
/// A ResourceDependency::Detailed variant configured for the skill
pub fn create_skill_dependency(
    resource_name: String,
    path: String,
    source: Option<String>,
    parent_dep: Option<&ResourceDependency>,
) -> (String, ResourceDependency) {
    let (tool, target, flatten, version) = if let Some(dep) = parent_dep {
        match dep {
            ResourceDependency::Detailed(d) => (
                d.tool.clone(),
                d.target.clone(),
                d.flatten,
                dep.get_version().map(std::string::ToString::to_string),
            ),
            _ => (None, None, None, None),
        }
    } else {
        (None, None, None, None)
    };

    (
        resource_name,
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source,
            path,
            version,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target,
            filename: None,
            dependencies: None,
            tool,
            flatten,
            install: None,
            template_vars: None,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_pattern_wildcard() {
        let pattern = Pattern::new("*").unwrap();
        assert!(pattern.matches("any-name"));
        assert!(pattern.matches("skill-1"));
        assert!(pattern.matches(""));
    }

    #[test]
    fn test_glob_pattern_exact() {
        let pattern = Pattern::new("my-skill").unwrap();
        assert!(pattern.matches("my-skill"));
        assert!(!pattern.matches("other-skill"));
        assert!(!pattern.matches("my-skill-extended"));
    }

    #[test]
    fn test_glob_pattern_prefix() {
        let pattern = Pattern::new("ai-*").unwrap();
        assert!(pattern.matches("ai-helper"));
        assert!(pattern.matches("ai-assistant"));
        assert!(pattern.matches("ai-"));
        assert!(!pattern.matches("helper-ai"));
        assert!(!pattern.matches("ai"));
    }

    #[test]
    fn test_glob_pattern_suffix() {
        let pattern = Pattern::new("*-helper").unwrap();
        assert!(pattern.matches("ai-helper"));
        assert!(pattern.matches("test-helper"));
        assert!(!pattern.matches("helper"));
        assert!(!pattern.matches("helper-test"));
    }

    #[test]
    fn test_glob_pattern_character_class() {
        let pattern = Pattern::new("test-[0-9]*").unwrap();
        assert!(pattern.matches("test-1"));
        assert!(pattern.matches("test-123"));
        assert!(pattern.matches("test-9-foo"));
        assert!(!pattern.matches("test-abc"));
        assert!(!pattern.matches("test-"));
    }

    #[test]
    fn test_create_skill_dependency_no_parent() {
        let (name, dep) = create_skill_dependency(
            "test-skill".to_string(),
            "skills/test-skill".to_string(),
            Some("community".to_string()),
            None,
        );

        assert_eq!(name, "test-skill");
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.path, "skills/test-skill");
                assert_eq!(d.source, Some("community".to_string()));
                assert_eq!(d.tool, None);
                assert_eq!(d.target, None);
                assert_eq!(d.flatten, None);
            }
            _ => panic!("Expected Detailed dependency"),
        }
    }

    #[test]
    fn test_create_skill_dependency_with_parent() {
        let parent = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "skills/*".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: Some(".custom/skills".to_string()),
            filename: None,
            dependencies: None,
            template_vars: None,
            tool: Some("claude-code".to_string()),
            flatten: Some(true),
            install: None,
        }));

        let (name, dep) = create_skill_dependency(
            "test-skill".to_string(),
            "skills/test-skill".to_string(),
            Some("community".to_string()),
            Some(&parent),
        );

        assert_eq!(name, "test-skill");
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.path, "skills/test-skill");
                assert_eq!(d.source, Some("community".to_string()));
                assert_eq!(d.tool, Some("claude-code".to_string()));
                assert_eq!(d.target, Some(".custom/skills".to_string()));
                assert_eq!(d.flatten, Some(true));
                assert_eq!(d.version, Some("v1.0.0".to_string()));
            }
            _ => panic!("Expected Detailed dependency"),
        }
    }
}
