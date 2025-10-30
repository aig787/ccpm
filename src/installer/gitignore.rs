//! Gitignore management utilities for AGPM resources.

use crate::core::ResourceTypeExt;
use crate::lockfile::{LockFile, LockedResource};
use crate::utils::fs::atomic_write;
use crate::utils::normalize_path_for_storage;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

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
            .with_context(|| format!("Failed to read {}", gitignore_path.display()))?;

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
        .with_context(|| format!("Failed to update {}", gitignore_path.display()))?;

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
/// update_gitignore(&lockfile, project_dir, true)?;
///
/// println!("Gitignore updated successfully");
/// # Ok(())
/// # }
/// ```
pub fn update_gitignore(lockfile: &LockFile, project_dir: &Path, enabled: bool) -> Result<()> {
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

    // Collect paths from all resource types using the resource iterator
    // Skip hooks and MCP servers - they are configured only, not installed as files
    use crate::core::ResourceType;
    for resource_type in ResourceType::all() {
        match resource_type {
            ResourceType::Hook | ResourceType::McpServer => {
                // Skip these types as they don't install files to disk
                continue;
            }
            _ => {
                // Add paths from all other resource types (agents, snippets, commands, scripts, skills)
                let resources = resource_type.get_lockfile_entries(lockfile);
                add_resource_paths(resources);
            }
        }
    }

    // Read existing gitignore if it exists
    let mut before_agpm_section = Vec::new();
    let mut after_agpm_section = Vec::new();

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .with_context(|| format!("Failed to read {}", gitignore_path.display()))?;

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
    if !before_agpm_section.is_empty() {
        for line in &before_agpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
        // Add blank line before AGPM section if the previous content doesn't end with one
        if !before_agpm_section.is_empty() && !before_agpm_section.last().unwrap().trim().is_empty()
        {
            new_content.push('\n');
        }
    }

    // Add AGPM managed section
    new_content.push_str("# AGPM managed entries - do not edit below this line\n");

    // Convert paths to gitignore format (relative to project root)
    // Sort paths for consistent output
    let mut sorted_paths: Vec<_> = paths_to_ignore.into_iter().collect();
    sorted_paths.sort();

    for path in &sorted_paths {
        // Use paths as-is since gitignore is now at project root
        let ignore_path = if path.starts_with("./") {
            // Remove leading ./ if present
            path.strip_prefix("./").unwrap_or(path).to_string()
        } else {
            path.clone()
        };

        // Normalize to forward slashes for .gitignore (Git expects forward slashes on all platforms)
        let normalized_path = normalize_path_for_storage(&ignore_path);

        new_content.push_str(&normalized_path);
        new_content.push('\n');
    }

    new_content.push_str("# End of AGPM managed entries\n");

    // Add everything after AGPM section exactly as it was
    if !after_agpm_section.is_empty() {
        new_content.push('\n');
        for line in &after_agpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // If this is a new file, add a basic header
    if before_agpm_section.is_empty() && after_agpm_section.is_empty() {
        let mut default_content = String::new();
        default_content.push_str("# .gitignore - AGPM managed entries\n");
        default_content.push_str("# AGPM entries are automatically generated\n");
        default_content.push('\n');
        default_content.push_str("# AGPM managed entries - do not edit below this line\n");

        // Add the AGPM paths
        for path in &sorted_paths {
            let ignore_path = if path.starts_with("./") {
                path.strip_prefix("./").unwrap_or(path).to_string()
            } else {
                path.clone()
            };
            // Normalize to forward slashes for .gitignore (Git expects forward slashes on all platforms)
            let normalized_path = ignore_path.replace('\\', "/");
            default_content.push_str(&normalized_path);
            default_content.push('\n');
        }

        default_content.push_str("# End of AGPM managed entries\n");
        new_content = default_content;
    }

    // Write the updated gitignore
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| format!("Failed to update {}", gitignore_path.display()))?;

    Ok(())
}
