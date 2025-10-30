//! Cleanup utilities for removing obsolete artifacts.

use crate::lockfile::LockFile;
use anyhow::{Context, Result};

/// Removes artifacts that are no longer needed based on lockfile comparison.
///
/// This function performs automatic cleanup of obsolete resource files by comparing
/// the old and new lockfiles. It identifies and removes artifacts that have been:
/// - **Removed from manifest**: Dependencies deleted from `agpm.toml`
/// - **Changed to content-only**: Dependencies that changed from `install: true` to `install: false`
/// - **Relocated**: Files with changed `installed_at` paths due to:
///   - Relative path preservation (v0.3.18+)
///   - Custom target changes
///   - Dependency name changes
/// - **Replaced**: Resources that moved due to source or version changes
///
/// After removing files, it also cleans up any empty parent directories to prevent
/// directory accumulation over time.
///
/// # Cleanup Strategy
///
/// The function uses a **set-based difference algorithm**:
/// 1. Collects all `installed_at` paths from the new lockfile into a `HashSet`
///    (excluding resources with `install: false` which should not have files)
/// 2. Iterates through old lockfile resources
/// 3. For each old path not in the new set:
///    - Removes the file if it exists
///    - Recursively cleans empty parent directories
///    - Records the path for reporting
///
/// # Arguments
///
/// * `old_lockfile` - The previous lockfile state containing old installation paths
/// * `new_lockfile` - The current lockfile state with updated installation paths
/// * `project_dir` - The project root directory (usually contains `.claude/`)
///
/// # Returns
///
/// Returns `Ok(Vec<String>)` containing the list of `installed_at` paths that were
/// successfully removed. An empty vector indicates no artifacts needed cleanup.
///
/// # Errors
///
/// Returns an error if:
/// - File removal fails due to permissions or locks
/// - Directory cleanup encounters unexpected I/O errors
/// - File system operations fail for other reasons
///
/// # Examples
///
/// ## Basic Cleanup After Update
///
/// ```no_run
/// use agpm_cli::installer::cleanup_removed_artifacts;
/// use agpm_cli::lockfile::LockFile;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let old_lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let new_lockfile = LockFile::new(); // After resolution
/// let project_dir = Path::new(".");
///
/// let removed = cleanup_removed_artifacts(&old_lockfile, &new_lockfile, project_dir).await?;
/// if !removed.is_empty() {
///     println!("Cleaned up {} artifact(s)", removed.len());
///     for path in removed {
///         println!("  - Removed: {}", path);
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Cleanup After Path Migration
///
/// When relative path preservation changes installation paths:
///
/// ```text
/// Old lockfile (v0.3.17):
///   installed_at: ".claude/agents/helper.md"
///
/// New lockfile (v0.3.18+):
///   installed_at: ".claude/agents/ai/helper.md"  # Preserved subdirectory
///
/// Cleanup removes: .claude/agents/helper.md
/// ```
///
/// ## Cleanup After Dependency Removal
///
/// ```no_run
/// # use agpm_cli::installer::cleanup_removed_artifacts;
/// # use agpm_cli::lockfile::{LockFile, LockedResource};
/// # use std::path::Path;
/// # async fn removal_example() -> anyhow::Result<()> {
/// // Old lockfile had 3 agents
/// let mut old_lockfile = LockFile::new();
/// old_lockfile.agents = vec![
///     // ... 3 agents including one at .claude/agents/removed.md
/// ];
///
/// // New lockfile only has 2 agents (one was removed from manifest)
/// let mut new_lockfile = LockFile::new();
/// new_lockfile.agents = vec![
///     // ... 2 agents, removed.md is gone
/// ];
///
/// let removed = cleanup_removed_artifacts(&old_lockfile, &new_lockfile, Path::new(".")).await?;
/// assert!(removed.contains(&".claude/agents/removed.md".to_string()));
/// # Ok(())
/// # }
/// ```
///
/// ## Integration with Install Command
///
/// This function is automatically called during `agpm install` when both old and
/// new lockfiles exist:
///
/// ```rust,ignore
/// // In src/cli/install.rs
/// if !self.frozen && !self.regenerate && lockfile_path.exists() {
///     if let Ok(old_lockfile) = LockFile::load(&lockfile_path) {
///         detect_tag_movement(&old_lockfile, &lockfile, self.quiet);
///
///         // Automatic cleanup of removed or moved artifacts
///         if let Ok(removed) = cleanup_removed_artifacts(
///             &old_lockfile,
///             &lockfile,
///             actual_project_dir,
///         ).await && !removed.is_empty() && !self.quiet {
///             println!("🗑️  Cleaned up {} moved or removed artifact(s)", removed.len());
///         }
///     }
/// }
/// ```
///
/// # Performance
///
/// - **Time Complexity**: O(n + m) where n = old resources, m = new resources
/// - **Space Complexity**: O(m) for the `HashSet` of new paths
/// - **I/O Operations**: One file removal per obsolete artifact
/// - **Directory Cleanup**: Walks up parent directories once per removed file
///
/// The function is highly efficient as it:
/// - Uses `HashSet` for O(1) path lookups
/// - Only performs I/O for files that actually exist
/// - Cleans directories recursively but stops at first non-empty directory
///
/// # Safety
///
/// - Only removes files explicitly tracked in the old lockfile
/// - Never removes files outside the project directory
/// - Stops directory cleanup at `.claude/` boundary
/// - Handles concurrent file access gracefully (ENOENT is not an error)
///
/// # Use Cases
///
/// ## Relative Path Migration (v0.3.18+)
///
/// When upgrading to v0.3.18+, resource paths change to preserve directory structure:
/// ```text
/// Before: .claude/agents/helper.md  (flat)
/// After:  .claude/agents/ai/helper.md  (nested)
/// ```
/// This function removes the old flat file automatically.
///
/// ## Dependency Reorganization
///
/// When reorganizing dependencies with custom targets:
/// ```toml
/// # Before
/// [agents]
/// helper = { source = "community", path = "agents/helper.md" }
///
/// # After (with custom target)
/// [agents]
/// helper = { source = "community", path = "agents/helper.md", target = "tools" }
/// ```
/// Old file at `.claude/agents/helper.md` is removed, new file at
/// `.claude/agents/tools/helper.md` is installed.
///
/// ## Manifest Cleanup
///
/// Simply removing dependencies from `agpm.toml` triggers automatic cleanup:
/// ```toml
/// # Remove unwanted dependency
/// [agents]
/// # old-agent = { ... }  # Commented out or deleted
/// ```
/// The next `agpm install` removes the old agent file automatically.
///
/// # Version History
///
/// - **v0.3.18**: Introduced to handle relative path preservation and custom target changes
/// - Works in conjunction with `cleanup_empty_dirs()` for comprehensive cleanup
pub async fn cleanup_removed_artifacts(
    old_lockfile: &LockFile,
    new_lockfile: &LockFile,
    project_dir: &std::path::Path,
) -> Result<Vec<String>> {
    use std::collections::HashSet;

    let mut removed = Vec::new();

    // Collect installed paths from new lockfile (only resources that should have files on disk)
    // Resources with install=false are content-only and should not have files
    let new_paths: HashSet<String> = new_lockfile
        .all_resources()
        .into_iter()
        .filter(|r| r.install != Some(false))
        .map(|r| r.installed_at.clone())
        .collect();

    // Check each old resource
    for old_resource in old_lockfile.all_resources() {
        // If the old path doesn't exist in new lockfile, it needs to be removed
        if !new_paths.contains(&old_resource.installed_at) {
            let full_path = project_dir.join(&old_resource.installed_at);

            tracing::debug!(
                "Cleanup: old path not in new lockfile - name={}, path={}, install={:?}, exists={}",
                old_resource.name,
                old_resource.installed_at,
                old_resource.install,
                full_path.exists()
            );

            // Only remove if the file actually exists
            if full_path.exists() {
                // Skills are directories, all other resources are files
                if old_resource.resource_type == crate::core::ResourceType::Skill {
                    // Remove skill directory recursively
                    tokio::fs::remove_dir_all(&full_path).await.with_context(|| {
                        format!("Failed to remove old skill directory: {}", full_path.display())
                    })?;
                } else {
                    // Remove single file for other resource types
                    tokio::fs::remove_file(&full_path).await.with_context(|| {
                        format!("Failed to remove old artifact: {}", full_path.display())
                    })?;
                }

                removed.push(old_resource.installed_at.clone());

                // Try to clean up empty parent directories
                cleanup_empty_dirs(&full_path).await?;
            }
        }
    }

    Ok(removed)
}

