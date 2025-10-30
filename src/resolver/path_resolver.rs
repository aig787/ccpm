//! Path computation and resolution for dependency installation.
//!
//! This module provides utilities for computing installation paths, resolving
//! pattern paths, determining flatten behavior, and handling all path-related
//! operations for resource dependencies. It supports both merge-target resources
//! (Hooks, MCP servers) and regular file-based resources (Agents, Commands,
//! Snippets, Scripts).

use crate::core::ResourceType;
use crate::manifest::{Manifest, ResourceDependency};
use crate::utils::{compute_relative_install_path, normalize_path, normalize_path_for_storage};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parses a pattern string to extract the base path and pattern components.
///
/// Handles three cases:
/// 1. Patterns with path separators and absolute/relative parents
/// 2. Patterns with path separators but simple relative paths
/// 3. Simple patterns without path separators
///
/// # Arguments
///
/// * `pattern` - The glob pattern string (e.g., "agents/*.md", "../foo/*.md")
///
/// # Returns
///
/// A tuple of (base_path, pattern_str) where:
/// - `base_path` is the directory to search in
/// - `pattern_str` is the glob pattern to match files against
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_resolver::parse_pattern_base_path;
///
/// let (base, pattern) = parse_pattern_base_path("agents/*.md");
/// assert_eq!(base, PathBuf::from("."));
/// assert_eq!(pattern, "agents/*.md");
///
/// let (base, pattern) = parse_pattern_base_path("../foo/bar/*.md");
/// assert_eq!(base, PathBuf::from("../foo/bar"));
/// assert_eq!(pattern, "*.md");
/// ```
pub fn parse_pattern_base_path(pattern: &str) -> (PathBuf, String) {
    if pattern.contains('/') || pattern.contains('\\') {
        // Pattern contains path separators, extract base path
        let pattern_path = Path::new(pattern);
        if let Some(parent) = pattern_path.parent() {
            if parent.is_absolute() || parent.starts_with("..") || parent.starts_with(".") {
                // Use the parent as base path and just the filename pattern
                (
                    parent.to_path_buf(),
                    pattern_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(pattern)
                        .to_string(),
                )
            } else {
                // Relative path, use current directory as base
                (PathBuf::from("."), pattern.to_string())
            }
        } else {
            // No parent, use current directory
            (PathBuf::from("."), pattern.to_string())
        }
    } else {
        // Simple pattern without path separators
        (PathBuf::from("."), pattern.to_string())
    }
}

/// Computes the installation path for a merge-target resource (Hook or McpServer).
///
/// These resources are not installed as files but are merged into configuration files.
/// The installation path is determined by the tool's merge target configuration or
/// hardcoded defaults.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Hook or McpServer)
///
/// # Returns
///
/// The normalized path to the merge target configuration file.
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_resolver::compute_merge_target_install_path;
///
/// let manifest = Manifest::new();
/// let path = compute_merge_target_install_path(&manifest, "claude-code", ResourceType::Hook);
/// assert_eq!(path, ".claude/settings.local.json");
/// ```
pub fn compute_merge_target_install_path(
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> String {
    // Use configured merge target, with fallback to hardcoded defaults
    if let Some(merge_target) = manifest.get_merge_target(artifact_type, resource_type) {
        normalize_path_for_storage(merge_target.display().to_string())
    } else {
        // Fallback to hardcoded defaults if not configured
        match resource_type {
            ResourceType::Hook => ".claude/settings.local.json".to_string(),
            ResourceType::McpServer => {
                if artifact_type == "opencode" {
                    ".opencode/opencode.json".to_string()
                } else {
                    ".mcp.json".to_string()
                }
            }
            _ => unreachable!(
                "compute_merge_target_install_path should only be called for Hook or McpServer"
            ),
        }
    }
}

/// Computes the installation path for a regular resource (Agent, Command, Snippet, Script).
///
/// Regular resources are installed as files in tool-specific directories. This function
/// determines the final installation path by:
/// 1. Getting the base artifact path from tool configuration
/// 2. Applying any custom target override from the dependency
/// 3. Computing the relative path based on flatten behavior
/// 4. Avoiding redundant directory prefixes
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, etc.)
/// * `filename` - The meaningful path structure extracted from the source file
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported
/// by the specified tool.
///
/// # Errors
///
/// Returns an error if:
/// - The resource type is not supported by the specified tool
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_resolver::compute_regular_resource_install_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let path = compute_regular_resource_install_path(
///     &manifest,
///     &dep,
///     "claude-code",
///     ResourceType::Agent,
///     "agents/helper.md"
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn compute_regular_resource_install_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    filename: &str,
) -> Result<String> {
    // Get the artifact path for this resource type
    let artifact_path =
        manifest.get_artifact_resource_path(artifact_type, resource_type).ok_or_else(|| {
            anyhow::anyhow!(
                "Resource type '{}' is not supported by tool '{}'",
                resource_type,
                artifact_type
            )
        })?;

    // Determine flatten behavior: use explicit setting or tool config default
    let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);

    // Determine the base target directory
    let base_target = if let Some(custom_target) = dep.get_target() {
        // Custom target is relative to the artifact's resource directory
        PathBuf::from(artifact_path.display().to_string())
            .join(custom_target.trim_start_matches('/'))
    } else {
        artifact_path.to_path_buf()
    };

    // Use compute_relative_install_path to avoid redundant prefixes
    let relative_path = compute_relative_install_path(&base_target, Path::new(filename), flatten);
    Ok(normalize_path_for_storage(normalize_path(&base_target.join(relative_path))))
}

