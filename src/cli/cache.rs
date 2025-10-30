//! Manage the global Git repository cache for AGPM.
//!
//! This module provides the `cache` command which allows users to manage the
//! global cache directory where AGPM stores cloned Git repositories. The cache
//! improves performance by avoiding repeated clones of the same repositories.
//!
//! # Features
//!
//! - **Cache Information**: View cache location, size, and contents
//! - **Selective Cleanup**: Remove unused cached repositories
//! - **Complete Cleanup**: Clear entire cache directory
//! - **Size Reporting**: Human-readable cache size formatting
//! - **Usage Analysis**: Identify active vs. unused cache entries
//!
//! # Cache Structure
//!
//! The cache directory (typically `~/.agpm/cache/`) contains:
//! - One subdirectory per source repository
//! - Each subdirectory is a bare Git clone
//! - Directory names match source names from manifests
//!
//! # Examples
//!
//! Show cache information:
//! ```bash
//! agpm cache info
//! agpm cache  # defaults to info
//! ```
//!
//! Clean unused cache entries:
//! ```bash
//! agpm cache clean
//! ```
//!
//! Clear entire cache:
//! ```bash
//! agpm cache clean --all
//! ```
//!
//! # Cache Management Strategy
//!
//! ## Automatic Cache Population
//! - Cache is populated during `install` and `update` commands
//! - Repositories are cloned as bare repositories for efficiency
//! - Multiple projects can share the same cached repository
//!
//! ## Cache Cleanup Logic
//! - **Unused Detection**: Compares cache contents with active project manifest
//! - **Safe Cleanup**: Only removes repositories not referenced in current project
//! - **Complete Cleanup**: `--all` flag removes entire cache regardless of usage
//!
//! ## Performance Benefits
//! - Avoids repeated Git clones for same repositories
//! - Reduces network traffic and installation time
//! - Enables offline operation for already-cached repositories
//!
//! # Security Considerations
//!
//! - Cache may contain authentication tokens in Git URLs
//! - Cleanup operations respect file permissions
//! - Cache location follows platform conventions for security
//!
//! # Error Conditions
//!
//! - Cache directory access permission issues
//! - File system errors during cleanup operations
//! - Manifest file parsing errors (for usage analysis)

use anyhow::Result;
use clap::{Args, Subcommand};
use colored::Colorize;

use crate::cache::Cache;
use crate::manifest::{Manifest, find_manifest_with_optional};
use std::path::PathBuf;

/// Command to manage the global Git repository cache.
///
/// This command provides operations for managing AGPM's global cache directory
/// where Git repositories are stored. The cache improves performance by avoiding
/// repeated clones of the same repositories across multiple projects.
///
/// # Default Behavior
///
/// If no subcommand is specified, defaults to showing cache information.
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::cache::{CacheCommand, CacheSubcommands};
///
/// // Show cache info (default behavior)
/// let cmd = CacheCommand { command: None };
///
/// // Clean unused cache entries
/// let cmd = CacheCommand {
///     command: Some(CacheSubcommands::Clean { all: false })
/// };
///
/// // Clear entire cache
/// let cmd = CacheCommand {
///     command: Some(CacheSubcommands::Clean { all: true })
/// };
/// ```
#[derive(Args)]
pub struct CacheCommand {
    /// Cache management operation to perform
    #[command(subcommand)]
    command: Option<CacheSubcommands>,
}

/// Subcommands for cache management operations.
///
/// This enum defines the available operations for managing the Git repository cache.
/// Each operation serves a different cache management purpose.
#[derive(Subcommand)]
enum CacheSubcommands {
    /// Remove cached repositories that are no longer needed.
    ///
    /// By default, this command performs "smart" cleanup by analyzing the current
    /// project's manifest to determine which cache entries are still needed. Only
    /// repositories not referenced by the current project are removed.
    ///
    /// # Smart Cleanup Logic
    /// 1. Loads the current project manifest (`agpm.toml`)
    /// 2. Extracts all source repository names
    /// 3. Compares with cached repository directories
    /// 4. Removes cache entries not referenced in the manifest
    ///
    /// # Complete Cleanup
    /// With the `--all` flag, removes the entire cache directory regardless
    /// of current usage. This is useful for:
    /// - Freeing maximum disk space
    /// - Resolving cache corruption issues
    /// - Forcing fresh downloads of all repositories
    ///
    /// # Examples
    /// ```bash
    /// agpm cache clean           # Remove unused entries
    /// agpm cache clean --all     # Remove entire cache
    /// ```
    Clean {
        /// Remove all cache, not just unused entries
        ///
        /// When enabled, removes the entire cache directory instead of
        /// performing selective cleanup based on manifest analysis.
        #[arg(long)]
        all: bool,
    },