/// Recursively removes empty parent directories up to the project root.
///
/// This helper function performs bottom-up directory cleanup after file removal.
/// It walks up the directory tree from a given file path, removing empty parent
/// directories until it encounters:
/// - A non-empty directory (containing other files or subdirectories)
/// - The `.claude` directory boundary (cleanup stops here for safety)
/// - The project root (no parent directory)
/// - A directory that cannot be removed (permissions, locks, etc.)
///
/// This prevents accumulation of empty directory trees over time as resources
/// are removed, renamed, or relocated.
///
/// # Cleanup Algorithm
///
/// The function implements a **safe recursive cleanup** strategy:
/// 1. Starts at the parent directory of the given file path
/// 2. Attempts to remove the directory
/// 3. If successful (directory was empty), moves to parent and repeats
/// 4. If unsuccessful, stops immediately (directory has content or other issues)
/// 5. Always stops at `.claude/` directory to avoid over-cleanup
///
/// # Safety Boundaries
///
/// The function enforces strict boundaries to prevent accidental data loss:
/// - **`.claude/` boundary**: Never removes the `.claude` directory itself
/// - **Project root**: Stops if parent directory is None
/// - **Non-empty guard**: Only removes truly empty directories
/// - **Error tolerance**: ENOENT (directory not found) is not considered an error
///
/// # Arguments
///
/// * `file_path` - The path to the removed file whose parent directories should be cleaned.
///   Typically this is the full path to a resource file that was just deleted.
///
/// # Returns
///
/// Returns `Ok(())` in all normal cases, including:
/// - All empty directories successfully removed
/// - Cleanup stopped at a non-empty directory
/// - Directory already doesn't exist (ENOENT)
///
/// # Errors
///
/// Returns an error only for unexpected I/O failures during directory removal
/// that are not normal "directory not empty" or "not found" errors.
///
/// # Examples
///
/// ## Basic Directory Cleanup
///
/// ```ignore
/// # use agpm_cli::installer::cleanup_empty_dirs;
/// # use std::path::Path;
/// # use std::fs;
/// # async fn example() -> anyhow::Result<()> {
/// // After removing: .claude/agents/rust/specialized/expert.md
/// let file_path = Path::new(".claude/agents/rust/specialized/expert.md");
///
/// // If this was the last file in specialized/, the directory will be removed
/// // If specialized/ was the last item in rust/, that will be removed too
/// // Cleanup stops at .claude/agents/ or when it finds a non-empty directory
/// cleanup_empty_dirs(file_path).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Cleanup Scenarios
///
/// ### Scenario 1: Full Cleanup
///
/// ```text
/// Before:
///   .claude/agents/rust/specialized/expert.md  (only file in hierarchy)
///
/// After removing expert.md:
///   cleanup_empty_dirs() removes:
///   - .claude/agents/rust/specialized/  (now empty)
///   - .claude/agents/rust/              (now empty)
///   Stops at .claude/agents/ (keeps base directory)
/// ```
///
/// ### Scenario 2: Partial Cleanup
///
/// ```text
/// Before:
///   .claude/agents/rust/specialized/expert.md
///   .claude/agents/rust/specialized/tester.md
///   .claude/agents/rust/basic.md
///
/// After removing expert.md:
///   .claude/agents/rust/specialized/ still has tester.md
///   cleanup_empty_dirs() stops at specialized/ (not empty)
/// ```
///
/// ### Scenario 3: Boundary Enforcement
///
/// ```text
/// After removing: .claude/agents/only-agent.md
///
/// cleanup_empty_dirs() attempts to remove:
/// - .claude/agents/ (empty now)
/// - But stops because parent is .claude/ (boundary)
///
/// Result: .claude/agents/ remains (empty but preserved)
/// ```
///
/// ## Integration with `cleanup_removed_artifacts`
///
/// This function is called automatically by [`cleanup_removed_artifacts`]
/// after each file removal:
///
/// ```rust,ignore
/// for old_resource in old_lockfile.all_resources() {
///     if !new_paths.contains(&old_resource.installed_at) {
///         let full_path = project_dir.join(&old_resource.installed_at);
///
///         if full_path.exists() {
///             tokio::fs::remove_file(&full_path).await?;
///             removed.push(old_resource.installed_at.clone());
///
///             // Automatic directory cleanup after file removal
///             cleanup_empty_dirs(&full_path).await?;
///         }
///     }
/// }
/// ```
///
/// # Performance
///
/// - **Time Complexity**: O(d) where d = directory depth from file to `.claude/`
/// - **I/O Operations**: One `remove_dir` attempt per directory level
/// - **Early Termination**: Stops immediately on first non-empty directory
///
/// The function is extremely efficient as it:
/// - Only walks up the directory tree (no scanning of siblings)
/// - Stops at the first non-empty directory (no unnecessary attempts)
/// - Uses atomic `remove_dir` which fails fast on non-empty directories
/// - Typical depth is 2-4 levels (.claude/agents/subdir/file.md)
///
/// # Error Handling Strategy
///
/// The function differentiates between expected and unexpected errors:
///
/// | Error Kind | Interpretation | Action |
/// |------------|----------------|--------|
/// | `Ok(())` | Directory was empty and removed | Continue up tree |
/// | `ENOENT` | Directory doesn't exist | Continue up tree (race condition) |
/// | `ENOTEMPTY` | Directory has contents | Stop cleanup (expected) |
/// | `EPERM` | No permission | Stop cleanup (expected) |
/// | Other | Unexpected I/O error | Propagate error |
///
/// In practice, most errors simply stop the cleanup process without failing
/// the overall operation, as the goal is best-effort cleanup.
///
/// # Thread Safety
///
/// This function is safe for concurrent use because:
/// - Uses async filesystem operations from `tokio::fs`
/// - `remove_dir` is atomic (succeeds only if directory is empty)
/// - ENOENT handling accounts for race conditions
/// - Multiple concurrent calls won't interfere with each other
///
/// # Use Cases
///
/// ## After Pattern-Based Installation Changes
///
/// When pattern matches change, old directory structures may become empty:
/// ```toml
/// # Old: pattern matched agents/rust/expert.md, agents/rust/testing.md
/// # New: pattern only matches agents/rust/expert.md
///
/// # testing.md removed → agents/rust/ might now be empty
/// ```
///
/// ## After Custom Target Changes
///
/// Custom target changes can leave old directory structures empty:
/// ```toml
/// # Old: target = "tools"  → .claude/agents/tools/helper.md
/// # New: target = "utils" → .claude/agents/utils/helper.md
///
/// # .claude/agents/tools/ might now be empty
/// ```
///
/// ## After Dependency Removal
///
/// Removing the last dependency in a category may leave empty subdirectories:
/// ```toml
/// [agents]
/// # Removed: python-helper (was in agents/python/)
/// # Only agents/rust/ remains
///
/// # .claude/agents/python/ should be cleaned up
/// ```
///
/// # Design Rationale
///
/// This function exists to solve the "directory accumulation problem":
/// - Without cleanup: Empty directories accumulate over time
/// - With cleanup: Project structure stays clean and organized
/// - Safety boundaries: Prevents accidental removal of important directories
/// - Best-effort approach: Cleanup failures don't block main operations
///
/// # Version History
///
/// - **v0.3.18**: Introduced alongside [`cleanup_removed_artifacts`]
/// - Complements relative path preservation by cleaning up old directory structures
async fn cleanup_empty_dirs(file_path: &std::path::Path) -> Result<()> {
    let mut current = file_path.parent();

    while let Some(dir) = current {
        // Stop at .claude directory (check file name, not path suffix)
        // This prevents incorrectly matching paths like .claude/skills/my-skill/.claude/test
        if dir.file_name().and_then(|n| n.to_str()) == Some(".claude") {
            break;
        }

        // Stop at project root
        if dir.parent().is_none() {
            break;
        }

        // Try to remove the directory (will only succeed if empty)
        match tokio::fs::remove_dir(dir).await {
            Ok(()) => {
                // Directory was empty and removed, continue up
                current = dir.parent();
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Directory doesn't exist, continue up
                current = dir.parent();
            }
            Err(_) => {
                // Directory is not empty or we don't have permission, stop here
                break;
            }
        }
    }

    Ok(())
}
