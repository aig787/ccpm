//! Gitignore management utilities for AGPM resources.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::lockfile::{LockFile, LockedResource};
use crate::utils::fs::atomic_write;
use crate::utils::normalize_path_for_storage;

/// Sanitize file paths for user-facing error messages to prevent information disclosure.
///
/// In debug mode, shows full paths for development convenience.
/// In release mode, shows only the filename to prevent exposing system paths.
pub(crate) fn sanitize_path_for_error(path: &Path) -> String {
    // In debug mode, show full paths for development
    if cfg!(debug_assertions) {
        path.display().to_string()
    } else {
        // In release mode, show only filename to prevent information disclosure
        path.file_name().and_then(|name| name.to_str()).unwrap_or("file").to_string()
    }
}

/// Add a single path to .gitignore atomically
///
/// This function adds a single path to the AGPM-managed section of `.gitignore`,
/// ensuring the file is protected from accidental commits even if subsequent
/// operations fail. Thread-safe via mutex locking.
///
/// # Arguments
///
/// * `project_dir` - Project root directory containing `.gitignore`
/// * `path` - Path to add (relative to project root, forward slashes)
/// * `lock` - Mutex to synchronize concurrent gitignore updates
///
/// # Returns
///
/// Returns `Ok(())` if the path was added successfully or was already present.
pub async fn add_path_to_gitignore(
    project_dir: &Path,
    path: &str,
    lock: &Arc<Mutex<()>>,
) -> Result<()> {
    // Acquire lock to ensure thread-safe updates
    let _guard = lock.lock().await;

    let gitignore_path = project_dir.join(".gitignore");

    // Read existing .gitignore content
    let mut before_agpm = Vec::new();
    let mut agpm_paths = std::collections::HashSet::new();
    let mut after_agpm = Vec::new();

    if gitignore_path.exists() {
        let content = tokio::fs::read_to_string(&gitignore_path)
            .await
            .with_context(|| "Failed to read .gitignore file")
            .with_context(|| {
                format!("Failed to read {}", sanitize_path_for_error(&gitignore_path))
            })?;

        let mut in_agpm_section = false;
        let mut past_agpm_section = false;

        for line in content.lines() {
            if line == "# AGPM managed entries - do not edit below this line"
                || line == "# CCPM managed entries - do not edit below this line"
            {
                in_agpm_section = true;
            } else if line == "# End of AGPM managed entries"
                || line == "# End of CCPM managed entries"
            {
                in_agpm_section = false;
                past_agpm_section = true;
            } else if in_agpm_section {
                // Collect existing AGPM paths
                if !line.is_empty() && !line.starts_with('#') {
                    agpm_paths.insert(line.to_string());
                }
            } else if !past_agpm_section {
                before_agpm.push(line.to_string());
            } else {
                after_agpm.push(line.to_string());
            }
        }
    }

    // Add the new path if not already present
    let normalized_path = normalize_path_for_storage(path);
    if agpm_paths.contains(&normalized_path) {
        // Path already exists, no update needed
        return Ok(());
    }
    agpm_paths.insert(normalized_path);

    // Always include private config files
    agpm_paths.insert("agpm.private.toml".to_string());
    agpm_paths.insert("agpm.private.lock".to_string());

    // Build new content
    let mut new_content = String::new();

    // Add header for new files
    if before_agpm.is_empty() && after_agpm.is_empty() {
        new_content.push_str("# .gitignore - AGPM managed entries\n");
        new_content.push_str("# AGPM entries are automatically generated\n");
        new_content.push('\n');
    } else {
        // Preserve content before AGPM section
        for line in &before_agpm {
            new_content.push_str(line);
            new_content.push('\n');
        }
        if !before_agpm.is_empty() && !before_agpm.last().unwrap().trim().is_empty() {
            new_content.push('\n');
        }
    }

    // Add AGPM section
    new_content.push_str("# AGPM managed entries - do not edit below this line\n");
    let mut sorted_paths: Vec<_> = agpm_paths.into_iter().collect();
    sorted_paths.sort();
    for p in sorted_paths {
        new_content.push_str(&p);
        new_content.push('\n');
    }
    new_content.push_str("# End of AGPM managed entries\n");

    // Preserve content after AGPM section
    if !after_agpm.is_empty() {
        new_content.push('\n');
        for line in &after_agpm {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // Write atomically
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| "Failed to update .gitignore file")
        .with_context(|| {
            format!("Failed to update {}", sanitize_path_for_error(&gitignore_path))
        })?;

    Ok(())
}

/// Ensure gitignore state matches the manifest configuration.
///
/// This helper function centralizes gitignore state management logic
/// to reduce code duplication across the codebase.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing gitignore configuration
/// * `lockfile` - The current lockfile containing installed resources
/// * `project_dir` - The project root directory
/// * `lock` - Optional lock for coordinating concurrent gitignore updates. Pass `Some(&lock)`
///   when coordinating with parallel resource installations, or `None` for single-threaded
///   contexts.
///
/// # Behavior
///
/// - If `manifest.gitignore` is true: Updates .gitignore with all installed paths
/// - If `manifest.gitignore` is false: Removes all AGPM-managed entries from .gitignore
///
/// # Errors
///
/// Returns an error if:
/// - Gitignore file cannot be read or written
/// - Lockfile processing fails
/// - File system permissions prevent gitignore operations
pub async fn ensure_gitignore_state(
    manifest: &crate::manifest::Manifest,
    lockfile: &crate::lockfile::LockFile,
    project_dir: &std::path::Path,
    lock: Option<&std::sync::Arc<tokio::sync::Mutex<()>>>,
) -> anyhow::Result<()> {
    if manifest.gitignore {
        update_gitignore(lockfile, project_dir, true, lock)?;
    } else {
        // Clean up any existing AGPM entries
        cleanup_gitignore(project_dir, lock).await?;
    }
    Ok(())
}

/// Update .gitignore with all installed resource file paths.
///
/// This function updates the project's `.gitignore` file to include all resources
/// that are installed by AGPM, preventing accidental commits of managed files.
/// It preserves existing user entries while managing the AGPM section automatically.
///
/// # AGPM Section Management
///
/// The function maintains a dedicated section in `.gitignore`:
/// ```text
/// # AGPM managed entries - do not edit below this line
/// .claude/agents/example.md
/// .claude/snippets/shared.md
/// agpm.private.toml
/// agpm.private.lock
/// # End of AGPM managed entries
/// ```
///
/// # Arguments
///
/// * `lockfile` - The lockfile containing all installed resources and their paths
/// * `project_dir` - The project root directory containing the `.gitignore` file
/// * `enabled` - Whether gitignore management is enabled (can be disabled via config)
/// * `_lock` - Lock parameter for API consistency (unused). This function uses
///   [`atomic_write()`](crate::utils::fs::atomic_write) for filesystem-level atomicity,
///   which provides sufficient safety for batch gitignore updates. Individual path
///   additions via [`add_path_to_gitignore()`] acquire the lock for concurrent safety
///   during incremental updates.
///
/// # Behavior
///
/// - **Creates new file**: If no `.gitignore` exists, creates one with AGPM section
/// - **Updates existing file**: Preserves user content, adds/replaces AGPM section
/// - **No-op when disabled**: Returns early if gitignore management is disabled
/// - **Always included**: Private config files (`agpm.private.toml`, `agpm.private.lock`)
/// - **Resource types**: Includes agents, snippets, commands, and scripts
/// - **Excludes**: Hooks and MCP servers (configuration only, not installed as files)
///
/// # Preservation of User Content
///
/// The function preserves all non-AGPM content:
/// - Existing entries before AGPM section are kept unchanged
/// - Existing entries after AGPM section are kept unchanged
/// - Only the managed section between the markers is replaced
///
/// # Migration Support
///
/// Supports migration from legacy CCPM (Claude Code Package Manager):
/// - Recognizes both `# CCPM managed entries` and `# AGPM managed entries`
/// - Automatically converts to AGPM format on update
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be read due to permissions
/// - File cannot be written due to permissions or disk space
/// - Project directory doesn't exist
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::installer::update_gitignore;
/// use agpm_cli::lockfile::LockFile;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let project_dir = Path::new(".");
///
/// // Update .gitignore with all installed resources
/// update_gitignore(&lockfile, project_dir, true, None)?;
///
/// println!("Gitignore updated successfully");
/// # Ok(())
/// # }
/// ```
pub fn update_gitignore(
    lockfile: &LockFile,
    project_dir: &Path,
    enabled: bool,
    _lock: Option<&std::sync::Arc<tokio::sync::Mutex<()>>>,
) -> Result<()> {
    if !enabled {
        // Gitignore management is disabled
        return Ok(());
    }

    let gitignore_path = project_dir.join(".gitignore");

    // Collect all installed file paths relative to project root
    let mut paths_to_ignore = HashSet::new();

    // Helper to add paths from a resource list
    let mut add_resource_paths = |resources: &[LockedResource]| {
        for resource in resources {
            // Skip resources with install=false (they're not written to disk)
            if resource.install == Some(false) {
                continue;
            }
            if !resource.installed_at.is_empty() {
                // Use the explicit installed_at path
                paths_to_ignore.insert(resource.installed_at.clone());
            }
        }
    };

    // Collect paths from all resource types
    // Skip hooks and MCP servers - they are configured only, not installed as files
    add_resource_paths(&lockfile.agents);
    add_resource_paths(&lockfile.snippets);
    add_resource_paths(&lockfile.commands);
    add_resource_paths(&lockfile.scripts);

    // Always include private config files
    paths_to_ignore.insert("agpm.private.toml".to_string());
    paths_to_ignore.insert("agpm.private.lock".to_string());

    // Read existing gitignore if it exists
    let mut before_agpm_section = Vec::new();
    let mut after_agpm_section = Vec::new();

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .with_context(|| "Failed to read .gitignore file")
            .with_context(|| {
                format!("Failed to read {}", sanitize_path_for_error(&gitignore_path))
            })?;

        let mut in_agpm_section = false;
        let mut past_agpm_section = false;

        for line in content.lines() {
            // Support both AGPM and legacy CCPM markers for migration compatibility
            if line == "# AGPM managed entries - do not edit below this line"
                || line == "# CCPM managed entries - do not edit below this line"
            {
                in_agpm_section = true;
                continue;
            } else if line == "# End of AGPM managed entries"
                || line == "# End of CCPM managed entries"
            {
                in_agpm_section = false;
                past_agpm_section = true;
                continue;
            }

            if !in_agpm_section && !past_agpm_section {
                // Preserve everything before AGPM section exactly as-is
                before_agpm_section.push(line.to_string());
            } else if in_agpm_section {
                // Skip existing AGPM/CCPM entries (they'll be replaced)
                // Continue to next line
            } else {
                // Preserve everything after AGPM section exactly as-is
                after_agpm_section.push(line.to_string());
            }
        }
    }

    // Build the new content
    let mut new_content = String::new();

    // Add everything before AGPM section exactly as it was
    // Build initial content efficiently
    if !before_agpm_section.is_empty() {
        // Add all lines before AGPM section at once
        new_content.push_str(&before_agpm_section.join("\n"));
        new_content.push('\n');
        // Add blank line before AGPM section if the previous content doesn't end with one
        if !before_agpm_section.last().unwrap().trim().is_empty() {
            new_content.push('\n');
        }
    }

    // Add AGPM managed section
    new_content.push_str("# AGPM managed entries - do not edit below this line\n");

    // Convert paths to gitignore format (relative to project root)
    // Sort paths for consistent output
    let mut sorted_paths: Vec<_> = paths_to_ignore.into_iter().collect();
    sorted_paths.sort();

    // Collect normalized paths efficiently
    let mut path_lines: Vec<String> = Vec::with_capacity(sorted_paths.len());
    for path in &sorted_paths {
        // Use paths as-is since gitignore is now at project root
        let ignore_path = if path.starts_with("./") {
            // Remove leading ./ if present
            path.strip_prefix("./").unwrap_or(path)
        } else {
            path
        };

        // Normalize to forward slashes for .gitignore (Git expects forward slashes on all platforms)
        let normalized_path = normalize_path_for_storage(ignore_path);
        path_lines.push(normalized_path);
    }

    // Add all paths at once
    new_content.push_str(&path_lines.join("\n"));
    new_content.push('\n');
    new_content.push_str("# End of AGPM managed entries\n");

    // Add everything after AGPM section exactly as it was
    if !after_agpm_section.is_empty() {
        new_content.push('\n');
        // Add all lines after AGPM section at once
        new_content.push_str(&after_agpm_section.join("\n"));
        new_content.push('\n');
    }

    // If this is a new file, add a basic header
    if before_agpm_section.is_empty() && after_agpm_section.is_empty() {
        // Reuse the already collected path_lines
        let header_lines = [
            "# .gitignore - AGPM managed entries",
            "# AGPM entries are automatically generated",
            "",
            "# AGPM managed entries - do not edit below this line",
        ];
        let footer_lines = ["# End of AGPM managed entries"];

        // Combine all sections efficiently
        let all_lines: Vec<String> = header_lines
            .iter()
            .map(|&s| s.to_string())
            .chain(path_lines)
            .chain(footer_lines.iter().map(|&s| s.to_string()))
            .collect();
        new_content = all_lines.join("\n");
    }

    // Write the updated gitignore
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| "Failed to update .gitignore file")
        .with_context(|| {
            format!("Failed to update {}", sanitize_path_for_error(&gitignore_path))
        })?;

    Ok(())
}

