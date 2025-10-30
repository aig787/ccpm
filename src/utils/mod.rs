//! Cross-platform utilities and helpers
//!
//! This module provides utility functions for file operations, platform-specific
//! code, and user interface elements like progress bars. All utilities are designed
//! to work consistently across Windows, macOS, and Linux.
//!
//! # Modules
//!
//! - [`fs`] - File system operations with atomic writes and safe copying
//! - [`manifest_utils`] - Utilities for loading and validating manifests
//! - [`platform`] - Platform-specific helpers and path resolution
//! - [`progress`] - Multi-phase progress tracking for long-running operations
//!
//! # Cross-Platform Considerations
//!
//! All utilities handle platform differences:
//! - Path separators (`/` vs `\`)
//! - Line endings (`\n` vs `\r\n`)
//! - File permissions and attributes
//! - Shell commands and environment variables
//!
//! # Example
//!
//! ```rust,no_run
//! use agpm_cli::utils::{ensure_dir, atomic_write, MultiPhaseProgress, InstallationPhase};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Ensure directory exists
//! ensure_dir(Path::new("output/agents"))?;
//!
//! // Write file atomically
//! atomic_write(Path::new("output/config.toml"), b"content")?;
//!
//! // Show progress with phases
//! let progress = MultiPhaseProgress::new(true);
//! progress.start_phase(InstallationPhase::Installing, Some("Processing files"));
//! # Ok(())
//! # }
//! ```

use anyhow::Context;
use std::path::{Path, PathBuf};

pub mod fs;
pub mod manifest_utils;
pub mod path_validation;
pub mod platform;
pub mod progress;
pub mod security;

pub use fs::{
    atomic_write, compare_file_times, copy_dir, create_temp_file, ensure_dir,
    file_exists_and_readable, get_modified_time, normalize_path, read_json_file, read_text_file,
    read_toml_file, read_yaml_file, safe_write, write_json_file, write_text_file, write_toml_file,
    write_yaml_file,
};
pub use manifest_utils::{
    load_and_validate_manifest, load_project_manifest, manifest_exists, manifest_path,
};
pub use path_validation::{
    ensure_directory_exists, ensure_within_directory, find_project_root, safe_canonicalize,
    safe_relative_path, sanitize_file_name, validate_no_traversal, validate_project_path,
    validate_resource_path,
};
pub use platform::{
    compute_relative_install_path, get_git_command, get_home_dir, is_windows,
    normalize_path_for_storage, resolve_path,
};
pub use progress::{InstallationPhase, MultiPhaseProgress, ProgressBar, collect_dependency_names};

/// Canonicalize JSON for deterministic hashing.
///
/// Uses `serde_json` with `preserve_order` feature to ensure
/// consistent key ordering across serialization calls. This is
/// critical for generating stable checksums of template contexts.
///
/// # Arguments
///
/// * `value` - The JSON value to canonicalize
///
/// # Returns
///
/// A deterministic string representation of the JSON value
///
/// # Errors
///
/// Returns an error if the JSON value cannot be serialized (should be rare
/// for valid `serde_json::Value` instances).
pub fn canonicalize_json(value: &serde_json::Value) -> anyhow::Result<String> {
    serialize_json_canonically(value)
}

/// SHA-256 hash of an empty JSON object `{}`.
/// This is the default hash when there are no template variables.
/// Computed lazily to ensure consistency with the hash function.
pub static EMPTY_VARIANT_INPUTS_HASH: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| {
        compute_variant_inputs_hash(&serde_json::json!({}))
            .expect("Failed to compute hash of empty JSON object")
    });

