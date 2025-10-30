//! Path validation and security utilities for AGPM.
//!
//! This module provides utilities for safe path handling, validation,
//! and security checks to prevent path traversal attacks.

use anyhow::{Context, Result, anyhow};
use std::path::{Component, Path, PathBuf};

/// Validates that a path is safe and within project boundaries.
///
/// # Arguments
/// * `path` - The path to validate
/// * `project_dir` - The project root directory
///
/// # Returns
/// The canonicalized path if valid
///
/// # Errors
/// Returns an error if:
/// - The path doesn't exist
/// - The path escapes the project directory
/// - The path cannot be canonicalized
pub fn validate_project_path(path: &Path, project_dir: &Path) -> Result<PathBuf> {
    let canonical = safe_canonicalize(path)?;
    let project_canonical = safe_canonicalize(project_dir)?;

    if !canonical.starts_with(&project_canonical) {
        return Err(anyhow!("Path '{}' escapes project directory", path.display()));
    }

    Ok(canonical)
}

/// Safely canonicalizes a path, handling various edge cases.
///
/// # Arguments
/// * `path` - The path to canonicalize
///
/// # Returns
/// The canonicalized path
///
/// # Errors
/// Returns an error if the path cannot be canonicalized
pub fn safe_canonicalize(path: &Path) -> Result<PathBuf> {
    // First check if the path exists
    if !path.exists() {
        // If it doesn't exist, try to canonicalize the parent
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            let canonical_parent = parent.canonicalize().with_context(|| {
                format!("Failed to canonicalize parent of '{}'", path.display())
            })?;

            if let Some(file_name) = path.file_name() {
                return Ok(canonical_parent.join(file_name));
            }
        }
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }

    path.canonicalize().with_context(|| format!("Failed to canonicalize path: {}", path.display()))
}

/// Ensures a path is within a specific directory boundary.
///
/// # Arguments
/// * `path` - The path to check
/// * `boundary` - The boundary directory
///
/// # Returns
/// `true` if the path is within the boundary
pub fn ensure_within_directory(path: &Path, boundary: &Path) -> Result<bool> {
    let canonical_path = safe_canonicalize(path)?;
    let canonical_boundary = safe_canonicalize(boundary)?;

    Ok(canonical_path.starts_with(&canonical_boundary))
}

/// Validates that a path doesn't contain dangerous components.
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// `Ok(())` if the path is safe
///
/// # Errors
/// Returns an error if the path contains dangerous components like:
/// - Parent directory references (..)
pub fn validate_no_traversal(path: &Path) -> Result<()> {
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(anyhow!(
                    "Path contains parent directory reference (..): {}",
                    path.display()
                ));
            }
            // Allow RootDir for absolute paths or paths that start with /
            // On Windows, /path is not absolute but is still valid within a project
            Component::RootDir => {
                // RootDir is OK - it just means the path starts with /
                // This is valid for both absolute paths and project-relative paths
            }
            _ => {}
        }
    }
    Ok(())
}

/// Creates a safe relative path from a base directory.
///
/// # Arguments
/// * `base` - The base directory
/// * `target` - The target path
///
/// # Returns
/// A relative path from base to target, or None if not possible
pub fn safe_relative_path(base: &Path, target: &Path) -> Result<PathBuf> {
    let base_canonical = safe_canonicalize(base)?;
    let target_canonical = safe_canonicalize(target)?;

    target_canonical.strip_prefix(&base_canonical).map(std::path::Path::to_path_buf).map_err(|_| {
        anyhow!("Cannot create relative path from {} to {}", base.display(), target.display())
    })
}

/// Ensures a directory exists, creating it if necessary.
///
/// # Arguments
/// * `dir` - The directory path
///
/// # Returns
/// The canonical path to the directory
///
/// # Errors
/// Returns an error if the directory cannot be created
pub fn ensure_directory_exists(dir: &Path) -> Result<PathBuf> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    }

    safe_canonicalize(dir)
}