/// Determines the flatten behavior for a resource installation.
///
/// Flatten behavior controls whether directory structure from the source repository
/// is preserved in the installation path. The decision is made by checking:
/// 1. Explicit `flatten` setting on the dependency (highest priority)
/// 2. Tool configuration default for this resource type
/// 3. Global default (false)
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, etc.)
///
/// # Returns
///
/// `true` if directory structure should be flattened, `false` if it should be preserved.
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_resolver::get_flatten_behavior;
///
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let flatten = get_flatten_behavior(&manifest, &dep, "claude-code", ResourceType::Agent);
/// ```
pub fn get_flatten_behavior(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
) -> bool {
    let dep_flatten = dep.get_flatten();
    let tool_flatten = manifest
        .get_tool_config(artifact_type)
        .and_then(|config| config.resources.get(resource_type.to_plural()))
        .and_then(|resource_config| resource_config.flatten);

    dep_flatten.or(tool_flatten).unwrap_or(false) // Default to false if not configured
}

/// Constructs the full relative path for a matched pattern file.
///
/// Combines the base path with the matched file path, normalizing path separators
/// for storage in the lockfile.
///
/// # Arguments
///
/// * `base_path` - The base directory the pattern was resolved in
/// * `matched_path` - The path to the matched file (relative to base_path)
///
/// # Returns
///
/// A normalized path string suitable for storage in the lockfile.
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_resolver::construct_full_relative_path;
///
/// let base = PathBuf::from(".");
/// let matched = Path::new("agents/helper.md");
/// let path = construct_full_relative_path(&base, matched);
/// assert_eq!(path, "agents/helper.md");
///
/// let base = PathBuf::from("../foo");
/// let matched = Path::new("bar.md");
/// let path = construct_full_relative_path(&base, matched);
/// assert_eq!(path, "../foo/bar.md");
/// ```
pub fn construct_full_relative_path(base_path: &Path, matched_path: &Path) -> String {
    if base_path == Path::new(".") {
        crate::utils::normalize_path_for_storage(matched_path.to_string_lossy().to_string())
    } else {
        crate::utils::normalize_path_for_storage(format!(
            "{}/{}",
            base_path.display(),
            matched_path.display()
        ))
    }
}

/// Extracts the meaningful path for pattern matching.
///
/// Constructs the full path from base path and matched path, then extracts
/// the meaningful structure by removing redundant directory prefixes.
///
/// # Arguments
///
/// * `base_path` - The base directory the pattern was resolved in
/// * `matched_path` - The path to the matched file (relative to base_path)
///
/// # Returns
///
/// The meaningful path structure string.
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_resolver::extract_pattern_filename;
///
/// let base = PathBuf::from(".");
/// let matched = Path::new("agents/helper.md");
/// let filename = extract_pattern_filename(&base, matched);
/// assert_eq!(filename, "agents/helper.md");
/// ```
pub fn extract_pattern_filename(base_path: &Path, matched_path: &Path) -> String {
    let full_path = if base_path == Path::new(".") {
        matched_path.to_path_buf()
    } else {
        base_path.join(matched_path)
    };
    extract_meaningful_path(&full_path)
}