/// Compute SHA-256 hash of variant_inputs JSON value.
///
/// This is the **single source of truth** for computing `variant_inputs_hash` values.
/// It ensures consistent hashing by serializing the JSON value and hashing the result.
///
/// This function MUST be used everywhere `variant_inputs_hash` is computed to ensure
/// that identity comparisons work correctly across the codebase.
///
/// # Arguments
///
/// * `variant_inputs` - The variant_inputs JSON value to hash
///
/// # Returns
///
/// A string in the format "sha256:hexdigest"
///
/// # Errors
///
/// Returns an error if the JSON value cannot be serialized.
///
/// # Example
///
/// ```rust,no_run
/// use agpm_cli::utils::{compute_variant_inputs_hash, EMPTY_VARIANT_INPUTS_HASH};
/// use serde_json::json;
///
/// let hash = compute_variant_inputs_hash(&json!({})).unwrap();
/// assert_eq!(hash, *EMPTY_VARIANT_INPUTS_HASH);
/// ```
pub fn compute_variant_inputs_hash(variant_inputs: &serde_json::Value) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};

    // Serialize with sorted keys for deterministic hashing
    // This ensures {"a": 1, "b": 2} and {"b": 2, "a": 1} have the same hash
    let serialized = serialize_json_canonically(variant_inputs)?;

    // Hash the serialized version
    let hash_result = Sha256::digest(serialized.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hash_result)))
}

/// Serialize JSON value with sorted keys for deterministic output.
///
/// This ensures that semantically identical JSON objects produce identical strings,
/// regardless of key insertion order.
fn serialize_json_canonically(value: &serde_json::Value) -> anyhow::Result<String> {
    match value {
        serde_json::Value::Object(map) => {
            // Sort keys alphabetically for determinism
            let mut sorted_keys: Vec<_> = map.keys().collect();
            sorted_keys.sort();

            let mut result = String::from("{");
            for (i, key) in sorted_keys.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                // Serialize key
                result.push_str(&serde_json::to_string(key)?);
                result.push(':');
                // Recursively serialize value
                let val = map.get(*key).unwrap();
                result.push_str(&serialize_json_canonically(val)?);
            }
            result.push('}');
            Ok(result)
        }
        serde_json::Value::Array(arr) => {
            // Arrays: serialize elements in order
            let mut result = String::from("[");
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                result.push_str(&serialize_json_canonically(item)?);
            }
            result.push(']');
            Ok(result)
        }
        // Primitives: use standard serialization
        _ => serde_json::to_string(value).context("Failed to serialize JSON value"),
    }
}

/// Generates a backup path for tool configuration files.
///
/// Creates backup paths in the format: `.agpm/backups/<tool>/<filename>`
/// at the project root level, not inside tool-specific directories.
///
/// # Arguments
///
/// * `config_path` - Path to the configuration file being backed up
/// * `tool_name` - Name of the tool (e.g., "claude-code", "opencode")
///
/// # Returns
///
/// Full path to the backup file at project root level
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use agpm_cli::utils::generate_backup_path;
///
/// // For .claude/settings.local.json with claude-code tool
/// let backup_path = generate_backup_path(
///     Path::new("/project/.claude/settings.local.json"),
///     "claude-code"
/// );
/// // Returns: /project/.agpm/backups/claude-code/settings.local.json
///
/// // For .mcp.json with claude-code tool  
/// let backup_path = generate_backup_path(
///     Path::new("/project/.mcp.json"),
///     "claude-code"
/// );
/// // Returns: /project/.agpm/backups/claude-code/.mcp.json
/// ```
pub fn generate_backup_path(config_path: &Path, tool_name: &str) -> anyhow::Result<PathBuf> {
    use anyhow::{Context, anyhow};

    // Find project root by looking for agpm.toml
    let project_root = find_project_root(config_path)
        .with_context(|| format!("Failed to find project root from: {}", config_path.display()))?;

    // Create backup path: .agpm/backups/<tool>/<filename>
    let backup_dir = project_root.join(".agpm").join("backups").join(tool_name);

    // Get just the filename from the original config path
    let filename = config_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid config path: {}", config_path.display()))?;

    Ok(backup_dir.join(filename))
}

