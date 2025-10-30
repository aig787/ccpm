//! Skills module for AGPM
//!
//! This module provides functionality for managing Claude Skills, which are
//! directory-based resources containing a SKILL.md file with frontmatter and
//! optional supporting files.
//!
//! ## What are Skills?
//!
//! Skills are directories that:
//! - Contain a SKILL.md file with required YAML frontmatter
//! - May include additional files (REFERENCE.md, scripts, examples)
//! - Install to `.claude/skills/<name>/` as directories
//! - Can declare dependencies on other resources
//! - Support patching for customization
//!
//! ## SKILL.md Format
//!
//! ```yaml
//! ---
//! name: Skill Name
//! description: What this skill does
//! version: 1.0.0  # optional
//! allowed-tools: Read, Grep  # optional
//! dependencies:  # optional
//!   agents:
//!     - path: agents/helper.md
//! ---
//! # Skill content in markdown
//! ```

pub mod patches;

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Maximum number of files allowed in a skill directory (hard limit)
const MAX_SKILL_FILES: usize = 1000;

/// Maximum total size in bytes for all files in a skill (hard limit)
const MAX_SKILL_SIZE_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

/// Frontmatter structure for SKILL.md files
///
/// This struct represents the YAML frontmatter that must be present
/// in every SKILL.md file. It defines the skill's metadata and
/// configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    /// Human-readable name of the skill
    pub name: String,

    /// Description of what the skill does
    pub description: String,

    /// Optional version identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional list of tools the skill is allowed to use
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional dependencies on other resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<serde_yaml::Value>,
}

/// Validate and extract frontmatter from SKILL.md content
///
/// This function parses the YAML frontmatter from a SKILL.md file,
/// validates that required fields are present, and returns the
/// structured frontmatter data.
///
/// # Arguments
///
/// * `content` - The full content of the SKILL.md file
///
/// # Returns
///
/// Returns the parsed frontmatter if valid
///
/// # Errors
///
/// Returns an error if:
/// - The file doesn't have proper YAML frontmatter (missing --- markers)
/// - The YAML is invalid
/// - Required fields (name, description) are missing or empty
///
/// # Examples
///
/// ```
/// use agpm_cli::skills::validate_skill_frontmatter;
///
/// # fn example() -> anyhow::Result<()> {
/// let content = r#"---
/// name: My Skill
/// description: A helpful skill
/// ---
/// # My Skill
///
/// This skill helps with...
/// "#;
///
/// let frontmatter = validate_skill_frontmatter(content)?;
/// assert_eq!(frontmatter.name, "My Skill");
/// assert_eq!(frontmatter.description, "A helpful skill");
/// # Ok(())
/// # }
/// ```
pub fn validate_skill_frontmatter(content: &str) -> Result<SkillFrontmatter> {
    // Split content by --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();

    if parts.len() < 3 {
        return Err(anyhow!(
            "SKILL.md missing required YAML frontmatter. Format:\n---\nname: Skill Name\ndescription: What it does\n---\n# Content"
        ));
    }

    // Parse YAML frontmatter
    let frontmatter_str = parts[1].trim();
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(frontmatter_str).map_err(|e| {
        anyhow!("Invalid SKILL.md frontmatter: {}\nYAML content:\n{}", e, frontmatter_str)
    })?;

    // Validate required fields
    if frontmatter.name.trim().is_empty() {
        return Err(anyhow!("SKILL.md frontmatter missing required 'name' field"));
    }

    if frontmatter.description.trim().is_empty() {
        return Err(anyhow!("SKILL.md frontmatter missing required 'description' field"));
    }

    // Validate name contains only allowed characters
    if !frontmatter.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ') {
        return Err(anyhow!(
            "Skill name contains invalid characters. Use letters, numbers, spaces, hyphens, and underscores only"
        ));
    }

    Ok(frontmatter)
}

/// Validate skill directory size and file count before installation.
///
/// This prevents malicious or accidentally large skills from consuming
/// excessive disk space or inodes. Checks:
/// - File count ≤ MAX_SKILL_FILES (1000)
/// - Total size ≤ MAX_SKILL_SIZE_BYTES (100MB)
/// - No symlinks (security risk: could point to sensitive files)
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory to validate
///
/// # Returns
///
/// * `Ok(())` - Skill passes all size and security checks
/// * `Err(anyhow::Error)` - Skill exceeds limits or contains symlinks
///
/// # Security
///
/// This function rejects symlinks to prevent:
/// - Data exfiltration (symlink to /etc/passwd, ~/.ssh/id_rsa)
/// - Path traversal attacks
/// - Unexpected behavior across platforms
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::skills::validate_skill_size;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// validate_skill_size(Path::new("my-skill")).await?;
/// # Ok(())
/// # }
/// ```
pub async fn validate_skill_size(skill_path: &Path) -> Result<()> {
    let mut total_size = 0u64;
    let mut file_count = 0usize;

    for entry in walkdir::WalkDir::new(skill_path).follow_links(false) {
        let entry = entry?;

        // Reject symlinks (security: could point to /etc/passwd, etc.)
        if entry.file_type().is_symlink() {
            return Err(anyhow!(
                "Skill at {} contains symlinks, which are not allowed for security reasons. \
                Symlinks could point to sensitive files or cause unexpected behavior across platforms.",
                skill_path.display()
            ));
        }

        if entry.file_type().is_file() {
            file_count += 1;
            total_size += entry.metadata()?.len();

            // Check file count limit
            if file_count > MAX_SKILL_FILES {
                return Err(anyhow!(
                    "Skill at {} contains {} files, which exceeds the maximum limit of {} files. \
                    Skills should be focused and minimal. Consider splitting into multiple skills.",
                    skill_path.display(),
                    file_count,
                    MAX_SKILL_FILES
                ));
            }

            // Check size limit
            if total_size > MAX_SKILL_SIZE_BYTES {
                let size_mb = total_size as f64 / (1024.0 * 1024.0);
                let limit_mb = MAX_SKILL_SIZE_BYTES as f64 / (1024.0 * 1024.0);
                return Err(anyhow!(
                    "Skill at {} total size is {:.2} MB, which exceeds the maximum limit of {:.0} MB. \
                    Skills should be focused and minimal. Consider optimizing file sizes or removing unnecessary files.",
                    skill_path.display(),
                    size_mb,
                    limit_mb
                ));
            }
        }
    }

    Ok(())
}