/// Extracts the meaningful path by removing redundant directory prefixes.
///
/// This prevents paths like `.claude/agents/agents/file.md` by eliminating
/// duplicate directory components.
///
/// # Arguments
///
/// * `path` - The path to extract meaningful structure from
///
/// # Returns
///
/// The normalized meaningful path string
pub fn extract_meaningful_path(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();

    if path.is_absolute() {
        // Case 2: Absolute path - resolve ".." components first, then strip root
        let mut resolved = Vec::new();

        for component in components.iter() {
            match component {
                std::path::Component::Normal(name) => {
                    resolved.push(name.to_str().unwrap_or(""));
                }
                std::path::Component::ParentDir => {
                    // Pop the last component if there is one
                    resolved.pop();
                }
                // Skip RootDir, Prefix, and CurDir
                _ => {}
            }
        }

        resolved.join("/")
    } else if components.iter().any(|c| matches!(c, std::path::Component::ParentDir)) {
        // Case 1: Relative path with "../" - skip all parent components
        let start_idx = components
            .iter()
            .position(|c| matches!(c, std::path::Component::Normal(_)))
            .unwrap_or(0);

        components[start_idx..]
            .iter()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/")
    } else {
        // Case 3: Clean relative path - use as-is
        path.to_str().unwrap_or("").replace('\\', "/") // Normalize to forward slashes
    }
}

/// Checks if a path is a file-relative path (starts with "./" or "../").
///
/// # Arguments
///
/// * `path` - The path to check
///
/// # Returns
///
/// `true` if the path is file-relative, `false` otherwise
pub fn is_file_relative_path(path: &str) -> bool {
    path.starts_with("./") || path.starts_with("../")
}

/// Normalizes a bare filename by removing directory components.
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// The normalized filename
pub fn normalize_bare_filename(path: &str) -> String {
    let path_buf = Path::new(path);
    path_buf.file_name().and_then(|name| name.to_str()).unwrap_or(path).to_string()
}

// ============================================================================
// Installation Path Resolution
// ============================================================================

/// Resolves the installation path for any resource type.
///
/// This is the main entry point for computing where a resource will be installed.
/// It handles both merge-target resources (Hooks, MCP servers) and regular resources
/// (Agents, Commands, Snippets, Scripts).
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type
/// * `source_filename` - The filename/path from the source repository
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported
/// by the specified tool.
///
/// # Errors
///
/// Returns an error if:
/// - The resource type is not supported by the specified tool
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_resolver::resolve_install_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let path = resolve_install_path(
///     &manifest,
///     &dep,
///     "claude-code",
///     ResourceType::Agent,
///     "agents/helper.md"
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn resolve_install_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    source_filename: &str,
) -> Result<String> {
    match resource_type {
        ResourceType::Hook | ResourceType::McpServer => {
            Ok(resolve_merge_target_path(manifest, artifact_type, resource_type))
        }
        _ => resolve_regular_resource_path(
            manifest,
            dep,
            artifact_type,
            resource_type,
            source_filename,
        ),
    }
}

/// Resolves the installation path for merge-target resources (Hook, McpServer).
///
/// These resources are not installed as files but are merged into configuration files.
/// Uses configured merge targets or falls back to hardcoded defaults.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - Must be Hook or McpServer
///
/// # Returns
///
/// The normalized path to the merge target configuration file.
pub fn resolve_merge_target_path(
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> String {
    if let Some(merge_target) = manifest.get_merge_target(artifact_type, resource_type) {
        normalize_path_for_storage(merge_target.display().to_string())
    } else {
        // Fallback to hardcoded defaults if not configured
        match resource_type {
            ResourceType::Hook => ".claude/settings.local.json".to_string(),
            ResourceType::McpServer => {
                if artifact_type == "opencode" {
                    ".opencode/opencode.json".to_string()
                } else {
                    ".mcp.json".to_string()
                }
            }
            _ => unreachable!(
                "resolve_merge_target_path should only be called for Hook or McpServer"
            ),
        }
    }
}

/// Resolves the installation path for regular file-based resources.
///
/// Handles agents, commands, snippets, scripts, and skills by:
/// 1. Getting the base artifact path from tool configuration
/// 2. Applying custom target overrides if specified
/// 3. Computing the relative path based on flatten behavior
/// 4. Avoiding redundant directory prefixes
/// 5. For skills, ensuring directory paths (not file paths)
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, Snippet, Script, Skill)
/// * `source_filename` - The filename/path from the source repository
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported.
///
/// # Errors
///
/// Returns an error if the resource type is not supported by the specified tool.
pub fn resolve_regular_resource_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    source_filename: &str,
) -> Result<String> {
    // Special handling for skills - they are directories, not files
    if resource_type == ResourceType::Skill {
        // For skills, ensure we don't add .md extension
        let skill_name = if let Some(stripped) = source_filename.strip_suffix(".md") {
            stripped
        } else {
            source_filename
        };

        // Get the artifact path for skills
        let artifact_path =
            manifest.get_artifact_resource_path(artifact_type, resource_type).ok_or_else(|| {
                create_unsupported_resource_error(artifact_type, resource_type, dep.get_path())
            })?;

        // Use the same prefix extraction logic as other resources
        // This prevents paths like .claude/skills/skills/rust-helper
        let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);
        let relative_path =
            compute_relative_install_path(&artifact_path.clone(), Path::new(skill_name), flatten);

        let skill_path = artifact_path.join(relative_path);
        Ok(normalize_path_for_storage(normalize_path(&skill_path)))
    } else {
        // Regular file-based resources (agents, commands, snippets, scripts)
        let artifact_path =
            manifest.get_artifact_resource_path(artifact_type, resource_type).ok_or_else(|| {
                create_unsupported_resource_error(artifact_type, resource_type, dep.get_path())
            })?;

        // Compute the final path
        let path = if let Some(custom_target) = dep.get_target() {
            compute_custom_target_path(
                &artifact_path,
                custom_target,
                source_filename,
                dep,
                manifest,
                artifact_type,
                resource_type,
            )
        } else {
            compute_default_path(
                &artifact_path,
                source_filename,
                dep,
                manifest,
                artifact_type,
                resource_type,
            )
        };

        Ok(normalize_path_for_storage(normalize_path(&path)))
    }
}