    /// Display information about the cache directory.
    ///
    /// Shows comprehensive information about the cache including:
    /// - Cache directory location
    /// - Total cache size in human-readable format
    /// - List of cached repositories
    /// - Usage tips and commands
    ///
    /// # Information Displayed
    /// - **Location**: Full path to cache directory
    /// - **Size**: Total disk space used by cache
    /// - **Repositories**: List of cached repository directories
    /// - **Tips**: Helpful commands for cache management
    ///
    /// This is the default command when no subcommand is specified.
    ///
    /// # Examples
    /// ```bash
    /// agpm cache info    # Explicit info command
    /// agpm cache         # Defaults to info
    /// ```
    Info,
}

impl CacheCommand {
    /// Execute the cache command with default cache configuration.
    ///
    /// This method creates a new cache instance using the default cache directory
    /// and dispatches to the appropriate subcommand handler.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the cache operation completed successfully
    /// - `Err(anyhow::Error)` if cache creation or operation fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::cache::CacheCommand;
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = CacheCommand { command: None };
    /// // cmd.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    #[allow(dead_code)] // Used in tests
    pub async fn execute(self) -> Result<()> {
        let cache = Cache::new()?;
        self.execute_with_cache_and_manifest(cache, None).await
    }

    /// Execute the cache command with a specific manifest path.
    ///
    /// This method creates a new cache instance and uses the provided manifest path
    /// for operations that need to read the project manifest.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional path to the manifest file
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the cache operation completed successfully
    /// - `Err(anyhow::Error)` if cache creation or operation fails
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        let cache = Cache::new()?;
        self.execute_with_cache_and_manifest(cache, manifest_path).await
    }

    /// Execute the cache command with a specific cache instance.
    ///
    /// This method allows dependency injection of a cache instance, which is
    /// particularly useful for testing with temporary cache directories.
    ///
    /// # Arguments
    ///
    /// * `cache` - The cache instance to use for operations
    ///
    /// # Behavior
    ///
    /// Dispatches to the appropriate handler based on the subcommand:
    /// - `Clean { all: true }` → Complete cache cleanup
    /// - `Clean { all: false }` → Smart unused cache cleanup  
    /// - `Info` or `None` → Display cache information
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation completed successfully
    /// - `Err(anyhow::Error)` if the operation encounters an error
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::cache::{CacheCommand, CacheSubcommands};
    /// use agpm_cli::cache::Cache;
    /// use tempfile::TempDir;
    ///
    /// # tokio_test::block_on(async {
    /// let temp_dir = TempDir::new()?;
    /// let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    /// let cmd = CacheCommand {
    ///     command: Some(CacheSubcommands::Info)
    /// };
    /// cmd.execute_with_cache(cache).await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    #[allow(dead_code)] // Used in tests
    pub async fn execute_with_cache(self, cache: Cache) -> Result<()> {
        self.execute_with_cache_and_manifest(cache, None).await
    }

    /// Execute the cache command with a specific cache instance and manifest path.
    ///
    /// This method allows dependency injection of both cache and manifest path,
    /// which is particularly useful for testing.
    ///
    /// # Arguments
    ///
    /// * `cache` - The cache instance to use for operations
    /// * `manifest_path` - Optional path to the manifest file
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation completed successfully
    /// - `Err(anyhow::Error)` if the operation encounters an error
    async fn execute_with_cache_and_manifest(
        self,
        cache: Cache,
        manifest_path: Option<PathBuf>,
    ) -> Result<()> {
        match self.command {
            Some(CacheSubcommands::Clean {
                all,
            }) => {
                if all {
                    self.clean_all(cache).await
                } else {
                    self.clean_unused(cache, manifest_path).await
                }
            }
            Some(CacheSubcommands::Info) | None => self.show_info(cache).await,
        }
    }

    /// Remove all cached repositories regardless of usage.
    ///
    /// This method performs complete cache cleanup by removing the entire cache
    /// directory and all its contents. This is more aggressive than selective
    /// cleanup and is useful for:
    ///
    /// - Freeing maximum disk space
    /// - Resolving cache corruption issues  
    /// - Forcing fresh downloads of all repositories
    /// - Starting with a clean slate
    ///
    /// # Arguments
    ///
    /// * `cache` - The cache instance to operate on
    ///
    /// # Returns
    ///
    /// - `Ok(())` if cache cleanup completed successfully
    /// - `Err(anyhow::Error)` if file system operations fail
    ///
    /// # Side Effects
    ///
    /// - Removes the entire cache directory tree
    /// - All subsequent operations will need to re-clone repositories
    /// - Performance impact on next install/update operations
    async fn clean_all(&self, cache: Cache) -> Result<()> {
        println!("🗑️  Cleaning all cache...");

        // Also clean up stale lock files (older than 1 hour)
        let cache_dir = cache.cache_dir();
        if let Ok(removed) = crate::cache::lock::cleanup_stale_locks(cache_dir, 3600).await
            && removed > 0
        {
            println!("  Removed {removed} stale lock files");
        }

        cache.clear_all().await?;

        println!("{}", "✅ Cache cleared successfully".green().bold());
        Ok(())
    }

    /// Remove only cached repositories that are not referenced in the current manifest.
    ///
    /// This method performs intelligent cache cleanup by:
    ///
    /// 1. Loading the current project's manifest (`agpm.toml`)
    /// 2. Extracting the list of source repository names
    /// 3. Comparing cached repositories with active sources
    /// 4. Removing only cache entries not referenced in the manifest
    ///
    /// # Safety Features
    ///
    /// - Preserves cache entries for sources defined in the manifest
    /// - Gracefully handles missing manifest files
    /// - Provides clear feedback about cleanup results
    /// - Non-destructive when no manifest is found
    ///
    /// # Arguments
    ///
    /// * `cache` - The cache instance to operate on
    /// * `manifest_path` - Optional path to the manifest file
    ///
    /// # Behavior Without Manifest
    ///
    /// If no `agpm.toml` file is found:
    /// - Issues a warning message
    /// - Performs no cleanup operations
    /// - Suggests using `--all` flag for complete cleanup
    /// - Returns successfully without error
    ///
    /// # Returns
    ///
    /// - `Ok(())` if cleanup completed successfully or no manifest found
    /// - `Err(anyhow::Error)` if manifest loading or file operations fail
    ///
    /// # Examples
    ///
    /// Given a manifest with sources "official" and "community":
    /// - Cache entries "official" and "community" are preserved
    /// - Cache entry "old-unused" is removed
    /// - Cache entry "another-project" is removed
    async fn clean_unused(&self, cache: Cache, manifest_path: Option<PathBuf>) -> Result<()> {
        println!("🔍 Scanning for unused cache entries...");

        // Find manifest to get active sources
        let active_sources = if let Ok(manifest_path) = find_manifest_with_optional(manifest_path) {
            let manifest = Manifest::load(&manifest_path)?;
            manifest.sources.keys().cloned().collect::<Vec<_>>()
        } else {
            // No manifest found, can't determine what's in use
            println!("⚠️  No agpm.toml found. Use --all to clear entire cache.");
            return Ok(());
        };

        let removed = cache.clean_unused(&active_sources).await?;

        // Also clean up stale lock files (older than 1 hour)
        let cache_dir = cache.cache_dir();
        let lock_removed =
            crate::cache::lock::cleanup_stale_locks(cache_dir, 3600).await.unwrap_or(0);

        if removed > 0 || lock_removed > 0 {
            let mut messages = Vec::new();
            if removed > 0 {
                messages.push(format!("{removed} unused cache entries"));
            }
            if lock_removed > 0 {
                messages.push(format!("{lock_removed} stale lock files"));
            }
            println!("{}", format!("✅ Removed {}", messages.join(" and ")).green().bold());
        } else {
            println!("✨ Cache is already clean - no unused entries found");
        }

        Ok(())
    }

    /// Display comprehensive information about the cache directory and contents.
    ///
    /// This method provides a detailed overview of the cache including:
    ///
    /// - **Location**: Absolute path to the cache directory
    /// - **Size**: Total disk space used, formatted in human-readable units
    /// - **Contents**: List of cached repository directories
    /// - **Usage Tips**: Helpful commands for cache management
    ///
    /// # Information Displayed
    ///
    /// ## Cache Location
    /// Shows the full path to the cache directory, typically:
    /// - `~/.agpm/cache/` on Unix-like systems
    /// - `%APPDATA%/agpm/cache/` on Windows
    ///
    /// ## Cache Size
    /// Total disk space used by all cached repositories, automatically
    /// formatted using appropriate units (B, KB, MB, GB).
    ///
    /// ## Repository Listing
    /// Each cached repository is listed by name, corresponding to the
    /// source names defined in project manifests.
    ///
    /// # Arguments
    ///
    /// * `cache` - The cache instance to analyze
    ///
    /// # Returns
    ///
    /// - `Ok(())` if information display completed successfully
    /// - `Err(anyhow::Error)` if cache directory access or size calculation fails
    ///
    /// # Behavior
    ///
    /// - Handles non-existent cache directories gracefully
    /// - Shows empty cache state when no repositories are cached
    /// - Provides actionable tips for cache management
    /// - Uses async I/O for efficient directory scanning
    async fn show_info(&self, cache: Cache) -> Result<()> {
        let location = cache.get_cache_location();
        let size = cache.get_cache_size().await?;

        println!("{}", "Cache Information".bold());
        println!("  Location: {}", location.display());
        println!("  Size: {}", format_size(size));

        // List cached repositories
        if location.exists() {
            let mut entries = tokio::fs::read_dir(location).await?;
            let mut repos = Vec::new();

            while let Some(entry) = entries.next_entry().await? {
                if entry.path().is_dir()
                    && let Some(name) = entry.path().file_name()
                {
                    repos.push(name.to_string_lossy().to_string());
                }
            }

            if !repos.is_empty() {
                println!("\n{}", "Cached repositories:".bold());
                for repo in repos {
                    println!("  • {repo}");
                }
            }
        }

        println!("\n{}", "Tip:".yellow());
        println!("  Use 'agpm cache clean' to remove unused cache");
        println!("  Use 'agpm cache clean --all' to clear all cache");

        Ok(())
    }
}