/// Determines if a given URL/path is a local filesystem path (not a Git repository URL).
///
/// Local paths are directories on the filesystem that are directly accessible,
/// as opposed to Git repository URLs that need to be cloned/fetched.
///
/// # Examples
///
/// ```
/// use agpm_cli::utils::is_local_path;
///
/// // Unix-style paths
/// assert!(is_local_path("/absolute/path"));
/// assert!(is_local_path("./relative/path"));
/// assert!(is_local_path("../parent/path"));
///
/// // Windows-style paths (with drive letters or UNC)
/// assert!(is_local_path("C:/Users/path"));
/// assert!(is_local_path("C:\\Users\\path"));
/// assert!(is_local_path("//server/share"));
/// assert!(is_local_path("\\\\server\\share"));
///
/// // Git URLs (not local paths)
/// assert!(!is_local_path("https://github.com/user/repo.git"));
/// assert!(!is_local_path("git@github.com:user/repo.git"));
/// assert!(!is_local_path("file:///path/to/repo.git"));
/// ```
#[must_use]
pub fn is_local_path(url: &str) -> bool {
    // file:// URLs are Git repository URLs, not local paths
    if url.starts_with("file://") {
        return false;
    }

    // Unix-style absolute or relative paths
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
        return true;
    }

    // Windows-style paths
    // Check for drive letter (e.g., C:/ or C:\)
    if url.len() >= 2 {
        let chars: Vec<char> = url.chars().collect();
        if chars[0].is_ascii_alphabetic() && chars[1] == ':' {
            return true;
        }
    }

    // Check for UNC paths (e.g., //server/share or \\server\share)
    if url.starts_with("//") || url.starts_with("\\\\") {
        return true;
    }

    false
}

/// Determines if a given URL is a Git repository URL (including file:// URLs).
///
/// Git repository URLs need to be cloned/fetched, unlike local filesystem paths.
///
/// # Examples
///
/// ```
/// use agpm_cli::utils::is_git_url;
///
/// assert!(is_git_url("https://github.com/user/repo.git"));
/// assert!(is_git_url("git@github.com:user/repo.git"));
/// assert!(is_git_url("file:///path/to/repo.git"));
/// assert!(is_git_url("ssh://git@server.com/repo.git"));
/// assert!(!is_git_url("/absolute/path"));
/// assert!(!is_git_url("./relative/path"));
/// ```
#[must_use]
pub fn is_git_url(url: &str) -> bool {
    !is_local_path(url)
}

/// Resolves a file-relative path from a transitive dependency.
///
/// This function resolves paths that start with `./` or `../` relative to the
/// directory containing the parent resource file. This provides a unified way to
/// resolve transitive dependencies for both Git-backed and path-only resources.
///
/// # Arguments
///
/// * `parent_file_path` - Absolute path to the file declaring the dependency
/// * `relative_path` - Path from the transitive dep spec (must start with `./` or `../`)
///
/// # Returns
///
/// Canonical absolute path to the dependency.
///
/// # Errors
///
/// Returns an error if:
/// - `relative_path` doesn't start with `./` or `../`
/// - The resolved path doesn't exist
/// - Canonicalization fails
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::resolve_file_relative_path;
///
/// let parent = Path::new("/project/agents/helper.md");
/// let resolved = resolve_file_relative_path(parent, "./snippets/utils.md")?;
/// // Returns: /project/agents/snippets/utils.md
///
/// let resolved = resolve_file_relative_path(parent, "../common/base.md")?;
/// // Returns: /project/common/base.md
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_file_relative_path(
    parent_file_path: &std::path::Path,
    relative_path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::{Context, anyhow};

    // Validate it's a file-relative path (allow ./, ../, or simple relative paths)
    // We allow simple relative paths to support skill dependencies like "snippets/utils.md"
    let is_relative = relative_path.starts_with("./")
        || relative_path.starts_with("../")
        || (!relative_path.starts_with('/') && !relative_path.contains(":"));

    if !is_relative {
        return Err(anyhow!(
            "Transitive dependency path must be relative (not absolute or URL): {}",
            relative_path
        ));
    }

    // Get parent directory
    let parent_dir = parent_file_path
        .parent()
        .ok_or_else(|| anyhow!("Parent file has no directory: {}", parent_file_path.display()))?;

    // Resolve relative to parent's directory
    let resolved = parent_dir.join(relative_path);

    // Canonicalize the final path
    resolved.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize resolved path: {} -> {}",
            parent_file_path.display(),
            resolved.display()
        )
    })
}