/// Extract metadata from a skill directory
///
/// This function reads a skill directory, validates its structure,
/// and extracts metadata including the frontmatter and file list.
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory
///
/// # Returns
///
/// Returns a tuple of (frontmatter, file_list) if valid
///
/// # Examples
///
/// ```
/// use agpm_cli::skills::extract_skill_metadata;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let (frontmatter, files) = extract_skill_metadata(Path::new("my-skill"))?;
/// println!("Skill: {}", frontmatter.name);
/// println!("Files: {:?}", files);
/// # Ok(())
/// # }
/// ```
pub fn extract_skill_metadata(skill_path: &Path) -> Result<(SkillFrontmatter, Vec<String>)> {
    tracing::debug!("extract_skill_metadata called with path: {}", skill_path.display());

    if !skill_path.is_dir() {
        return Err(anyhow!("Skill path {} is not a directory", skill_path.display()));
    }

    let skill_md_path = skill_path.join("SKILL.md");
    tracing::debug!("Looking for SKILL.md at: {}", skill_md_path.display());

    if !skill_md_path.exists() {
        return Err(anyhow!("Skill at {} missing required SKILL.md file", skill_path.display()));
    }

    // Read and validate SKILL.md
    tracing::debug!("Reading SKILL.md file...");
    let skill_md_content = std::fs::read_to_string(&skill_md_path).with_file_context(
        FileOperation::Read,
        &skill_md_path,
        "loading skill metadata",
        "extract_skill_metadata",
    )?;

    let frontmatter = validate_skill_frontmatter(&skill_md_content)?;

    // Collect all files in the skill directory and calculate total size
    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    for entry in
        walkdir::WalkDir::new(skill_path).follow_links(false).into_iter().filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let relative_path = entry
                .path()
                .strip_prefix(skill_path)
                .map_err(|e| anyhow!("Failed to get relative path: {}", e))?;

            files.push(relative_path.to_string_lossy().to_string());

            // Add file size to total
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
            }
        }
    }

    // Sort files for consistent ordering
    files.sort();

    // Validate file count (hard limit - error)
    if files.len() > MAX_SKILL_FILES {
        return Err(anyhow!(
            "Skill '{}' contains {} files, which exceeds the maximum limit of {} files. \
            Skills should be focused and minimal. Consider splitting into multiple skills or removing unnecessary files.",
            frontmatter.name,
            files.len(),
            MAX_SKILL_FILES
        ));
    }

    // Validate total size (hard limit - error)
    if total_size > MAX_SKILL_SIZE_BYTES {
        let size_mb = total_size as f64 / (1024.0 * 1024.0);
        let limit_mb = MAX_SKILL_SIZE_BYTES as f64 / (1024.0 * 1024.0);
        return Err(anyhow!(
            "Skill '{}' total size is {:.2} MB, which exceeds the maximum limit of {:.0} MB. \
            Skills should be focused and minimal. Consider optimizing file sizes or splitting into multiple skills.",
            frontmatter.name,
            size_mb,
            limit_mb
        ));
    }

    Ok((frontmatter, files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_skill_frontmatter_valid() {
        let content = r#"---
name: Test Skill
description: A test skill
version: 1.0.0
allowed-tools:
  - Read
  - Write
dependencies:
  agents:
    - path: helper.md
---
# Test Skill

This is a test skill.
"#;

        let result = validate_skill_frontmatter(content).unwrap();
        assert_eq!(result.name, "Test Skill");
        assert_eq!(result.description, "A test skill");
        assert_eq!(result.version, Some("1.0.0".to_string()));
        assert_eq!(result.allowed_tools, Some(vec!["Read".to_string(), "Write".to_string()]));
    }

    #[test]
    fn test_validate_skill_frontmatter_missing_fields() {
        let content = r#"---
name: Test Skill
---
# Test Skill
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("description"));
    }

    #[test]
    fn test_validate_skill_frontmatter_no_frontmatter() {
        let content = r#"# Test Skill

This skill has no frontmatter.
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing required YAML frontmatter"));
    }

    #[test]
    fn test_validate_skill_frontmatter_invalid_yaml() {
        let content = r#"---
name: Test Skill
description: Invalid YAML
unclosed: [ "item1", "item2"
---
# Test Skill
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid SKILL.md frontmatter"));
    }
}