/// Validates a skill installation path to prevent directory traversal.
///
/// # Arguments
/// * `installed_at` - The custom installation path from lockfile
/// * `skill_name` - The name of the skill being installed
/// * `project_dir` - The project directory
///
/// # Returns
/// The validated and normalized installation path
///
/// # Errors
/// Returns an error if:
/// - The path contains parent directory references (..)
/// - The path attempts to escape the project directory
/// - The path contains invalid characters
pub fn validate_skill_installation_path(
    installed_at: &str,
    skill_name: &str,
    project_dir: &Path,
) -> Result<PathBuf> {
    // First validate no traversal attempts in the custom path
    let path = Path::new(installed_at);
    validate_no_traversal(path)?;

    // Sanitize the skill name to prevent injection
    let safe_skill_name = sanitize_file_name(skill_name);
    if safe_skill_name.is_empty() {
        return Err(anyhow!("Invalid skill name: '{}'", skill_name));
    }

    // Build the full installation path
    let full_path = if path.is_absolute() {
        // Absolute paths are not allowed for skill installation
        return Err(anyhow!(
            "Skill installation path must be relative to project directory: '{}'",
            installed_at
        ));
    } else if installed_at.is_empty() {
        // Default path: .claude/skills/{skill_name}/
        project_dir.join(".claude").join("skills").join(&safe_skill_name)
    } else {
        // Custom path: ensure it's within project and ends with skill name
        let custom_path = project_dir.join(installed_at);

        // If the custom path doesn't already end with the skill name, append it
        let final_path = if custom_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n != safe_skill_name)
            .unwrap_or(true)
        {
            custom_path.join(&safe_skill_name)
        } else {
            custom_path
        };

        // Ensure the final path is within the project directory
        if let Some(parent) = final_path.parent() {
            if parent.exists() {
                let canonical_parent = safe_canonicalize(parent)?;
                let canonical_project = safe_canonicalize(project_dir)?;

                if !canonical_parent.starts_with(&canonical_project) {
                    return Err(anyhow!(
                        "Skill installation path '{}' escapes project directory",
                        installed_at
                    ));
                }
            }
        }

        final_path
    };

    // Additional validation: ensure we're not installing to sensitive locations
    let path_str = full_path.to_string_lossy();
    let sensitive_patterns = [
        ".git/",
        ".agpm/",
        "node_modules/",
        "target/",
        ".claude/settings.local.json",
        ".claude/settings.json",
    ];

    for pattern in &sensitive_patterns {
        if path_str.contains(pattern) {
            return Err(anyhow!(
                "Cannot install skill to sensitive location: '{}'",
                full_path.display()
            ));
        }
    }

    Ok(full_path)
}

/// Validates and normalizes a file path for a specific resource type.
///
/// # Arguments
/// * `path` - The path to validate
/// * `resource_type` - The type of resource (e.g., "agent", "snippet")
/// * `project_dir` - The project directory
///
/// # Returns
/// The validated and normalized path
pub fn validate_resource_path(
    path: &Path,
    resource_type: &str,
    project_dir: &Path,
) -> Result<PathBuf> {
    // Ensure no traversal attempts
    validate_no_traversal(path)?;

    // Build the full path
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    };

    // For new files that don't exist yet, validate the parent directory
    let canonical_project = safe_canonicalize(project_dir)?;

    if full_path.exists() {
        // If file exists, validate it's within project
        validate_project_path(&full_path, project_dir)?;
    } else {
        // For non-existent files, check parent directory
        if let Some(parent) = full_path.parent()
            && parent.exists()
        {
            let canonical_parent = safe_canonicalize(parent)?;
            if !canonical_parent.starts_with(&canonical_project) {
                return Err(anyhow!("Path '{}' escapes project directory", full_path.display()));
            }
        }
    }

    // Check file extension for resource files
    if resource_type != "directory" && full_path.extension().is_none_or(|ext| ext != "md") {
        return Err(anyhow!(
            "Invalid {} file: expected .md extension, got {}",
            resource_type,
            full_path.display()
        ));
    }

    Ok(full_path)
}

/// Sanitizes a file name to remove potentially dangerous characters.
///
/// # Arguments
/// * `name` - The file name to sanitize
///
/// # Returns
/// A sanitized version of the file name
pub fn sanitize_file_name(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.').collect()
}