/// Computes the installation path when a custom target directory is specified.
///
/// Custom targets are relative to the artifact's resource directory. The function
/// uses the original artifact path (not the custom target) for prefix stripping
/// to avoid duplicate directories.
fn compute_custom_target_path(
    artifact_path: &Path,
    custom_target: &str,
    source_filename: &str,
    dep: &ResourceDependency,
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> PathBuf {
    let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);
    let base_target = PathBuf::from(artifact_path.display().to_string())
        .join(custom_target.trim_start_matches('/'));
    // For custom targets, still strip prefix based on the original artifact path
    let relative_path =
        compute_relative_install_path(artifact_path, Path::new(source_filename), flatten);
    base_target.join(relative_path)
}

/// Computes the installation path using the default artifact path.
fn compute_default_path(
    artifact_path: &Path,
    source_filename: &str,
    dep: &ResourceDependency,
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> PathBuf {
    let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);
    let relative_path =
        compute_relative_install_path(artifact_path, Path::new(source_filename), flatten);
    artifact_path.join(relative_path)
}

/// Creates a detailed error message when a resource type is not supported by a tool.
///
/// Provides helpful hints if it looks like a tool name was used as a resource type.
fn create_unsupported_resource_error(
    artifact_type: &str,
    resource_type: ResourceType,
    source_path: &str,
) -> anyhow::Error {
    let base_msg =
        format!("Resource type '{}' is not supported by tool '{}'", resource_type, artifact_type);

    let resource_type_str = resource_type.to_string();
    let hint = if ["claude-code", "opencode", "agpm"].contains(&resource_type_str.as_str()) {
        format!(
            "\n\nIt looks like '{}' is a tool name, not a resource type.\n\
            In transitive dependencies, use resource types (agents, snippets, commands)\n\
            as section headers, then specify 'tool: {}' within each dependency.",
            resource_type_str, resource_type_str
        )
    } else {
        format!(
            "\n\nValid resource types: agent, command, snippet, hook, mcp-server, script\n\
            Source file: {}",
            source_path
        )
    };

    anyhow::anyhow!("{}{}", base_msg, hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pattern_base_path_simple() {
        let (base, pattern) = parse_pattern_base_path("*.md");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_directory() {
        let (base, pattern) = parse_pattern_base_path("agents/*.md");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "agents/*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_parent() {
        let (base, pattern) = parse_pattern_base_path("../foo/*.md");
        assert_eq!(base, PathBuf::from("../foo"));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_current_dir() {
        let (base, pattern) = parse_pattern_base_path("./foo/*.md");
        assert_eq!(base, PathBuf::from("./foo"));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_construct_full_relative_path_current_dir() {
        let base = PathBuf::from(".");
        let matched = Path::new("agents/helper.md");
        let path = construct_full_relative_path(&base, matched);
        assert_eq!(path, "agents/helper.md");
    }

    #[test]
    fn test_construct_full_relative_path_with_base() {
        let base = PathBuf::from("../foo");
        let matched = Path::new("bar.md");
        let path = construct_full_relative_path(&base, matched);
        assert_eq!(path, "../foo/bar.md");
    }

    #[test]
    fn test_extract_pattern_filename_current_dir() {
        let base = PathBuf::from(".");
        let matched = Path::new("agents/helper.md");
        let filename = extract_pattern_filename(&base, matched);
        assert_eq!(filename, "agents/helper.md");
    }
}