/// Ensure gitignore state matches the manifest configuration.
///
/// This helper function centralizes gitignore state management logic
/// to reduce code duplication across the codebase.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing gitignore configuration
/// * `lockfile` - The current lockfile containing installed resources
/// * `project_dir` - The project root directory
/// * `lock` - Optional lock for coordinating concurrent gitignore updates. Pass `Some(&lock)`
///   when coordinating with parallel resource installations, or `None` for single-threaded
///   contexts.
///
/// # Behavior
///
/// - If `manifest.gitignore` is true: Updates .gitignore with all installed paths
/// - If `manifest.gitignore` is false: Removes all AGPM-managed entries from .gitignore
///
/// # Errors
///
/// Returns an error if:
/// - Gitignore file cannot be read or written
/// - Lockfile processing fails
/// - File system permissions prevent gitignore operations
///
/// Remove AGPM managed entries from .gitignore.
///
/// This function removes the AGPM-managed section from .gitignore,
/// preserving all user content. This is used when gitignore management
/// is disabled in the manifest.
///
/// # Arguments
///
/// * `project_dir` - Project root directory containing `.gitignore`
/// * `_lock` - Lock parameter for API consistency (unused). This function uses
///   [`atomic_write()`](crate::utils::fs::atomic_write) for filesystem-level atomicity.
///   Since cleanup operations are typically performed in single-threaded finalization
///   contexts (not during parallel resource installation), the lock is not required.
///
/// # Behavior
///
/// - **Preserves user content**: All content outside AGPM section is kept
/// - **Removes AGPM section**: Deletes everything between the markers
/// - **Handles migration**: Supports both AGPM and CCPM markers
/// - **Cleans up empty files**: Deletes .gitignore if it becomes empty
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be read due to permissions
/// - File cannot be written due to permissions or disk space
/// - Project directory doesn't exist
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::installer::cleanup_gitignore;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// cleanup_gitignore(Path::new("."), None).await?;
/// println!("AGPM entries removed from .gitignore");
/// # Ok(())
/// # }
/// ```
pub async fn cleanup_gitignore(
    project_dir: &Path,
    _lock: Option<&std::sync::Arc<tokio::sync::Mutex<()>>>,
) -> Result<()> {
    let gitignore_path = project_dir.join(".gitignore");

    // Attempt direct read and handle ENOENT gracefully to prevent TOCTOU race condition
    let content = match tokio::fs::read_to_string(&gitignore_path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist, nothing to clean up
            return Ok(());
        }
        Err(e) => {
            return Err(e).with_context(|| "Failed to read .gitignore file").with_context(|| {
                format!("Failed to read {}", sanitize_path_for_error(&gitignore_path))
            });
        }
    };

    // Parse content and remove AGPM managed section
    let mut before_agpm = Vec::new();
    let mut after_agpm = Vec::new();
    let mut in_agpm_section = false;
    let mut past_agpm_section = false;

    for line in content.lines() {
        if line == "# AGPM managed entries - do not edit below this line"
            || line == "# CCPM managed entries - do not edit below this line"
        {
            in_agpm_section = true;
            continue;
        } else if line == "# End of AGPM managed entries" || line == "# End of CCPM managed entries"
        {
            in_agpm_section = false;
            past_agpm_section = true;
            continue;
        }

        if !in_agpm_section && !past_agpm_section {
            // Content before AGPM section - keep it
            before_agpm.push(line);
        } else if in_agpm_section {
            // Inside AGPM section - skip it
            continue;
        } else if past_agpm_section {
            // Content after AGPM section - keep it
            after_agpm.push(line);
        }
    }

    // Build new content without AGPM section efficiently
    let mut lines = Vec::new();

    // Add content before AGPM section
    if !before_agpm.is_empty() {
        lines.extend_from_slice(&before_agpm);
    }

    // Add content after AGPM section
    if !after_agpm.is_empty() {
        // Add blank line if there's content before and after
        if !before_agpm.is_empty() {
            lines.push("");
        }
        lines.extend_from_slice(&after_agpm);
    }

    // Join with single allocation
    let mut new_content = lines.join("\n");

    // Trim trailing newlines
    new_content = new_content.trim_end().to_string();

    // If the file would be empty, delete it
    if new_content.is_empty() {
        match tokio::fs::remove_file(&gitignore_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File already deleted (race condition), nothing to do
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| "Failed to remove .gitignore file")
                    .with_context(|| {
                        format!("Failed to remove {}", sanitize_path_for_error(&gitignore_path))
                    });
            }
        }
        return Ok(());
    }

    // Write the cleaned content back
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| "Failed to update .gitignore file")
        .with_context(|| {
            format!("Failed to update {}", sanitize_path_for_error(&gitignore_path))
        })?;

    Ok(())
}
