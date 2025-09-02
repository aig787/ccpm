//! Shared installation utilities for CCPM resources.
//!
//! This module provides common functionality for installing resources from
//! lockfile entries to the project directory. It's shared between the install
//! and update commands to avoid code duplication.

use anyhow::{Context, Result};
use futures::future::try_join_all;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::core::{FileOperations, FileOps, ResourceIterator, ResourceTypeExt};
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;
use crate::utils::fs::{atomic_write, ensure_dir};
use crate::utils::progress::ProgressBar;
use std::collections::HashSet;
use std::fs;

/// Install a single resource from a lock entry
pub async fn install_resource(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
) -> Result<()> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        // Default location based on resource type
        project_dir
            .join(resource_dir)
            .join(format!("{}.md", entry.name))
    } else {
        project_dir.join(&entry.installed_at)
    };

    // Install based on source type
    if let Some(source_name) = &entry.source {
        // Remote resource - use cache
        let url = entry
            .url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Remote resource {} has no URL", entry.name))?;

        // Get or clone the source to cache
        let cache_dir = cache
            .get_or_clone_source(
                source_name,
                url,
                entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .map(std::string::String::as_str),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to sync source '{}' for resource '{}'",
                    source_name, entry.name
                )
            })?;

        // Source path in the cache
        let source_path = cache_dir.join(&entry.path);

        // Read file content
        let content = FileOps::read_file_with_context(&source_path)?;

        // Validate markdown format only for markdown files
        if source_path.extension().and_then(|e| e.to_str()) == Some("md") {
            let _markdown = MarkdownFile::parse(&content)
                .with_context(|| format!("Invalid markdown file: {}", source_path.display()))?;
        }

        // Ensure destination directory exists
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Write to destination with atomic operation
        atomic_write(&dest_path, content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    } else {
        // Local resource
        let source_path = Path::new(&entry.path);
        if !source_path.exists() {
            return Err(anyhow::anyhow!(
                "Local resource file does not exist: {}",
                source_path.display()
            ));
        }

        // Read and validate markdown file
        let content = FileOps::read_file_with_context(source_path)?;

        // Validate markdown format
        let _markdown = MarkdownFile::parse(&content)
            .with_context(|| format!("Invalid markdown file: {}", source_path.display()))?;

        // Ensure destination directory exists
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Write to destination
        atomic_write(&dest_path, content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    }

    Ok(())
}

/// Install a single resource with progress tracking
pub async fn install_resource_with_progress(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    pb: &ProgressBar,
    cache: &Cache,
) -> Result<()> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(entry, project_dir, resource_dir, cache).await
}