/// Gets the project root directory from a path.
///
/// Searches upward from the given path to find a directory containing agpm.toml
///
/// # Arguments
/// * `start_path` - The path to start searching from
///
/// # Returns
/// The project root directory if found
pub fn find_project_root(start_path: &Path) -> Result<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent().ok_or_else(|| anyhow!("Invalid start path"))?
    } else {
        start_path
    };

    loop {
        if current.join("agpm.toml").exists() {
            return safe_canonicalize(current);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(anyhow!(
                    "No agpm.toml found in any parent directory of {}",
                    start_path.display()
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_validate_no_traversal() {
        // Valid paths
        assert!(validate_no_traversal(Path::new("foo/bar")).is_ok());
        assert!(validate_no_traversal(Path::new("/absolute/path")).is_ok());
        assert!(validate_no_traversal(Path::new("./relative")).is_ok());

        // Invalid paths
        assert!(validate_no_traversal(Path::new("../parent")).is_err());
        assert!(validate_no_traversal(Path::new("foo/../bar")).is_err());
        assert!(validate_no_traversal(Path::new("../../escape")).is_err());
    }

    #[test]
    fn test_sanitize_file_name() {
        assert_eq!(sanitize_file_name("valid-name_123.md"), "valid-name_123.md");
        assert_eq!(sanitize_file_name("bad/\\name<>:|?*"), "badname");
        assert_eq!(sanitize_file_name("spaces are removed"), "spacesareremoved");
    }

    #[test]
    fn test_validate_project_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let project_dir = temp_dir.path();

        // Create a test file
        let test_file = project_dir.join("test.md");
        fs::write(&test_file, "test")?;

        // Valid path within project
        let result = validate_project_path(&test_file, project_dir)?;
        let canonical_project = project_dir.canonicalize()?;
        assert!(result.starts_with(&canonical_project));

        // Path outside project should fail
        let outside_path = temp_dir.path().parent().unwrap().join("outside.md");
        assert!(validate_project_path(&outside_path, project_dir).is_err());

        Ok(())
    }

    #[test]
    fn test_ensure_directory_exists() -> Result<()> {
        let temp_dir = tempdir()?;
        let new_dir = temp_dir.path().join("new").join("nested").join("dir");

        assert!(!new_dir.exists());

        let result = ensure_directory_exists(&new_dir)?;
        assert!(result.exists());
        assert!(result.is_dir());

        Ok(())
    }

    #[test]
    fn test_find_project_root() -> Result<()> {
        let temp_dir = tempdir()?;
        let project_dir = temp_dir.path();

        // Create agpm.toml
        fs::write(project_dir.join("agpm.toml"), "[project]")?;

        // Create nested directory
        let nested = project_dir.join("src").join("nested");
        fs::create_dir_all(&nested)?;

        // Should find root from nested directory
        let found = find_project_root(&nested)?;
        assert_eq!(found, project_dir.canonicalize()?);

        // Should find root from file in nested directory
        let file_path = nested.join("file.rs");
        fs::write(&file_path, "// test")?;
        let found = find_project_root(&file_path)?;
        assert_eq!(found, project_dir.canonicalize()?);

        Ok(())
    }

    #[test]
    fn test_safe_relative_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let base = temp_dir.path();
        let target = base.join("subdir").join("file.md");

        // Create the target directory
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(&target, "test")?;

        let relative = safe_relative_path(base, &target)?;
        assert_eq!(relative, Path::new("subdir").join("file.md"));

        Ok(())
    }

    #[test]
    fn test_validate_resource_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let project_dir = temp_dir.path();

        // Valid agent path
        let agent_path = Path::new("agents/my-agent.md");
        let result = validate_resource_path(agent_path, "agent", project_dir);
        assert!(result.is_ok());

        // Invalid extension
        let wrong_ext = Path::new("agents/my-agent.txt");
        let result = validate_resource_path(wrong_ext, "agent", project_dir);
        assert!(result.is_err());

        // Path with traversal
        let traversal = Path::new("../outside/agent.md");
        let result = validate_resource_path(traversal, "agent", project_dir);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_validate_skill_installation_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let project_dir = temp_dir.path();

        // Default path (empty installed_at)
        let result = validate_skill_installation_path("", "my-skill", project_dir)?;
        let expected = project_dir.join(".claude").join("skills").join("my-skill");
        assert_eq!(result, expected);

        // Valid custom path
        let result =
            validate_skill_installation_path(".claude/custom-skills", "my-skill", project_dir)?;
        let expected = project_dir.join(".claude").join("custom-skills").join("my-skill");
        assert_eq!(result, expected);

        // Custom path already ending with skill name
        let result =
            validate_skill_installation_path(".claude/skills/my-skill", "my-skill", project_dir)?;
        let expected = project_dir.join(".claude").join("skills").join("my-skill");
        assert_eq!(result, expected);

        // Path with traversal should fail
        let result = validate_skill_installation_path("../outside/skills", "my-skill", project_dir);
        assert!(result.is_err());

        // Absolute path should fail
        #[cfg(windows)]
        let result = validate_skill_installation_path("C:\\absolute\\path", "my-skill", project_dir);
        #[cfg(not(windows))]
        let result = validate_skill_installation_path("/absolute/path", "my-skill", project_dir);
        assert!(result.is_err());

        // Sensitive locations should fail
        let result = validate_skill_installation_path(".git/skills", "my-skill", project_dir);
        assert!(result.is_err());

        let result = validate_skill_installation_path(
            ".claude/settings.local.json",
            "my-skill",
            project_dir,
        );
        assert!(result.is_err());

        // Invalid skill name should be sanitized
        let result = validate_skill_installation_path("", "my/invalid\\skill", project_dir)?;
        let expected = project_dir.join(".claude").join("skills").join("myinvalidskill");
        assert_eq!(result, expected);

        Ok(())
    }
}