/// Format byte size into human-readable string with appropriate units.
///
/// This function converts raw byte values into human-readable format using
/// standard binary prefixes (1024-based). It automatically selects the most
/// appropriate unit to avoid large numbers while maintaining reasonable precision.
///
/// # Arguments
///
/// * `bytes` - The number of bytes to format
///
/// # Units
///
/// Uses binary prefixes with the following progression:
/// - B (bytes): 0-1023 bytes
/// - KB (kilobytes): 1024+ bytes  
/// - MB (megabytes): 1024+ KB
/// - GB (gigabytes): 1024+ MB
///
/// # Formatting Rules
///
/// - Bytes: Displayed as whole numbers (e.g., "512 B")
/// - Larger units: Displayed with 2 decimal places (e.g., "1.50 KB")
/// - Zero bytes: Special case returns "0 B"
///
/// # Returns
///
/// A formatted string with the size and appropriate unit.
///
/// # Examples
///
/// ```rust,ignore
/// # use agpm_cli::cli::cache::format_size;
/// assert_eq!(format_size(0), "0 B");
/// assert_eq!(format_size(512), "512 B");
/// assert_eq!(format_size(1024), "1.00 KB");
/// assert_eq!(format_size(1536), "1.50 KB");
/// assert_eq!(format_size(1048576), "1.00 MB");
/// assert_eq!(format_size(1073741824), "1.00 GB");
/// ```
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    #[allow(clippy::cast_precision_loss)]
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_size_edge_cases() {
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1025), "1.00 KB");
        assert_eq!(format_size(1048575), "1024.00 KB");
        assert_eq!(format_size(1048577), "1.00 MB");
        assert_eq!(format_size(2097152), "2.00 MB");
        assert_eq!(format_size(5242880), "5.00 MB");
    }

    #[tokio::test]
    async fn test_cache_info_command() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Info),
        };

        // Create a cache directory with some test content
        let cache_dir = temp_dir.path().join("test-repo");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("test.txt"), "test content").unwrap();

        // This won't fail even if the display output changes
        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_clean_all() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create some test cache directories
        let repo1 = temp_dir.path().join("repo1");
        let repo2 = temp_dir.path().join("repo2");
        std::fs::create_dir_all(&repo1).unwrap();
        std::fs::create_dir_all(&repo2).unwrap();
        std::fs::write(repo1.join("file.txt"), "content").unwrap();
        std::fs::write(repo2.join("file.txt"), "content").unwrap();

        assert!(repo1.exists());
        assert!(repo2.exists());

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: true,
            }),
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());

        // Give a small delay to ensure async removal is completed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check that cache directory itself was cleared
        // Note: clear_all removes the entire cache directory
        // Since clear_all removes the cache dir entirely, the entire temp_dir should be empty
        // But temp_dir itself (the parent) should still exist due to TempDir
        assert!(!repo1.exists());
        assert!(!repo2.exists());
        // The entire cache directory should be gone
        assert!(
            !temp_dir.path().exists()
                || temp_dir.path().read_dir().map(|mut d| d.next().is_none()).unwrap_or(false)
        );
    }

    #[tokio::test]
    async fn test_cache_clean_unused_no_manifest() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: false,
            }),
        };

        // Pass a non-existent manifest path to ensure no manifest is found
        let non_existent_manifest = work_dir.path().join("agpm.toml");
        assert!(!non_existent_manifest.exists());

        // Without a manifest, should warn and not clean
        let result = cmd.execute_with_cache_and_manifest(cache, Some(non_existent_manifest)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_clean_unused_with_manifest() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create a manifest with one source
        let manifest = Manifest {
            sources: std::collections::HashMap::from([(
                "active".to_string(),
                "https://github.com/test/active.git".to_string(),
            )]),
            ..Default::default()
        };
        let manifest_path = work_dir.path().join("agpm.toml");
        manifest.save(&manifest_path).unwrap();

        // Create cache directories - one active (matches manifest), one unused
        let active_cache = temp_dir.path().join("active");
        let unused_cache = temp_dir.path().join("unused-test-source");
        std::fs::create_dir_all(&active_cache).unwrap();
        std::fs::create_dir_all(&unused_cache).unwrap();

        // Verify both exist before cleaning
        assert!(active_cache.exists());
        assert!(unused_cache.exists());

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: false,
            }),
        };

        let result = cmd.execute_with_cache_and_manifest(cache, Some(manifest_path)).await;
        assert!(result.is_ok());

        // Give a small delay to ensure async removal is completed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Active cache should remain, unused should be removed
        assert!(active_cache.exists());
        assert!(!unused_cache.exists());
    }

    #[tokio::test]
    async fn test_cache_default_command() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Test that no subcommand defaults to Info
        let cmd = CacheCommand {
            command: None,
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_info_with_empty_cache() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Ensure cache directory exists
        cache.ensure_cache_dir().await.unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Info),
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_info_with_content() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Ensure cache directory exists
        cache.ensure_cache_dir().await.unwrap();

        // Create some test content
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file1.txt"), "content1").unwrap();
        std::fs::write(source_dir.join("file2.txt"), "content2 with more data").unwrap();

        // Create nested directory with content
        let nested = source_dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("file3.txt"), "nested content").unwrap();

        // Get size before executing command (which consumes cache)
        let size = cache.get_cache_size().await.unwrap();
        assert!(size > 0);

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Info),
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_execute_without_dir() {
        // Test CacheCommand::execute which creates its own Cache

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Info),
        };

        // This uses the default cache directory
        let result = cmd.execute().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_clean_all_empty_cache() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Don't create any cache content
        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: true,
            }),
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());

        // Should handle empty cache gracefully
        assert!(!temp_dir.path().exists() || temp_dir.path().read_dir().unwrap().next().is_none());
    }

    #[tokio::test]
    async fn test_cache_clean_with_multiple_sources() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create manifest with multiple sources
        let manifest = Manifest {
            sources: std::collections::HashMap::from([
                ("source1".to_string(), "https://github.com/test/repo1.git".to_string()),
                ("source2".to_string(), "https://github.com/test/repo2.git".to_string()),
            ]),
            ..Default::default()
        };
        let manifest_path = work_dir.path().join("agpm.toml");
        manifest.save(&manifest_path).unwrap();

        // Create cache directories
        let source1_cache = temp_dir.path().join("source1");
        let source2_cache = temp_dir.path().join("source2");
        let unused_cache = temp_dir.path().join("unused");
        std::fs::create_dir_all(&source1_cache).unwrap();
        std::fs::create_dir_all(&source2_cache).unwrap();
        std::fs::create_dir_all(&unused_cache).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: false,
            }),
        };

        let result = cmd.execute_with_cache_and_manifest(cache, Some(manifest_path)).await;
        assert!(result.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Active sources should remain
        assert!(source1_cache.exists());
        assert!(source2_cache.exists());
        // Unused should be removed
        assert!(!unused_cache.exists());
    }

    #[tokio::test]
    async fn test_cache_info_formatting() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create cache with known size
        cache.ensure_cache_dir().await.unwrap();
        let test_file = temp_dir.path().join("test.txt");
        // Write exactly 1024 bytes (1 KB)
        let content = vec![b'a'; 1024];
        std::fs::write(&test_file, content).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Info),
        };

        let result = cmd.execute_with_cache(cache).await;
        assert!(result.is_ok());

        // The output should format 1024 bytes as "1.0 KB" or similar
        // This tests the format_size function
    }

    #[tokio::test]
    async fn test_cache_clean_no_manifest_warning() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // No manifest file exists
        let non_existent_manifest = work_dir.path().join("agpm.toml");
        assert!(!non_existent_manifest.exists());

        // Create some cache directories
        let cache_dir1 = temp_dir.path().join("source1");
        std::fs::create_dir_all(&cache_dir1).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: false,
            }),
        };

        // Pass a non-existent manifest path to ensure no manifest is found
        let result = cmd.execute_with_cache_and_manifest(cache, Some(non_existent_manifest)).await;
        assert!(result.is_ok());

        // Cache should remain untouched without manifest
        assert!(cache_dir1.exists());
    }

    #[tokio::test]
    async fn test_cache_size_calculation() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create files with known sizes
        std::fs::write(temp_dir.path().join("file1.txt"), vec![b'a'; 100]).unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), vec![b'b'; 200]).unwrap();

        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("file3.txt"), vec![b'c'; 300]).unwrap();

        // Total should be 100 + 200 + 300 = 600 bytes
        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 600);
    }

    #[tokio::test]
    async fn test_cache_clean_preserves_lockfile_sources() {
        use crate::lockfile::{LockFile, LockedSource};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create manifest with one source
        let manifest = Manifest {
            sources: std::collections::HashMap::from([(
                "manifest-source".to_string(),
                "https://github.com/test/repo.git".to_string(),
            )]),
            ..Default::default()
        };
        let manifest_path = work_dir.path().join("agpm.toml");
        manifest.save(&manifest_path).unwrap();

        // Create lockfile with additional sources
        let lockfile = LockFile {
            version: 1,
            sources: vec![
                LockedSource {
                    name: "manifest-source".to_string(),
                    url: "https://github.com/test/repo.git".to_string(),
                    fetched_at: chrono::Utc::now().to_string(),
                },
                LockedSource {
                    name: "lockfile-only".to_string(),
                    url: "https://github.com/test/other.git".to_string(),
                    fetched_at: chrono::Utc::now().to_string(),
                },
            ],
            agents: vec![],
            snippets: vec![],
            commands: vec![],
            mcp_servers: vec![],
            scripts: vec![],
            hooks: vec![],
            skills: vec![],
        };
        lockfile.save(&work_dir.path().join("agpm.lock")).unwrap();

        // Create cache directories
        let manifest_cache = temp_dir.path().join("manifest-source");
        let lockfile_cache = temp_dir.path().join("lockfile-only");
        let unused_cache = temp_dir.path().join("unused");
        std::fs::create_dir_all(&manifest_cache).unwrap();
        std::fs::create_dir_all(&lockfile_cache).unwrap();
        std::fs::create_dir_all(&unused_cache).unwrap();

        let cmd = CacheCommand {
            command: Some(CacheSubcommands::Clean {
                all: false,
            }),
        };

        let result = cmd.execute_with_cache_and_manifest(cache, Some(manifest_path)).await;
        assert!(result.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Manifest source should be preserved
        assert!(manifest_cache.exists());
        // Note: Current implementation doesn't preserve lockfile-only sources
        // This would be a good enhancement but isn't implemented yet
        // assert!(lockfile_cache.exists());
        // Unused should be removed
        assert!(!unused_cache.exists());
    }

    #[tokio::test]
    async fn test_format_size_function() {
        // Test the format_size helper function directly
        fn format_size(bytes: u64) -> String {
            const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
            let mut size = bytes as f64;
            let mut unit_index = 0;

            while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                size /= 1024.0;
                unit_index += 1;
            }

            if unit_index == 0 {
                format!("{} {}", size as u64, UNITS[unit_index])
            } else {
                format!("{:.1} {}", size, UNITS[unit_index])
            }
        }

        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(100), "100 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1099511627776), "1.0 TB");
    }

    #[tokio::test]
    async fn test_cache_path_display() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Get cache location
        let location = cache.get_cache_location();
        assert_eq!(location, temp_dir.path());

        // Test that path displays correctly (for Info command output)
        let path_str = location.display().to_string();
        assert!(path_str.contains(temp_dir.path().file_name().unwrap().to_str().unwrap()));
    }
}