/// Install multiple resources in parallel
pub async fn install_resources_parallel(
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    pb: &ProgressBar,
    cache: &Cache,
) -> Result<usize> {
    // Collect all entries to install using ResourceIterator
    let all_entries = ResourceIterator::collect_all_entries(lockfile, manifest);

    if all_entries.is_empty() {
        return Ok(0);
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let total = all_entries.len();
    let pb = Arc::new(pb.clone());

    // Wrap the cache in Arc so it can be shared across async tasks
    let cache = Arc::new(cache);

    // Set initial progress
    pb.set_message(format!("Installing 0/{total} resources"));

    // Create installation tasks
    let tasks = all_entries.into_iter().map(|(entry, resource_dir)| {
        let entry = entry.clone();
        let project_dir = project_dir.to_path_buf();
        let resource_dir = resource_dir.to_string();
        let installed_count = Arc::clone(&installed_count);
        let pb = Arc::clone(&pb);
        let cache = Arc::clone(&cache);

        async move {
            // Install the resource
            install_resource_for_parallel(&entry, &project_dir, &resource_dir, cache.as_ref())
                .await?;

            // Update progress
            let mut count = installed_count.lock().await;
            *count += 1;
            pb.set_message(format!("Installing {}/{} resources", *count, total));

            Ok::<(), anyhow::Error>(())
        }
    });

    // Execute all tasks in parallel
    try_join_all(tasks).await?;

    let final_count = *installed_count.lock().await;
    Ok(final_count)
}

/// Install a single resource in a thread-safe manner (for parallel execution)
async fn install_resource_for_parallel(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
) -> Result<()> {
    install_resource(entry, project_dir, resource_dir, cache).await
}

/// Install only specific updated resources
pub async fn install_updated_resources(
    updates: &[(String, String, String)], // (name, old_version, new_version)
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    quiet: bool,
) -> Result<usize> {
    let mut install_count = 0;

    for (name, _, _) in updates {
        // Find the resource in the lockfile using ResourceIterator
        if let Some((resource_type, entry)) =
            ResourceIterator::find_resource_by_name(lockfile, name)
        {
            if !quiet {
                println!("  Installing {name} ({})", resource_type);
            }
            let target_dir = resource_type.get_target_dir(&manifest.target);
            install_resource(entry, project_dir, target_dir, cache).await?;
            install_count += 1;
        }
    }

    Ok(install_count)
}

/// Update .gitignore with installed file paths
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
            if !resource.installed_at.is_empty() {
                // Use the explicit installed_at path
                paths_to_ignore.insert(resource.installed_at.clone());
            }
        }
    };

    // Collect paths from all resource types
    add_resource_paths(&lockfile.agents);
    add_resource_paths(&lockfile.snippets);
    add_resource_paths(&lockfile.commands);
    add_resource_paths(&lockfile.scripts);
    add_resource_paths(&lockfile.hooks);
    add_resource_paths(&lockfile.mcp_servers);

    // Read existing gitignore if it exists
    let mut before_ccpm_section = Vec::new();
    let mut after_ccpm_section = Vec::new();

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .with_context(|| format!("Failed to read {}", gitignore_path.display()))?;

        let mut in_ccpm_section = false;
        let mut past_ccpm_section = false;

        for line in content.lines() {
            if line == "# CCPM managed entries - do not edit below this line" {
                in_ccpm_section = true;
                continue;
            } else if line == "# End of CCPM managed entries" {
                in_ccpm_section = false;
                past_ccpm_section = true;
                continue;
            }

            if !in_ccpm_section && !past_ccpm_section {
                // Preserve everything before CCPM section exactly as-is
                before_ccpm_section.push(line.to_string());
            } else if in_ccpm_section {
                // Skip existing CCPM entries (they'll be replaced)
                continue;
            } else {
                // Preserve everything after CCPM section exactly as-is
                after_ccpm_section.push(line.to_string());
            }
        }
    }

    // Build the new content
    let mut new_content = String::new();

    // Add everything before CCPM section exactly as it was
    if !before_ccpm_section.is_empty() {
        for line in &before_ccpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
        // Add blank line before CCPM section if the previous content doesn't end with one
        if !before_ccpm_section.is_empty() && !before_ccpm_section.last().unwrap().trim().is_empty()
        {
            new_content.push('\n');
        }
    }

    // Add CCPM managed section
    new_content.push_str("# CCPM managed entries - do not edit below this line\n");

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

        new_content.push_str(&ignore_path);
        new_content.push('\n');
    }

    new_content.push_str("# End of CCPM managed entries\n");

    // Add everything after CCPM section exactly as it was
    if !after_ccpm_section.is_empty() {
        new_content.push('\n');
        for line in &after_ccpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // If this is a new file, add a basic header
    if before_ccpm_section.is_empty() && after_ccpm_section.is_empty() {
        let mut default_content = String::new();
        default_content.push_str("# .gitignore - CCPM managed entries\n");
        default_content.push_str("# CCPM entries are automatically generated\n");
        default_content.push('\n');
        default_content.push_str("# CCPM managed entries - do not edit below this line\n");

        // Add the CCPM paths
        for path in &sorted_paths {
            let ignore_path = if path.starts_with("./") {
                path.strip_prefix("./").unwrap_or(path).to_string()
            } else {
                path.clone()
            };
            default_content.push_str(&ignore_path);
            default_content.push('\n');
        }

        default_content.push_str("# End of CCPM managed entries\n");
        new_content = default_content;
    }

    // Write the updated gitignore
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| format!("Failed to update {}", gitignore_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_locked_resource(name: &str, is_local: bool) -> LockedResource {
        if is_local {
            LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                path: "test.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: String::new(),
            }
        } else {
            LockedResource {
                name: name.to_string(),
                source: Some("test_source".to_string()),
                url: Some("https://github.com/test/repo.git".to_string()),
                path: "resources/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:test".to_string(),
                installed_at: String::new(),
            }
        }
    }

    #[tokio::test]
    async fn test_install_resource_local() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Test Resource\nThis is a test").unwrap();

        // Create a locked resource pointing to the local file
        let mut entry = create_test_locked_resource("local-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(
            result.is_ok(),
            "Failed to install local resource: {:?}",
            result
        );

        // Verify the file was installed
        let expected_path = project_dir.join("agents").join("local-test.md");
        assert!(expected_path.exists(), "Installed file not found");

        // Verify content
        let content = std::fs::read_to_string(expected_path).unwrap();
        assert_eq!(content, "# Test Resource\nThis is a test");
    }

    #[tokio::test]
    async fn test_install_resource_with_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Custom Path Test").unwrap();

        // Create a locked resource with custom installation path
        let mut entry = create_test_locked_resource("custom-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "custom/location/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify the file was installed at custom path
        let expected_path = project_dir.join("custom/location/resource.md");
        assert!(expected_path.exists(), "File not installed at custom path");
    }

    #[tokio::test]
    async fn test_install_resource_local_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a locked resource pointing to non-existent file
        let mut entry = create_test_locked_resource("missing-test", true);
        entry.path = "/non/existent/file.md".to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_install_resource_invalid_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create an invalid markdown file
        let local_file = temp_dir.path().join("invalid.md");
        std::fs::write(&local_file, "---\ninvalid: yaml: [\n---\nContent").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("invalid-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid markdown"));
    }

    #[tokio::test]
    async fn test_install_resource_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Progress Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("progress-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install with progress
        let result =
            install_resource_with_progress(&entry, project_dir, "agents", &pb, &cache).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("progress-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resources_parallel_empty() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create empty lockfile and manifest
        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        let count = install_resources_parallel(&lockfile, &manifest, project_dir, &pb, &cache)
            .await
            .unwrap();

        assert_eq!(count, 0, "Should install 0 resources from empty lockfile");
    }

    #[tokio::test]
    async fn test_install_resources_parallel_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        let file3 = temp_dir.path().join("command.md");
        std::fs::write(&file1, "# Agent").unwrap();
        std::fs::write(&file2, "# Snippet").unwrap();
        std::fs::write(&file3, "# Command").unwrap();

        // Create lockfile with multiple resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let mut command = create_test_locked_resource("test-command", true);
        command.path = file3.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let count = install_resources_parallel(&lockfile, &manifest, project_dir, &pb, &cache)
            .await
            .unwrap();

        assert_eq!(count, 3, "Should install 3 resources");

        // Verify all files were installed (using default directories)
        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(project_dir
            .join(".claude/ccpm/snippets/test-snippet.md")
            .exists());
        assert!(project_dir
            .join(".claude/commands/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_updated_resources() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        std::fs::write(&file1, "# Updated Agent").unwrap();
        std::fs::write(&file2, "# Updated Snippet").unwrap();

        // Create lockfile with resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let manifest = Manifest::new();

        // Define updates (only agent is updated)
        let updates = vec![(
            "test-agent".to_string(),
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            false, // quiet
        )
        .await
        .unwrap();

        assert_eq!(count, 1, "Should install 1 updated resource");
        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(!project_dir
            .join(".claude/snippets/test-snippet.md")
            .exists()); // Not updated
    }

    #[tokio::test]
    async fn test_install_updated_resources_quiet_mode() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown file
        let file = temp_dir.path().join("command.md");
        std::fs::write(&file, "# Command").unwrap();

        // Create lockfile
        let mut lockfile = LockFile::new();
        let mut command = create_test_locked_resource("test-command", true);
        command.path = file.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let updates = vec![(
            "test-command".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            true, // quiet mode
        )
        .await
        .unwrap();

        assert_eq!(count, 1);
        assert!(project_dir
            .join(".claude/commands/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_resource_for_parallel() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("parallel.md");
        std::fs::write(&local_file, "# Parallel Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("parallel-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install using the parallel function
        let result = install_resource_for_parallel(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("parallel-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resource_creates_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("nested.md");
        std::fs::write(&local_file, "# Nested Test").unwrap();

        // Create a locked resource with deeply nested path
        let mut entry = create_test_locked_resource("nested-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "very/deeply/nested/path/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify nested directories were created
        let expected_path = project_dir.join("very/deeply/nested/path/resource.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_update_gitignore_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create a lockfile with some resources
        let mut lockfile = LockFile::new();

        // Add agent with installed path
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test-agent.md".to_string();
        lockfile.agents.push(agent);

        // Add snippet with installed path
        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.installed_at = ".claude/ccpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        // Call update_gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Check that .gitignore was created
        let gitignore_path = project_dir.join(".gitignore");
        assert!(gitignore_path.exists(), "Gitignore file should be created");

        // Check content
        let content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("CCPM managed entries"));
        assert!(content.contains(".claude/agents/test-agent.md"));
        assert!(content.contains(".claude/ccpm/snippets/test-snippet.md"));
    }

    #[tokio::test]
    async fn test_update_gitignore_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        let lockfile = LockFile::new();

        // Call with disabled flag
        let result = update_gitignore(&lockfile, project_dir, false);
        assert!(result.is_ok());

        // Check that .gitignore was NOT created
        let gitignore_path = project_dir.join(".gitignore");
        assert!(
            !gitignore_path.exists(),
            "Gitignore should not be created when disabled"
        );
    }

    #[tokio::test]
    async fn test_update_gitignore_preserves_user_entries() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create .claude directory for resources
        let claude_dir = project_dir.join(".claude");
        ensure_dir(&claude_dir).unwrap();

        // Create existing gitignore with user entries at project root
        let gitignore_path = project_dir.join(".gitignore");
        let existing_content = "# User comment\n\
                               user-file.txt\n\
                               *.backup\n\
                               # CCPM managed entries - do not edit below this line\n\
                               .claude/agents/old-entry.md\n\
                               # End of CCPM managed entries\n";
        std::fs::write(&gitignore_path, existing_content).unwrap();

        // Create lockfile with new resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("new-agent", true);
        agent.installed_at = ".claude/agents/new-agent.md".to_string();
        lockfile.agents.push(agent);

        // Update gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Check that user entries are preserved
        let updated_content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(updated_content.contains("user-file.txt"));
        assert!(updated_content.contains("*.backup"));
        assert!(updated_content.contains("# User comment"));

        // Check that new entries are added
        assert!(updated_content.contains(".claude/agents/new-agent.md"));

        // Check that old managed entries are replaced
        assert!(!updated_content.contains(".claude/agents/old-entry.md"));
    }

    #[tokio::test]
    async fn test_update_gitignore_handles_external_paths() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        let mut lockfile = LockFile::new();

        // Add resource installed outside .claude
        let mut script = create_test_locked_resource("test-script", true);
        script.installed_at = "scripts/test.sh".to_string();
        lockfile.scripts.push(script);

        // Add resource inside .claude
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test.md".to_string();
        lockfile.agents.push(agent);

        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        let gitignore_path = project_dir.join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path).unwrap();

        // External path should be as-is
        assert!(content.contains("scripts/test.sh"));

        // Internal path should be as-is
        assert!(content.contains(".claude/agents/test.md"));
    }

    #[tokio::test]
    async fn test_install_updated_resources_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        // Try to update a resource that doesn't exist
        let updates = vec![(
            "non-existent".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count =
            install_updated_resources(&updates, &lockfile, &manifest, project_dir, &cache, false)
                .await
                .unwrap();

        assert_eq!(count, 0, "Should install 0 resources when not found");
    }
}