/// Resolves a path relative to the manifest directory.
///
/// This function handles shell expansion and both relative and absolute paths,
/// resolving them relative to the directory containing the manifest file.
///
/// # Arguments
///
/// * `manifest_dir` - The directory containing the agpm.toml manifest
/// * `rel_path` - The path to resolve (can be relative or absolute)
///
/// # Returns
///
/// Canonical absolute path to the resource.
///
/// # Errors
///
/// Returns an error if:
/// - Shell expansion fails
/// - The path doesn't exist
/// - Canonicalization fails
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::resolve_path_relative_to_manifest;
///
/// let manifest_dir = Path::new("/project");
/// let resolved = resolve_path_relative_to_manifest(manifest_dir, "../shared/agents/helper.md")?;
/// // Returns: /shared/agents/helper.md
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_path_relative_to_manifest(
    manifest_dir: &std::path::Path,
    rel_path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;

    let expanded = shellexpand::full(rel_path)
        .with_context(|| format!("Failed to expand path: {}", rel_path))?;
    let path = std::path::PathBuf::from(expanded.as_ref());

    let resolved = if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    };

    resolved.canonicalize().with_context(|| {
        format!(
            "Path does not exist: {} (resolved from manifest dir '{}')",
            resolved.display(),
            manifest_dir.display()
        )
    })
}

/// Computes a relative path from a base directory to a target path.
///
/// This function handles paths both inside and outside the base directory,
/// using `../` notation when the target is outside. Both paths should be
/// absolute and canonicalized for correct results.
///
/// This is critical for lockfile portability - we must store manifest-relative
/// paths even when they go outside the project with `../`.
///
/// # Arguments
///
/// * `base` - The base directory (should be absolute and canonicalized)
/// * `target` - The target path (should be absolute and canonicalized)
///
/// # Returns
///
/// A relative path from base to target, using `../` notation if needed.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::compute_relative_path;
///
/// let base = Path::new("/project");
/// let target = Path::new("/project/agents/helper.md");
/// let relative = compute_relative_path(base, target);
/// // Returns: "agents/helper.md"
///
/// let target_outside = Path::new("/shared/utils.md");
/// let relative = compute_relative_path(base, target_outside);
/// // Returns: "../shared/utils.md"
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compute_relative_path(base: &std::path::Path, target: &std::path::Path) -> String {
    use std::path::Component;

    // Try simple strip_prefix first (common case: target inside base)
    if let Ok(relative) = target.strip_prefix(base) {
        // Normalize to forward slashes for cross-platform storage
        return normalize_path_for_storage(relative);
    }

    // Target is outside base - need to compute path with ../
    let base_components: Vec<_> = base.components().collect();
    let target_components: Vec<_> = target.components().collect();

    // Find the common prefix
    let mut common_prefix_len = 0;
    for (b, t) in base_components.iter().zip(target_components.iter()) {
        if b == t {
            common_prefix_len += 1;
        } else {
            break;
        }
    }

    // Use slices instead of drain for better performance (avoid reallocation)
    let base_remainder = &base_components[common_prefix_len..];
    let target_remainder = &target_components[common_prefix_len..];

    // Build the relative path
    let mut result = std::path::PathBuf::new();

    // Add ../ for each remaining base component
    for _ in base_remainder {
        result.push("..");
    }

    // Add the remaining target components
    for component in target_remainder {
        if let Component::Normal(c) = component {
            result.push(c);
        }
    }

    // Normalize to forward slashes for cross-platform storage
    normalize_path_for_storage(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_compute_relative_path_inside_base() {
        // Target inside base directory
        let base = Path::new("/project");
        let target = Path::new("/project/agents/helper.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_compute_relative_path_outside_base() {
        // Target outside base directory (sibling)
        let base = Path::new("/project");
        let target = Path::new("/shared/utils.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../shared/utils.md");
    }

    #[test]
    fn test_compute_relative_path_multiple_levels_up() {
        // Target multiple levels up
        let base = Path::new("/project/subdir");
        let target = Path::new("/other/file.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../../other/file.md");
    }

    #[test]
    fn test_compute_relative_path_same_directory() {
        // Base and target are the same
        let base = Path::new("/project");
        let target = Path::new("/project");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "");
    }

    #[test]
    fn test_compute_relative_path_nested() {
        // Complex nesting
        let base = Path::new("/a/b/c");
        let target = Path::new("/a/d/e/f.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../../d/e/f.md");
    }
}

#[cfg(test)]
mod backup_path_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_generate_backup_path() {
        use crate::utils::platform::normalize_path_for_storage;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create agpm.toml to establish project root
        fs::write(project_root.join("agpm.toml"), "[sources]\n").unwrap();

        // Create a config file
        let config_dir = project_root.join(".claude");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("settings.local.json");
        fs::write(&config_path, "{}").unwrap();

        let backup_path = generate_backup_path(&config_path, "claude-code").unwrap();

        // Convert both to absolute paths for comparison
        let project_root = std::fs::canonicalize(project_root).unwrap();
        assert!(backup_path.starts_with(project_root));

        // Use normalized path for cross-platform comparison
        let normalized_backup = normalize_path_for_storage(&backup_path);
        assert!(normalized_backup.contains(".agpm/backups/claude-code/"));
        assert!(normalized_backup.ends_with("settings.local.json"));
    }

    #[test]
    fn test_generate_backup_path_fails_when_no_project_root() {
        let temp_dir = TempDir::new().unwrap();

        // Create config file WITHOUT agpm.toml in any parent directory
        let config_path = temp_dir.path().join("orphan-config.json");
        fs::write(&config_path, "{}").unwrap();

        let result = generate_backup_path(&config_path, "claude-code");

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to find project root"));
        // The actual error from find_project_root mentions "No agpm.toml found"
        assert!(error_msg.contains("agpm.toml") || error_msg.contains("project root"));
    }

    #[test]
    fn test_generate_backup_path_with_nested_config() {
        use crate::utils::platform::normalize_path_for_storage;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::write(project_root.join("agpm.toml"), "[sources]\n").unwrap();

        // Config in nested directory
        let config_path = project_root.join(".claude/subdir/settings.local.json");
        let backup_path = generate_backup_path(&config_path, "claude-code").unwrap();

        // Backup should still go to project root, not relative to config
        let project_root = std::fs::canonicalize(project_root).unwrap();
        assert!(backup_path.starts_with(project_root));

        // Use normalized path for cross-platform comparison
        let normalized_backup = normalize_path_for_storage(&backup_path);
        assert!(normalized_backup.contains(".agpm/backups/claude-code/"));
        assert!(normalized_backup.ends_with("settings.local.json"));

        // Should NOT include "subdir" in backup path
        assert!(!normalized_backup.contains("subdir"));
    }

    #[test]
    fn test_generate_backup_path_different_tools() {
        use crate::utils::platform::normalize_path_for_storage;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::write(project_root.join("agpm.toml"), "[sources]\n").unwrap();

        let config_path = project_root.join(".mcp.json");

        // Test different tools
        let claude_backup = generate_backup_path(&config_path, "claude-code").unwrap();
        let open_backup = generate_backup_path(&config_path, "opencode").unwrap();
        let custom_backup = generate_backup_path(&config_path, "my-tool").unwrap();

        // Use normalized paths for cross-platform comparison
        let normalized_claude = normalize_path_for_storage(&claude_backup);
        let normalized_open = normalize_path_for_storage(&open_backup);
        let normalized_custom = normalize_path_for_storage(&custom_backup);

        assert!(normalized_claude.contains(".agpm/backups/claude-code/"));
        assert!(normalized_open.contains(".agpm/backups/opencode/"));
        assert!(normalized_custom.contains(".agpm/backups/my-tool/"));

        // All should end with same filename
        assert!(normalized_claude.ends_with("mcp.json"));
        assert!(normalized_open.ends_with("mcp.json"));
        assert!(normalized_custom.ends_with("mcp.json"));
    }

    #[test]
    fn test_generate_backup_path_invalid_config_path() {
        // Test with path that has no filename (root directory)
        let invalid_path = Path::new("/");
        let result = generate_backup_path(invalid_path, "claude-code");

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        // The function checks project root first, then filename
        assert!(
            error_msg.contains("Failed to find project root")
                || error_msg.contains("Invalid config path")
        );
    }

    #[test]
    fn test_backup_path_normalization_cross_platform() {
        use crate::utils::platform::normalize_path_for_storage;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("agpm.toml"), "[sources]\n").unwrap();

        // Test both Unix and Windows style paths (should work on any platform)
        let unix_style = temp_dir.path().join(".claude").join("settings.local.json");
        let direct_path = temp_dir.path().join(".claude/settings.local.json");

        let backup1 = generate_backup_path(&unix_style, "claude-code").unwrap();
        let backup2 = generate_backup_path(&direct_path, "claude-code").unwrap();

        // Both should produce the same normalized backup path
        assert_eq!(backup1, backup2);

        // Verify structure using normalized path (cross-platform)
        let backup_normalized = normalize_path_for_storage(&backup1);
        assert!(backup_normalized.contains(".agpm/backups/claude-code/settings.local.json"));
    }

    #[test]
    #[cfg(unix)]
    fn test_backup_with_symlinked_config() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::write(project_root.join("agpm.toml"), "[sources]\n").unwrap();

        // Create real config file
        let real_config = project_root.join("real-settings.json");
        fs::write(&real_config, r#"{"test": "value"}"#).unwrap();

        // Create directory for symlink
        fs::create_dir_all(project_root.join(".claude")).unwrap();

        // Create symlink
        let symlink_config = project_root.join(".claude/settings.local.json");
        symlink(&real_config, &symlink_config).unwrap();

        let backup_path = generate_backup_path(&symlink_config, "claude-code").unwrap();

        // Convert to absolute path for comparison
        let project_root = std::fs::canonicalize(project_root).unwrap();
        assert!(backup_path.starts_with(project_root));
        assert!(backup_path.to_str().unwrap().contains(".agpm/backups/claude-code/"));
        assert!(backup_path.to_str().unwrap().ends_with("settings.local.json"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_backup_with_long_windows_path() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::write(project_root.join("agpm.toml"), "[sources]\n").unwrap();

        // Create a very long path (Windows has 260 char limit traditionally)
        let mut long_path = project_root.to_path_buf();
        for _ in 0..10 {
            long_path = long_path.join("very_long_directory_name_that_might_cause_issues");
        }
        fs::create_dir_all(&long_path).unwrap();

        let config_path = long_path.join("settings.local.json");
        fs::write(&config_path, "{}").unwrap();

        let result = generate_backup_path(&config_path, "claude-code");

        // Should either succeed or give a clear error about path length
        match result {
            Ok(backup_path) => {
                // generate_backup_path uses find_project_root which canonicalizes,
                // so we need to canonicalize project_root for comparison
                let canonical_root = std::fs::canonicalize(project_root)
                    .unwrap_or_else(|_| project_root.to_path_buf());
                assert!(backup_path.starts_with(&canonical_root));
            }
            Err(err) => {
                // Should give a meaningful error, not just "path too long"
                let error_msg = err.to_string();
                assert!(error_msg.len() > 10);
            }
        }
    }
}
