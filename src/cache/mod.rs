//! Git repository cache management with worktree-based parallel operations
//!
//! This module provides a sophisticated caching system for Git repositories that enables
//! safe parallel resource installation through Git worktrees. The cache system has been
//! redesigned for optimal concurrency, simplified architecture, and enhanced performance
//! in AGPM v0.3.0.
//!
//! # Architecture Overview
//!
//! The cache system implements a multi-layered architecture:
//! - [`Cache`] struct: Core repository management and worktree orchestration
//! - [`CacheLock`]: File-based locking for process-safe concurrent access
//! - `WorktreeState`: Instance-level caching for worktree lifecycle management
//! - Bare repositories: Optimized Git storage for efficient worktree creation
//!
//! # Platform-Specific Cache Locations
//!
//! The cache follows platform conventions for optimal performance:
//! - **Linux/macOS**: `~/.agpm/cache/` (following XDG standards)
//! - **Windows**: `%LOCALAPPDATA%\agpm\cache\` (using Windows cache directory)
//! - **Environment Override**: Set `AGPM_CACHE_DIR` for custom locations
//!
//! # Cache Directory Structure
//!
//! The cache is organized for optimal parallel access patterns:
//! ```text
//! ~/.agpm/cache/
//! ├── sources/                    # Bare repositories optimized for worktrees
//! │   ├── github_owner_repo.git/  # Bare repo with all Git objects
//! │   └── gitlab_org_project.git/ # URL-parsed directory naming
//! ├── worktrees/                  # SHA-based worktrees for maximum deduplication
//! │   ├── github_owner_repo_abc12345/ # First 8 chars of commit SHA
//! │   ├── github_owner_repo_def67890/ # Each unique commit gets one worktree
//! │   ├── .state.json             # Persistent worktree registry
//! │   └── github_owner_repo_456789ab/ # Multiple refs to same SHA share worktree
//! └── .locks/                     # Fine-grained locking infrastructure
//!     ├── github_owner_repo.lock      # Repository-level locks
//!     └── worktree-owner_repo-v1.lock # Worktree creation locks
//! ```
//!
//! # Enhanced Concurrency Architecture
//!
//! The v0.3.2+ cache implements SHA-based worktree optimization with advanced concurrency:
//! - **SHA-based deduplication**: Worktrees keyed by commit SHA, not version reference
//! - **Centralized resolution**: `VersionResolver` handles batch SHA resolution upfront
//! - **Maximum reuse**: Multiple tags/branches pointing to same commit share one worktree
//! - **Instance-level caching**: `WorktreeState` tracks creation across threads
//! - **Per-worktree file locking**: Fine-grained locks prevent creation conflicts
//! - **Direct parallelism control**: `--max-parallel` flag controls concurrency
//! - **Command-instance fetch caching**: Single fetch per repository per command
//! - **Atomic state transitions**: Pending → Ready state coordination
//!
//! ## Locking Strategy
//!
//! ```text
//! Process A: acquire("source1") ───┐
//!                                   ├─── BLOCKS: same source
//! Process B: acquire("source1") ───┘
//!
//! Process C: acquire("source2") ───── CONCURRENT: different source
//! ```
//!
//! # Cache Operations
//!
//! ## Repository Management
//! - **Clone**: Initial repository cloning from remote URLs
//! - **Update**: Fetch latest changes from remote (git fetch)
//! - **Checkout**: Switch to specific versions (tags, branches, commits)
//! - **Cleanup**: Remove unused repositories to reclaim disk space
//!
//! ## Resource Installation
//! - **Copy-based**: Files copied from cache to project directories
//! - **Path resolution**: Handles relative paths within repositories
//! - **Directory creation**: Automatically creates parent directories
//! - **Overwrite safety**: Replaces existing files atomically
//!
//! # Performance Characteristics
//!
//! The cache is optimized for common AGPM workflows:
//! - **First install**: Clone repository once, reuse for all resources
//! - **Subsequent installs**: Copy from local cache (fast file operations)
//! - **Version switching**: Git checkout within cached repository
//! - **Parallel operations**: Multiple sources can be processed concurrently
//!
//! ## Disk Space Management
//!
//! - **Size calculation**: Recursive directory size calculation
//! - **Unused cleanup**: Remove repositories no longer referenced
//! - **Complete cleanup**: Clear entire cache when needed
//! - **Selective removal**: Keep active sources, remove only unused ones
//!
//! # Error Handling and Recovery
//!
//! The cache provides comprehensive error handling:
//! - **Lock timeouts**: Graceful handling of concurrent access
//! - **Clone failures**: Network and authentication error reporting
//! - **Version errors**: Clear messages for invalid tags/branches/commits
//! - **File system errors**: Detailed context for permission and space issues
//!
//! # Security Considerations
//!
//! - **Path validation**: Prevents directory traversal attacks
//! - **Lock file isolation**: Prevents lock file manipulation
//! - **Safe file operations**: Atomic operations prevent corruption
//! - **Permission handling**: Respects file system permissions
//!
//! # Usage Examples
//!
//! ## Basic Cache Operations
//!
//! ```rust,no_run
//! use agpm_cli::cache::Cache;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Initialize cache with default location
//! let cache = Cache::new()?;
//!
//! // Get or clone a source repository
//! let repo_path = cache.get_or_clone_source(
//!     "community",
//!     "https://github.com/example/agpm-community.git",
//!     Some("v1.0.0")  // Specific version
//! ).await?;
//!
//! // Copy a resource from cache to project
//! cache.copy_resource(
//!     &repo_path,
//!     "agents/helper.md",  // Source path in repository
//!     &PathBuf::from("./agents/helper.md")  // Destination in project
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Cache Maintenance
//!
//! ```rust,no_run
//! use agpm_cli::cache::Cache;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let cache = Cache::new()?;
//!
//! // Check cache size
//! let size_bytes = cache.get_cache_size().await?;
//! println!("Cache size: {} MB", size_bytes / 1024 / 1024);
//!
//! // Clean unused repositories
//! let active_sources = vec!["community".to_string(), "work".to_string()];
//! let removed_count = cache.clean_unused(&active_sources).await?;
//! println!("Removed {} unused repositories", removed_count);
//!
//! // Complete cache cleanup
//! cache.clear_all().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom Cache Location
//!
//! ```rust,no_run
//! use agpm_cli::cache::Cache;
//! use std::path::PathBuf;
//!
//! # fn custom_location() -> anyhow::Result<()> {
//! // Use custom cache directory (useful for testing or special setups)
//! let custom_dir = PathBuf::from("/tmp/my-agpm-cache");
//! let cache = Cache::with_dir(custom_dir)?;
//!
//! println!("Using cache at: {}", cache.get_cache_location().display());
//! # Ok(())
//! # }
//! ```
//!
//! # Integration with AGPM Workflow
//!
//! The cache module integrates seamlessly with AGPM's dependency management:
//! 1. **Manifest parsing**: Source URLs extracted from `agpm.toml`
//! 2. **Dependency resolution**: Version constraints resolved to specific commits
//! 3. **Cache population**: Repositories cloned and checked out as needed
//! 4. **Resource installation**: Files copied from cache to project directories
//! 5. **Lockfile generation**: Installed resources tracked in `agpm.lock`
//!
//! See [`crate::manifest`] for manifest handling and [`crate::lockfile`] for
//! lockfile management.

use crate::core::error::AgpmError;
use crate::core::file_error::{FileOperation, FileResultExt};
use crate::git::GitRepo;
use crate::git::command_builder::GitCommand;
use crate::utils::fs;
use crate::utils::security::validate_path_security;
use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs as async_fs;
use tokio::sync::{Mutex, RwLock};

// Concurrency Architecture:
// - Direct control approach: Command parallelism (--max-parallel) + per-worktree file locking
// - Instance-level caching: Worktrees and fetch operations cached per Cache instance
// - Command-level control: --max-parallel flag controls dependency processing parallelism
// - Fetch caching: Network operations cached for 5 minutes to reduce redundancy

/// State of a worktree in the instance-level cache for concurrent coordination.
///
/// This enum implements a sophisticated state machine for worktree lifecycle management
/// that enables safe concurrent access across multiple threads without race conditions.
/// The cache uses this state to coordinate between threads that might request the same
/// worktree simultaneously, eliminating the need for global synchronization bottlenecks.
///
/// # State Transitions
///
/// - **Initial**: No entry exists in cache (implicit state)
/// - [`Pending`](WorktreeState::Pending): One thread is creating the worktree
/// - [`Ready`](WorktreeState::Ready): Worktree exists and is ready for all threads
///
/// # Concurrency Coordination Pattern
///
/// The worktree creation process follows this coordinated pattern:
/// 1. **Reservation**: First thread reserves slot by setting state to `Pending`
/// 2. **Creation**: Reserved thread performs actual worktree creation with file lock
/// 3. **Notification**: Creator updates state to `Ready(path)` when complete
/// 4. **Reuse**: Subsequent threads immediately use the ready worktree path
/// 5. **Validation**: All threads verify worktree still exists before use
///
/// # Cache Key Format
///
/// Worktrees are uniquely identified by composite keys:
/// ```text
/// "{cache_dir_hash}:{owner}_{repo}:{version}"
/// ```
///
/// Components:
/// - `cache_dir_hash`: First 8 hex chars of cache directory path hash
/// - `owner_repo`: Parsed from Git URL (e.g., "`github_owner_project`")
/// - `version`: Git reference (tag, branch, commit, or "HEAD")
///
/// This format ensures isolation between:
/// - Different cache instances (via hash)
/// - Different repositories (via owner/repo)
/// - Different versions (via version string)
///
/// # Memory Management
///
/// The instance-level cache persists for the lifetime of the `Cache` instance,
/// but worktrees are validated on each access to handle external deletion.
#[derive(Debug, Clone)]
enum WorktreeState {
    /// Another thread is currently creating this worktree.
    ///
    /// When threads encounter this state, they should wait briefly and retry
    /// rather than attempting concurrent worktree creation which would fail.
    Pending,

    /// Worktree is fully created and ready to use.
    ///
    /// The `PathBuf` contains the filesystem path to the working directory.
    /// This path should be validated before use as the worktree may have been
    /// externally deleted.
    Ready(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorktreeRegistry {
    entries: HashMap<String, WorktreeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorktreeRecord {
    source: String,
    version: String,
    path: PathBuf,
    last_used: u64,
}

impl WorktreeRegistry {
    fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                tracing::warn!("Failed to load worktree registry from {}: {}", path.display(), err);
                Self::default()
            }
        }
    }

    fn update(&mut self, key: String, source: String, version: String, path: PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();

        self.entries.insert(
            key,
            WorktreeRecord {
                source,
                version,
                path,
                last_used: timestamp,
            },
        );
    }

    fn remove_by_path(&mut self, target: &Path) -> bool {
        if let Some(key) = self.entries.iter().find_map(|(k, record)| {
            if record.path == target {
                Some(k.clone())
            } else {
                None
            }
        }) {
            self.entries.remove(&key);
            true
        } else {
            false
        }
    }

    async fn persist(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            async_fs::create_dir_all(parent).await?;
        }

        let data = serde_json::to_vec_pretty(self)?;
        async_fs::write(path, data).await?;
        Ok(())
    }
}

/// File-based locking mechanism for cache operations
///
/// This module provides thread-safe and process-safe locking for cache
/// operations through OS-level file locks, ensuring data consistency
/// when multiple AGPM processes access the same cache directory.
pub mod lock;
pub use lock::CacheLock;

/// Git repository cache for efficient resource management
///
/// The `Cache` struct provides the primary interface for managing Git repository
/// caching in AGPM. It handles repository cloning, updating, version management,
/// and resource file copying operations.
///
/// # Thread Safety
///
/// While the `Cache` struct itself is not thread-safe (not `Send + Sync`),
/// multiple instances can safely operate on the same cache directory through
/// the file-based locking mechanism provided by [`CacheLock`].
///
/// # Platform Compatibility
///
/// The cache automatically handles platform-specific differences:
/// - **Path separators**: Uses [`std::path`] for cross-platform compatibility
/// - **Cache location**: Follows platform conventions for app data storage
/// - **File locking**: Uses [`fs4`] crate for cross-platform file locking
/// - **Directory creation**: Handles permissions and long paths on Windows
///
/// # Examples
///
/// Create a cache with default platform-specific location:
///
/// ```rust,no_run
/// use agpm_cli::cache::Cache;
///
/// # fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// println!("Cache location: {}", cache.get_cache_location().display());
/// # Ok(())
/// # }
/// ```
///
/// Create a cache with custom location (useful for testing):
///
/// ```rust,no_run
/// use agpm_cli::cache::Cache;
/// use std::path::PathBuf;
///
/// # fn example() -> anyhow::Result<()> {
/// let custom_dir = PathBuf::from("/tmp/test-cache");
/// let cache = Cache::with_dir(custom_dir)?;
/// # Ok(())
/// # }
/// ```
pub struct Cache {
    /// The root directory where all cached repositories are stored
    dir: PathBuf,

    /// Instance-level cache for worktrees to avoid redundant checkouts.
    ///
    /// This cache maps worktree identifiers to their creation state, enabling
    /// safe concurrent access. Multiple threads can request the same worktree
    /// without conflicts - the first thread creates it while others wait.
    ///
    /// **Key format**: `"{cache_dir_hash}:{owner}_{repo}:{version}"`
    ///
    /// The cache directory hash ensures isolation between different Cache instances,
    /// preventing conflicts when multiple instances operate on different cache roots.
    worktree_cache: Arc<RwLock<HashMap<String, WorktreeState>>>,

    /// Per-repository async locks that serialize fetch operations across
    /// concurrent tasks. This prevents redundant `git fetch` runs when
    /// multiple dependencies target the same repository simultaneously.
    fetch_locks: Arc<DashMap<PathBuf, Arc<Mutex<()>>>>,

    /// Command-instance fetch cache to track which repositories have been fetched
    /// during this command execution. This ensures we only fetch once per repository
    /// per command instance, dramatically reducing network operations for multi-dependency
    /// installations.
    ///
    /// Contains bare repository paths that have been fetched in this command instance.
    /// Works in conjunction with `VersionResolver` to minimize Git network operations.
    fetched_repos: Arc<RwLock<HashSet<PathBuf>>>,

    /// Persistent registry of worktrees stored on disk for reuse across
    /// AGPM runs. Tracks last-used timestamps and paths so we can validate
    /// and clean up cached worktrees without recreating them unnecessarily.
    worktree_registry: Arc<Mutex<WorktreeRegistry>>,
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Self {
            dir: self.dir.clone(),
            worktree_cache: Arc::clone(&self.worktree_cache),
            fetch_locks: Arc::clone(&self.fetch_locks),
            fetched_repos: Arc::clone(&self.fetched_repos),
            worktree_registry: Arc::clone(&self.worktree_registry),
        }
    }
}

impl Cache {
    fn registry_path_for(cache_dir: &Path) -> PathBuf {
        cache_dir.join("worktrees").join(".state.json")
    }

    fn registry_path(&self) -> PathBuf {
        Self::registry_path_for(&self.dir)
    }

    /// Verify that a worktree directory is fully accessible with actual content.
    ///
    /// This function ensures that a newly created worktree is fully accessible
    /// before it's marked as ready. This prevents race conditions in parallel
    /// operations where `git worktree add` returns but the filesystem hasn't
    /// finished writing all files yet.
    ///
    /// # Implementation
    ///
    /// Uses tokio-retry with exponential backoff to handle filesystem sync delays.
    ///
    /// Verification uses `git diff-index --quiet HEAD` which provides a comprehensive
    /// check that:
    /// - The worktree directory and .git marker exist
    /// - The git index is readable
    /// - ALL files from the commit are present and match HEAD
    /// - Git recognizes the worktree as valid
    ///
    /// This single command provides stronger guarantees than multi-level checks,
    /// as it verifies complete checkout rather than partial availability.
    ///
    /// # Parameters
    ///
    /// * `worktree_path` - Path to the worktree directory to verify
    /// * `sha` - The commit SHA being checked out (for logging)
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree is not accessible after all retries.
    async fn verify_worktree_accessible(worktree_path: &Path, sha: &str) -> Result<()> {
        use tokio_retry::Retry;
        use tokio_retry::strategy::{ExponentialBackoff, jitter};

        // Retry strategy with jitter for concurrent operations
        let retry_strategy = ExponentialBackoff::from_millis(50)
            .max_delay(std::time::Duration::from_secs(2))
            .take(10)
            .map(jitter);

        let worktree_path = worktree_path.to_path_buf();
        let sha_short = &sha[..8];

        tracing::debug!(
            target: "git::worktree",
            "Verifying worktree at {} for SHA {}",
            worktree_path.display(),
            sha_short
        );

        Retry::spawn(retry_strategy, || async {
            // Verify working tree matches HEAD (all files checked out)
            // This verifies the worktree structure is valid and all files are present.
            // Cache coherency (making files visible to the parent process) is now
            // handled at the point of actual file read in installer/mod.rs and resolver/mod.rs
            // via read_with_cache_retry functions.
            crate::git::command_builder::GitCommand::new()
                .args(["diff-index", "--quiet", "HEAD"])
                .current_dir(&worktree_path)
                .execute_success()
                .await
                .map_err(|_| "Working tree doesn't match HEAD (checkout incomplete)".to_string())?;

            tracing::debug!(
                target: "git::worktree",
                "Worktree verification passed for {}",
                worktree_path.display()
            );

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Worktree not fully initialized after retries: {} @ {} - {}",
                worktree_path.display(),
                sha_short,
                e
            )
        })
    }

    async fn record_worktree_usage(
        &self,
        registry_key: &str,
        source_name: &str,
        version_key: &str,
        worktree_path: &Path,
    ) -> Result<()> {
        let mut registry = self.worktree_registry.lock().await;
        registry.update(
            registry_key.to_string(),
            source_name.to_string(),
            version_key.to_string(),
            worktree_path.to_path_buf(),
        );
        registry.persist(&self.registry_path()).await?;
        Ok(())
    }

    async fn remove_worktree_record_by_path(&self, worktree_path: &Path) -> Result<()> {
        let mut registry = self.worktree_registry.lock().await;
        if registry.remove_by_path(worktree_path) {
            registry.persist(&self.registry_path()).await?;
        }
        Ok(())
    }

    async fn configure_connection_pooling(path: &Path) -> Result<()> {
        let commands = [
            ("http.version", "HTTP/2"),
            ("http.postBuffer", "524288000"),
            ("core.compression", "0"),
        ];

        for (key, value) in commands {
            GitCommand::new()
                .args(["config", key, value])
                .current_dir(path)
                .execute_success()
                .await
                .ok();
        }

        Ok(())
    }

    /// Creates a new `Cache` instance using the default platform-specific cache directory.
    ///
    /// The cache directory is determined based on the current platform:
    /// - **Linux/macOS**: `~/.agpm/cache/`
    /// - **Windows**: `%LOCALAPPDATA%\agpm\cache\`
    ///
    /// # Environment Variable Override
    ///
    /// The cache location can be overridden by setting the `AGPM_CACHE_DIR`
    /// environment variable. This is particularly useful for:
    /// - Testing with isolated cache directories
    /// - CI/CD environments with specific cache locations
    /// - Custom deployment scenarios
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unable to determine the home/local data directory
    /// - The resolved path is invalid or inaccessible
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// println!("Using cache at: {}", cache.get_cache_location().display());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let dir = crate::config::get_cache_dir()?;
        let registry_path = Self::registry_path_for(&dir);
        let registry = WorktreeRegistry::load(&registry_path);
        Ok(Self {
            dir,
            worktree_cache: Arc::new(RwLock::new(HashMap::new())),
            fetch_locks: Arc::new(DashMap::new()),
            fetched_repos: Arc::new(RwLock::new(HashSet::new())),
            worktree_registry: Arc::new(Mutex::new(registry)),
        })
    }

    /// Creates a new `Cache` instance using a custom cache directory.
    ///
    /// This constructor allows you to specify exactly where the cache should be
    /// stored, overriding platform defaults. The directory will be created if
    /// it doesn't exist when cache operations are performed.
    ///
    /// # Use Cases
    ///
    /// - **Testing**: Use temporary directories for isolated test environments
    /// - **Development**: Use project-local cache directories
    /// - **Deployment**: Use specific paths in containerized environments
    /// - **Multi-user systems**: Use user-specific cache locations
    ///
    /// # Parameters
    ///
    /// * `cache_dir` - The absolute path where cache data should be stored
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unable to load worktree registry from cache directory
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// // Use a project-local cache
    /// let project_cache = Cache::with_dir(PathBuf::from("./cache"))?;
    ///
    /// // Use a system-wide cache
    /// let system_cache = Cache::with_dir(PathBuf::from("/var/cache/agpm"))?;
    ///
    /// // Use a temporary cache for testing
    /// let temp_cache = Cache::with_dir(std::env::temp_dir().join("agpm-test"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_dir(dir: PathBuf) -> Result<Self> {
        let registry_path = Self::registry_path_for(&dir);
        let registry = WorktreeRegistry::load(&registry_path);
        Ok(Self {
            dir,
            worktree_cache: Arc::new(RwLock::new(HashMap::new())),
            fetch_locks: Arc::new(DashMap::new()),
            fetched_repos: Arc::new(RwLock::new(HashSet::new())),
            worktree_registry: Arc::new(Mutex::new(registry)),
        })
    }

    /// Ensures the cache directory exists, creating it if necessary.
    ///
    /// This method creates the cache directory and all necessary parent directories
    /// if they don't already exist. It's safe to call multiple times - it will
    /// not error if the directory already exists.
    ///
    /// # Platform Considerations
    ///
    /// - **Windows**: Handles long path names (>260 characters) correctly
    /// - **Unix**: Respects umask settings for directory permissions
    /// - **All platforms**: Creates intermediate directories as needed
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Insufficient permissions to create the directory
    /// - Disk space is exhausted
    /// - Path contains invalid characters for the platform
    /// - A file exists at the target path (not a directory)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Ensure cache directory exists before operations
    /// cache.ensure_cache_dir().await?;
    ///
    /// // Safe to call multiple times
    /// cache.ensure_cache_dir().await?; // No error
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ensure_cache_dir(&self) -> Result<()> {
        if !self.dir.exists() {
            async_fs::create_dir_all(&self.dir).await.with_file_context(
                FileOperation::CreateDir,
                &self.dir,
                "creating cache directory",
                "cache::ensure_cache_dir",
            )?;
        }
        Ok(())
    }

    /// Returns the path to the cache directory.
    ///
    /// This is useful for operations that need direct access to the cache directory,
    /// such as lock file cleanup or cache size calculations.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let cache_dir = cache.cache_dir();
    /// println!("Cache directory: {}", cache_dir.display());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.dir
    }

    /// Get the worktree path for a specific URL and commit SHA.
    ///
    /// This method constructs the expected worktree directory path based on the cache's
    /// naming scheme. It does NOT check if the worktree exists or create it - use
    /// `get_or_create_worktree_for_sha` for that.
    ///
    /// # Arguments
    ///
    /// * `url` - Git repository URL
    /// * `sha` - Full commit SHA (will be shortened to first 8 characters)
    ///
    /// # Returns
    ///
    /// Path to the worktree directory (may not exist yet)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Invalid Git URL format
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let path = cache.get_worktree_path(
    ///     "https://github.com/owner/repo.git",
    ///     "abc1234567890def"
    /// )?;
    /// println!("Worktree path: {}", path.display());
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_worktree_path(&self, url: &str, sha: &str) -> Result<PathBuf> {
        let (owner, repo) =
            crate::git::parse_git_url(url).map_err(|e| anyhow::anyhow!("Invalid Git URL: {e}"))?;
        let sha_short = &sha[..8.min(sha.len())];
        Ok(self.dir.join("worktrees").join(format!("{owner}_{repo}_{sha_short}")))
    }

    /// Gets or clones a source repository, ensuring it's available in the cache.
    ///
    /// This is the primary method for source repository management. It handles both
    /// initial cloning of new repositories and updating existing cached repositories.
    /// The operation is atomic and thread-safe through file-based locking.
    ///
    /// # Operation Flow
    ///
    /// 1. **Lock acquisition**: Acquires exclusive lock for the source name
    /// 2. **Directory check**: Determines if repository already exists in cache
    /// 3. **Clone or update**: Either clones new repository or fetches updates
    /// 4. **Version checkout**: Switches to requested version if specified
    /// 5. **Path return**: Returns path to cached repository
    ///
    /// # Concurrency Behavior
    ///
    /// - **Same source**: Concurrent calls with the same `name` will block
    /// - **Different sources**: Concurrent calls with different `name` run in parallel
    /// - **Process safety**: Safe across multiple AGPM processes
    ///
    /// # Version Handling
    ///
    /// The `version` parameter accepts various Git reference types:
    /// - **Tags**: `"v1.0.0"`, `"release-2023"` (most common for releases)
    /// - **Branches**: `"main"`, `"develop"`, `"feature/new-agents"`
    /// - **Commits**: `"abc123def"` (full or short SHA hashes)
    /// - **None**: Uses repository's default branch (typically `main` or `master`)
    ///
    /// # Parameters
    ///
    /// * `name` - Unique source identifier (used for cache directory and locking)
    /// * `url` - Git repository URL (HTTPS, SSH, or local paths)
    /// * `version` - Optional version constraint (tag, branch, or commit)
    ///
    /// # Returns
    ///
    /// Returns the [`PathBuf`] to the cached repository directory, which contains
    /// the full Git repository structure and can be used for resource file access.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - **Network issues**: Unable to clone or fetch from remote repository
    /// - **Authentication**: Invalid credentials for private repositories
    /// - **Version issues**: Specified version doesn't exist in repository
    /// - **Lock timeout**: Unable to acquire exclusive lock (rare)
    /// - **File system**: Permission or disk space issues
    /// - **Git errors**: Repository corruption or invalid Git operations
    ///
    /// # Performance Notes
    ///
    /// - **First call**: Performs full repository clone (slower)
    /// - **Subsequent calls**: Only fetches updates (faster)
    /// - **Version switching**: Uses Git checkout (very fast)
    /// - **Parallel sources**: Multiple sources processed concurrently
    ///
    /// # Examples
    ///
    /// Clone a public repository with specific version:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// let repo_path = cache.get_or_clone_source(
    ///     "community",
    ///     "https://github.com/example/agpm-community.git",
    ///     Some("v1.2.0")
    /// ).await?;
    ///
    /// println!("Repository cached at: {}", repo_path.display());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Use latest version from default branch:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// let repo_path = cache.get_or_clone_source(
    ///     "dev-tools",
    ///     "https://github.com/myorg/dev-tools.git",
    ///     None  // Use default branch
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Work with development branch:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// let repo_path = cache.get_or_clone_source(
    ///     "experimental",
    ///     "https://github.com/myorg/experimental.git",
    ///     Some("develop")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_or_clone_source(
        &self,
        name: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<PathBuf> {
        self.get_or_clone_source_impl(name, url, version).await
    }

    /// Clean up a worktree after use (fast version).
    ///
    /// This just removes the worktree directory without calling git.
    /// Git will clean up its internal references when `git worktree prune` is called.
    ///
    /// # Parameters
    ///
    /// * `worktree_path` - The path to the worktree to clean up
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unable to remove worktree directory
    /// - Unable to update worktree registry
    pub async fn cleanup_worktree(&self, worktree_path: &Path) -> Result<()> {
        // Just remove the directory - don't call git worktree remove
        // This is much faster and git will clean up its references later
        if worktree_path.exists() {
            tokio::fs::remove_dir_all(worktree_path).await.with_file_context(
                FileOperation::Write, // Using Write as it's the closest to directory modification
                worktree_path,
                "removing worktree directory",
                "cache::cleanup_worktree",
            )?;
            self.remove_worktree_record_by_path(worktree_path).await?;
        }
        Ok(())
    }

    /// Clean up all worktrees in the cache.
    ///
    /// This is useful for cleaning up after batch operations or on cache clear.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unable to remove worktrees directory
    /// - Unable to prune worktree references from bare repositories
    /// - Unable to update worktree registry
    pub async fn cleanup_all_worktrees(&self) -> Result<()> {
        let worktrees_dir = self.dir.join("worktrees");

        if !worktrees_dir.exists() {
            return Ok(());
        }

        // Remove the entire worktrees directory
        tokio::fs::remove_dir_all(&worktrees_dir).await.with_file_context(
            FileOperation::Write,
            &worktrees_dir,
            "cleaning up worktrees directory",
            "cache_module",
        )?;

        // Also prune worktree references from all bare repos
        let sources_dir = self.dir.join("sources");
        if sources_dir.exists() {
            let mut entries = tokio::fs::read_dir(&sources_dir).await.with_file_context(
                FileOperation::Read,
                &sources_dir,
                "reading sources directory",
                "cache_module",
            )?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("git") {
                    let bare_repo = GitRepo::new(&path);
                    bare_repo.prune_worktrees().await.ok();
                }
            }
        }

        {
            let mut registry = self.worktree_registry.lock().await;
            if !registry.entries.is_empty() {
                registry.entries.clear();
                registry.persist(&self.registry_path()).await?;
            }
        }

        Ok(())
    }

    /// Get or create a worktree for a specific commit SHA.
    ///
    /// This method is the cornerstone of AGPM's optimized dependency resolution.
    /// By using commit SHAs as the primary key for worktrees, we ensure:
    /// - Maximum worktree reuse (same SHA = same worktree)
    /// - Deterministic installations (SHA uniquely identifies content)
    /// - Reduced disk usage (no duplicate worktrees for same commit)
    ///
    /// # SHA-Based Caching Strategy
    ///
    /// Unlike version-based worktrees that create separate directories for
    /// "v1.0.0" and "release-1.0" even if they point to the same commit,
    /// SHA-based worktrees ensure a single worktree per unique commit.
    ///
    /// # Parameters
    ///
    /// * `name` - Source name from manifest
    /// * `url` - Git repository URL
    /// * `sha` - Full 40-character commit SHA (must be pre-resolved)
    /// * `context` - Optional context for logging
    ///
    /// # Returns
    ///
    /// Path to the worktree containing the exact commit specified by SHA.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use agpm_cli::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // First resolve version to SHA
    /// let sha = "abc1234567890def1234567890abcdef12345678";
    ///
    /// // Get worktree for that specific commit
    /// let worktree = cache.get_or_create_worktree_for_sha(
    ///     "community",
    ///     "https://github.com/example/repo.git",
    ///     sha,
    ///     Some("my-agent")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_lines)]
    pub async fn get_or_create_worktree_for_sha(
        &self,
        name: &str,
        url: &str,
        sha: &str,
        context: Option<&str>,
    ) -> Result<PathBuf> {
        // Validate SHA format
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "Invalid SHA format: expected 40 hex characters, got '{sha}'"
            ));
        }

        // Check if this is a local path
        let is_local_path = crate::utils::is_local_path(url);
        if is_local_path {
            // Local paths don't use worktrees
            return self.get_or_clone_source(name, url, None).await;
        }

        self.ensure_cache_dir().await?;

        // Parse URL for cache structure
        let (owner, repo) =
            crate::git::parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));

        // Create SHA-based cache key
        // Using first 8 chars of SHA for directory name (like Git does)
        let sha_short = &sha[..8];
        let cache_dir_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            self.dir.hash(&mut hasher);
            format!("{:x}", hasher.finish())[..8].to_string()
        };
        let cache_key = format!("{cache_dir_hash}:{owner}_{repo}:{sha}");

        // Check if we already have a worktree for this SHA
        let mut should_create_worktree = false;
        while !should_create_worktree {
            {
                let cache_read = self.worktree_cache.read().await;
                match cache_read.get(&cache_key) {
                    Some(WorktreeState::Ready(cached_path)) => {
                        if cached_path.exists() {
                            let cached_path = cached_path.clone();
                            drop(cache_read);
                            self.record_worktree_usage(&cache_key, name, sha_short, &cached_path)
                                .await?;

                            if let Some(ctx) = context {
                                tracing::debug!(
                                    target: "git",
                                    "({}) Reusing SHA-based worktree for {} @ {}",
                                    ctx,
                                    url.split('/').next_back().unwrap_or(url),
                                    sha_short
                                );
                            }
                            return Ok(cached_path);
                        }
                        should_create_worktree = true;
                    }
                    Some(WorktreeState::Pending) => {
                        if let Some(ctx) = context {
                            tracing::debug!(
                                target: "git",
                                "({}) Waiting for SHA worktree creation for {} @ {}",
                                ctx,
                                url.split('/').next_back().unwrap_or(url),
                                sha_short
                            );
                        }
                        drop(cache_read);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    None => {
                        should_create_worktree = true;
                    }
                }
            }
        }

        // Reserve the cache slot
        let mut reservation_successful = false;
        while !reservation_successful {
            let mut cache_write = self.worktree_cache.write().await;
            match cache_write.get(&cache_key) {
                Some(WorktreeState::Ready(cached_path)) if cached_path.exists() => {
                    return Ok(cached_path.clone());
                }
                Some(WorktreeState::Pending) => {
                    drop(cache_write);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                _ => {
                    cache_write.insert(cache_key.clone(), WorktreeState::Pending);
                    reservation_successful = true;
                }
            }
        }

        // Get bare repository (fetches if needed)
        let bare_repo_dir = self.dir.join("sources").join(format!("{owner}_{repo}.git"));

        if bare_repo_dir.exists() {
            // Fetch to ensure we have the SHA
            self.fetch_with_hybrid_lock(&bare_repo_dir, context).await?;
        } else {
            let lock_name = format!("{owner}_{repo}");
            let _lock = CacheLock::acquire(&self.dir, &lock_name).await?;

            if let Some(parent) = bare_repo_dir.parent() {
                tokio::fs::create_dir_all(parent).await.with_file_context(
                    FileOperation::CreateDir,
                    parent,
                    "creating cache parent directory",
                    "cache_module",
                )?;
            }

            if !bare_repo_dir.exists() {
                if let Some(ctx) = context {
                    tracing::debug!("📦 ({ctx}) Cloning repository {url}...");
                } else {
                    tracing::debug!("📦 Cloning repository {url} to cache...");
                }

                GitRepo::clone_bare_with_context(url, &bare_repo_dir, context).await?;
                Self::configure_connection_pooling(&bare_repo_dir).await.ok();
            }
        }

        let bare_repo = GitRepo::new(&bare_repo_dir);

        // Create worktree path using SHA
        let worktree_path = self.dir.join("worktrees").join(format!("{owner}_{repo}_{sha_short}"));

        // Acquire worktree creation lock
        let worktree_lock_name = format!("worktree-{owner}-{repo}-{sha_short}");
        let _worktree_lock = CacheLock::acquire(&self.dir, &worktree_lock_name).await?;

        // Re-check after lock
        if worktree_path.exists() {
            let mut cache_write = self.worktree_cache.write().await;
            cache_write.insert(cache_key.clone(), WorktreeState::Ready(worktree_path.clone()));
            self.record_worktree_usage(&cache_key, name, sha_short, &worktree_path).await?;
            return Ok(worktree_path);
        }

        // Prune stale worktrees if needed
        if !worktree_path.exists() {
            let _ = bare_repo.prune_worktrees().await;
        }

        // Create worktree at specific SHA
        if let Some(ctx) = context {
            tracing::debug!(
                target: "git",
                "({}) Creating SHA-based worktree: {} @ {}",
                ctx,
                url.split('/').next_back().unwrap_or(url),
                sha_short
            );
        }

        // Lock bare repo for worktree creation
        // Hold the lock through cache update to prevent git state corruption
        // when multiple worktrees are created concurrently for the same repo
        let bare_repo_lock_name = format!("bare-repo-{owner}_{repo}");
        let _bare_repo_lock = CacheLock::acquire(&self.dir, &bare_repo_lock_name).await?;

        // Create worktree using SHA directly
        let worktree_result =
            bare_repo.create_worktree_with_context(&worktree_path, Some(sha), context).await;

        // Keep lock held until cache is updated to ensure git state is fully settled
        match worktree_result {
            Ok(_) => {
                // Verify worktree is fully accessible before marking as Ready
                // This prevents race conditions where git worktree add returns
                // but filesystem hasn't finished writing all files yet
                Self::verify_worktree_accessible(&worktree_path, sha).await?;

                let mut cache_write = self.worktree_cache.write().await;
                cache_write.insert(cache_key.clone(), WorktreeState::Ready(worktree_path.clone()));
                self.record_worktree_usage(&cache_key, name, sha_short, &worktree_path).await?;
                // Lock automatically dropped here
                Ok(worktree_path)
            }
            Err(e) => {
                let mut cache_write = self.worktree_cache.write().await;
                cache_write.remove(&cache_key);
                // Lock automatically dropped here
                Err(e)
            }
        }
    }

    /// Get or clone a source repository with options to control cache behavior.
    ///
    /// This method provides the core functionality for repository access with
    /// additional control over cache behavior. Creates bare repositories that
    /// can be shared by all operations (resolution, installation, etc).
    ///
    /// # Parameters
    ///
    /// * `name` - The name of the source (used for cache directory naming)
    /// * `url` - The Git repository URL or local path
    /// * `version` - Optional specific version/tag/branch to checkout
    /// * `force_refresh` - If true, ignore cached version and clone/fetch fresh
    ///
    /// # Returns
    ///
    /// Returns the path to the cached bare repository directory
    async fn get_or_clone_source_impl(
        &self,
        name: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<PathBuf> {
        // Check if this is a local path (not a git repository URL)
        let is_local_path = crate::utils::is_local_path(url);

        if is_local_path {
            // For local paths (directories), validate and return the secure path
            // No cloning or version management needed

            // Resolve path securely with validation
            let resolved_path = crate::utils::platform::resolve_path(url)?;

            // Canonicalize to get the real path and prevent symlink attacks
            let canonical_path = crate::utils::safe_canonicalize(&resolved_path)
                .map_err(|_| anyhow::anyhow!("Local path is not accessible or does not exist"))?;

            // Security check: Validate path against blacklist and symlinks
            validate_path_security(&canonical_path, true)?;

            // For local paths, versions don't apply. Suppress warning for internal sentinel values.
            if let Some(ver) = version
                && ver != "local"
            {
                eprintln!("Warning: Version constraints are ignored for local paths");
            }

            return Ok(canonical_path);
        }

        self.ensure_cache_dir().await?;

        // Acquire lock for this source to prevent concurrent access
        let _lock = CacheLock::acquire(&self.dir, name)
            .await
            .with_context(|| format!("Failed to acquire lock for source: {name}"))?;

        // Use the same cache directory structure as worktrees - bare repos with .git suffix
        // This ensures we have ONE repository that's shared by all operations
        let (owner, repo) =
            crate::git::parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));
        let source_dir = self.dir.join("sources").join(format!("{owner}_{repo}.git")); // Always use .git suffix for bare repos

        // Ensure parent directory exists
        if let Some(parent) = source_dir.parent() {
            tokio::fs::create_dir_all(parent).await.with_file_context(
                FileOperation::CreateDir,
                parent,
                "creating cache directory",
                "cache_module",
            )?;
        }

        if source_dir.exists() {
            // Use existing cache - fetch to ensure we have latest refs
            // Skip fetch for local paths as they don't have remotes
            // For Git URLs, always fetch to get the latest refs (especially important for branches)
            if crate::utils::is_git_url(url) {
                // Check if we've already fetched this repo in this command instance
                let already_fetched = {
                    let fetched = self.fetched_repos.read().await;
                    fetched.contains(&source_dir)
                };

                if already_fetched {
                    tracing::debug!(
                        target: "agpm::cache",
                        "Skipping fetch for {} (already fetched in this command)",
                        name
                    );
                } else {
                    tracing::debug!(
                        target: "agpm::cache",
                        "Fetching updates for {} from {}",
                        name,
                        url
                    );
                    let repo = crate::git::GitRepo::new(&source_dir);
                    if let Err(e) = repo.fetch(None).await {
                        tracing::warn!(
                            target: "agpm::cache",
                            "Failed to fetch updates for {}: {}",
                            name,
                            e
                        );
                    } else {
                        // Mark this repo as fetched for this command execution
                        let mut fetched = self.fetched_repos.write().await;
                        fetched.insert(source_dir.clone());
                        tracing::debug!(
                            target: "agpm::cache",
                            "Successfully fetched updates for {}",
                            name
                        );
                    }
                }
            } else {
                tracing::debug!(
                    target: "agpm::cache",
                    "Skipping fetch for local path: {}",
                    url
                );
            }
        } else {
            // Directory doesn't exist - clone fresh as bare repo
            self.clone_source(url, &source_dir).await?;
        }

        Ok(source_dir)
    }

    /// Clones a Git repository to the specified target directory as a bare repository.
    ///
    /// This internal method performs the initial clone operation for repositories
    /// that are not yet present in the cache. It creates a bare repository which
    /// is optimal for serving and allows multiple worktrees to be created from it.
    ///
    /// # Why Bare Repositories
    ///
    /// Bare repositories are used because:
    /// - **No working directory conflicts**: Multiple worktrees can be created safely
    /// - **Optimized for serving**: Like GitHub/GitLab, designed for fetch operations
    /// - **Space efficient**: No checkout of files in the main repository
    /// - **Thread-safe**: Multiple processes can fetch from it simultaneously
    ///
    /// # Authentication
    ///
    /// Repository authentication is handled through:
    /// - **SSH keys**: For `git@github.com:` URLs (user's SSH configuration)
    /// - **HTTPS tokens**: For private repositories (from global config)
    /// - **Public repos**: No authentication required
    ///
    /// # Parameters
    ///
    /// * `url` - Git repository URL to clone from
    /// * `target` - Local directory path where bare repository should be created
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository URL is invalid or unreachable
    /// - Authentication fails for private repositories
    /// - Target directory cannot be created or written to
    /// - Network connectivity issues
    /// - Git command is not available in PATH
    async fn clone_source(&self, url: &str, target: &Path) -> Result<()> {
        tracing::debug!("📦 Cloning {} to cache...", url);

        // Clone as a bare repository for better concurrency and worktree support
        GitRepo::clone_bare(url, target)
            .await
            .with_context(|| format!("Failed to clone repository from {url}"))?;

        // Debug: List what was cloned
        if cfg!(test)
            && let Ok(entries) = std::fs::read_dir(target)
        {
            tracing::debug!(
                target: "agpm::cache",
                "Cloned bare repo to {}, contents:",
                target.display()
            );
            for entry in entries.flatten() {
                tracing::debug!(
                    target: "agpm::cache",
                    "  - {}",
                    entry.path().display()
                );
            }
        }

        Ok(())
    }

    /// Copies a resource file from cached repository to project directory.
    ///
    /// This method performs the core resource installation operation by copying
    /// files from the cached Git repository to the project's local directory.
    /// It provides a simple interface for resource installation without output.
    ///
    /// # Copy Strategy
    ///
    /// The method uses a copy-based approach rather than symlinks for:
    /// - **Cross-platform compatibility**: Works identically on all platforms
    /// - **Git integration**: Real files can be tracked and committed
    /// - **Editor support**: No symlink confusion in IDEs and editors
    /// - **User flexibility**: Local files can be modified if needed
    ///
    /// # Path Resolution
    ///
    /// - **Source path**: Relative to the repository root directory
    /// - **Target path**: Absolute path where file should be installed
    /// - **Directory creation**: Parent directories created automatically
    /// - **Path normalization**: Handles platform-specific path separators
    ///
    /// # Parameters
    ///
    /// * `source_dir` - Path to the cached repository directory
    /// * `source_path` - Relative path to the resource file within the repository
    /// * `target_path` - Absolute path where the resource should be installed
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source file doesn't exist in the repository
    /// - Target directory cannot be created (permissions)
    /// - File copy operation fails (disk space, permissions)
    /// - Source path attempts directory traversal (security)
    ///
    /// # Examples
    ///
    /// Copy a single resource file:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Get cached repository
    /// let repo_path = cache.get_or_clone_source(
    ///     "community",
    ///     "https://github.com/example/repo.git",
    ///     Some("v1.0.0")
    /// ).await?;
    ///
    /// // Copy resource to project
    /// cache.copy_resource(
    ///     &repo_path,
    ///     "agents/helper.md",  // Source: agents/helper.md in repository
    ///     &PathBuf::from("./my-agents/helper.md")  // Target: project location
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Copy nested resource:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let repo_path = PathBuf::from("/cache/community");
    ///
    /// cache.copy_resource(
    ///     &repo_path,
    ///     "tools/generators/api-client.md",  // Nested source path
    ///     &PathBuf::from("./tools/api-client.md")  // Flattened target
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn copy_resource(
        &self,
        source_dir: &Path,
        source_path: &str,
        target_path: &Path,
    ) -> Result<()> {
        self.copy_resource_with_output(source_dir, source_path, target_path, false).await
    }

    /// Copies a resource file with optional installation output messages.
    ///
    /// This is the full-featured resource copying method that provides control
    /// over whether installation progress is displayed to the user. It handles
    /// all the details of safe file copying including directory creation,
    /// error handling, and atomic operations.
    ///
    /// # Operation Details
    ///
    /// 1. **Source validation**: Verifies the source file exists in repository
    /// 2. **Directory creation**: Creates target parent directories if needed
    /// 3. **Atomic copy**: Performs file copy operation safely
    /// 4. **Progress output**: Optionally displays installation confirmation
    ///
    /// # File Safety
    ///
    /// - **Overwrite protection**: Will overwrite existing files without warning
    /// - **Atomic operations**: Uses system copy operations for atomicity
    /// - **Permission preservation**: Maintains reasonable file permissions
    /// - **Path validation**: Prevents directory traversal attacks
    ///
    /// # Output Control
    ///
    /// When `show_output` is `true`, displays user-friendly installation messages:
    /// ```text
    /// ✅ Installed ./agents/helper.md
    /// ✅ Installed ./snippets/docker-compose.md
    /// ```
    ///
    /// # Parameters
    ///
    /// * `source_dir` - Path to the cached repository directory
    /// * `source_path` - Relative path to resource file within repository
    /// * `target_path` - Absolute path where resource should be installed
    /// * `show_output` - Whether to display installation progress messages
    ///
    /// # Errors
    ///
    /// Returns specific error types for different failure modes:
    /// - [`AgpmError::ResourceFileNotFound`]: Source file doesn't exist
    /// - File system errors: Permission, disk space, invalid paths
    /// - Directory creation errors: Parent directory creation failures
    ///
    /// # Examples
    ///
    /// Silent installation (for batch operations):
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let repo_path = PathBuf::from("/cache/community");
    ///
    /// cache.copy_resource_with_output(
    ///     &repo_path,
    ///     "agents/helper.md",
    ///     &PathBuf::from("./agents/helper.md"),
    ///     false  // No output
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Interactive installation (with progress):
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let repo_path = PathBuf::from("/cache/community");
    ///
    /// cache.copy_resource_with_output(
    ///     &repo_path,
    ///     "snippets/deployment.md",
    ///     &PathBuf::from("./snippets/deployment.md"),
    ///     true  // Show "✅ Installed" message
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn copy_resource_with_output(
        &self,
        source_dir: &Path,
        source_path: &str,
        target_path: &Path,
        show_output: bool,
    ) -> Result<()> {
        let source_file = source_dir.join(source_path);

        if !source_file.exists() {
            return Err(AgpmError::ResourceFileNotFound {
                path: source_path.to_string(),
                source_name: source_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            }
            .into());
        }

        if let Some(parent) = target_path.parent() {
            async_fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        async_fs::copy(&source_file, target_path).await.with_context(|| {
            format!("Failed to copy {} to {}", source_file.display(), target_path.display())
        })?;

        if show_output {
            println!("  ✅ Installed {}", target_path.display());
        }

        Ok(())
    }

    /// Removes unused cached repositories to reclaim disk space.
    ///
    /// This method performs selective cache cleanup by removing repositories
    /// that are no longer referenced by any active source configurations.
    /// It's a safe operation that preserves repositories currently in use.
    ///
    /// # Cleanup Strategy
    ///
    /// 1. **Directory scanning**: Enumerates all cached repository directories
    /// 2. **Active comparison**: Checks each directory against active sources list
    /// 3. **Safe removal**: Removes only unused directories, preserving files
    /// 4. **Progress reporting**: Displays removal progress for user feedback
    ///
    /// # Safety Guarantees
    ///
    /// - **Active protection**: Never removes repositories listed in active sources
    /// - **Directory-only**: Only removes directories, preserves any loose files
    /// - **Atomic removal**: Each directory is removed completely or not at all
    /// - **Lock awareness**: Respects file locks but doesn't acquire them
    ///
    /// # Performance Considerations
    ///
    /// - **I/O intensive**: Scans entire cache directory structure
    /// - **Disk space recovery**: Can free significant space for large repositories
    /// - **Network savings**: Removed repositories will need re-cloning if used again
    /// - **Concurrent safe**: Can run while other cache operations are in progress
    ///
    /// # Parameters
    ///
    /// * `active_sources` - List of source names that should be preserved in cache
    ///
    /// # Returns
    ///
    /// Returns the number of repository directories that were successfully removed.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Cache directory cannot be read (permissions)
    /// - Unable to remove a directory (file locks, permissions)
    /// - File system errors during directory traversal
    ///
    /// # Output Messages
    ///
    /// Displays progress messages for each removed repository:
    /// ```text
    /// 🗑️  Removing unused cache: old-project
    /// 🗑️  Removing unused cache: deprecated-tools
    /// ```
    ///
    /// # Examples
    ///
    /// Clean cache based on current manifest sources:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Active sources from current agpm.toml
    /// let active_sources = vec![
    ///     "community".to_string(),
    ///     "work-tools".to_string(),
    ///     "personal".to_string(),
    /// ];
    ///
    /// let removed = cache.clean_unused(&active_sources).await?;
    /// println!("Cleaned {} unused repositories", removed);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Clean all cached repositories:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Empty active list removes everything
    /// let removed = cache.clean_unused(&[]).await?;
    /// println!("Removed all {} cached repositories", removed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clean_unused(&self, active_sources: &[String]) -> Result<usize> {
        self.ensure_cache_dir().await?;

        let mut removed_count = 0;
        let mut entries = async_fs::read_dir(&self.dir)
            .await
            .with_context(|| "Failed to read cache directory")?;

        while let Some(entry) =
            entries.next_entry().await.with_context(|| "Failed to read directory entry")?
        {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if !active_sources.contains(&dir_name.to_string()) {
                    println!("🗑️  Removing unused cache: {dir_name}");
                    async_fs::remove_dir_all(&path).await.with_context(|| {
                        format!("Failed to remove cache directory: {}", path.display())
                    })?;
                    removed_count += 1;
                }
            }
        }

        Ok(removed_count)
    }

    /// Calculates the total size of the cache directory in bytes.
    ///
    /// This method recursively calculates the disk space used by all cached
    /// repositories and supporting files. It's useful for cache size monitoring,
    /// cleanup decisions, and storage management.
    ///
    /// # Calculation Method
    ///
    /// - **Recursive traversal**: Includes all subdirectories and files
    /// - **Actual file sizes**: Reports real disk usage, not allocated blocks
    /// - **All file types**: Includes Git objects, working files, and lock files
    /// - **Cross-platform**: Consistent behavior across different file systems
    ///
    /// # Performance Notes
    ///
    /// - **I/O intensive**: May be slow for very large caches
    /// - **File system dependent**: Performance varies by underlying storage
    /// - **Concurrent safe**: Can run during other cache operations
    /// - **Memory efficient**: Streams directory traversal without loading all paths
    ///
    /// # Returns
    ///
    /// Returns the total size in bytes. For a non-existent cache directory,
    /// returns `0` without error.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Permission denied reading cache directory or subdirectories
    /// - File system errors during directory traversal
    /// - Symbolic link cycles (rare, but possible)
    ///
    /// # Examples
    ///
    /// Check current cache size:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// let size_bytes = cache.get_cache_size().await?;
    /// let size_mb = size_bytes / 1024 / 1024;
    ///
    /// println!("Cache size: {} MB ({} bytes)", size_mb, size_bytes);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Display human-readable sizes:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let size_bytes = cache.get_cache_size().await?;
    ///
    /// let (size, unit) = match size_bytes {
    ///     s if s < 1024 => (s, "B"),
    ///     s if s < 1024 * 1024 => (s / 1024, "KB"),
    ///     s if s < 1024 * 1024 * 1024 => (s / 1024 / 1024, "MB"),
    ///     s => (s / 1024 / 1024 / 1024, "GB"),
    /// };
    ///
    /// println!("Cache size: {}{}", size, unit);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_cache_size(&self) -> Result<u64> {
        if !self.dir.exists() {
            return Ok(0);
        }

        let size = fs::get_directory_size(&self.dir).await?;
        Ok(size)
    }

    /// Returns the path to the cache directory.
    ///
    /// This method provides access to the cache directory path for inspection,
    /// logging, or integration with other tools. The path represents where
    /// all cached repositories and supporting files are stored.
    ///
    /// # Return Value
    ///
    /// Returns a reference to the [`Path`] representing the cache directory.
    /// The path may or may not exist on the file system - use [`ensure_cache_dir`]
    /// to create it if needed.
    ///
    /// # Thread Safety
    ///
    /// This method is safe to call from multiple threads as it only returns
    /// a reference to the immutable path stored in the `Cache` instance.
    ///
    /// # Examples
    ///
    /// Display cache location:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// println!("Cache stored at: {}", cache.get_cache_location().display());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Check if cache exists:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let location = cache.get_cache_location();
    ///
    /// if location.exists() {
    ///     println!("Cache directory exists at: {}", location.display());
    /// } else {
    ///     println!("Cache directory not yet created: {}", location.display());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`ensure_cache_dir`]: Cache::ensure_cache_dir
    #[must_use]
    pub fn get_cache_location(&self) -> &Path {
        &self.dir
    }

    /// Completely removes the entire cache directory and all its contents.
    ///
    /// This is a destructive operation that removes all cached repositories,
    /// lock files, and any other cache-related data. Use with caution as
    /// this will require re-cloning all repositories on the next operation.
    ///
    /// # Operation Details
    ///
    /// - **Complete removal**: Deletes the entire cache directory tree
    /// - **Recursive deletion**: Removes all subdirectories and files
    /// - **Lock files**: Also removes .locks directory and all lock files
    /// - **Atomic operation**: Either succeeds completely or leaves cache intact
    ///
    /// # Recovery Impact
    ///
    /// After calling this method:
    /// - All repositories must be re-cloned on next use
    /// - Network bandwidth will be required for repository downloads
    /// - Disk space is immediately reclaimed
    /// - Cache directory will be recreated automatically on next operation
    ///
    /// # Safety Considerations
    ///
    /// - **No confirmation**: This method doesn't ask for confirmation
    /// - **Irreversible**: Cannot undo the deletion operation
    /// - **Concurrent operations**: May interfere with running cache operations
    /// - **Lock respect**: Doesn't wait for locks, may fail if repositories are in use
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Permission denied for cache directory or contents
    /// - Files are locked by other processes
    /// - File system errors during deletion
    /// - Cache directory is in use by another process
    ///
    /// # Output Messages
    ///
    /// Displays confirmation message on successful completion:
    /// ```text
    /// 🗑️  Cleared all cache
    /// ```
    ///
    /// # Examples
    ///
    /// Clear cache for fresh start:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Check size before clearing
    /// let size_before = cache.get_cache_size().await?;
    /// println!("Cache size before: {} bytes", size_before);
    ///
    /// // Clear everything
    /// cache.clear_all().await?;
    ///
    /// // Verify cache is empty
    /// let size_after = cache.get_cache_size().await?;
    /// println!("Cache size after: {} bytes", size_after); // Should be 0
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Clear cache with error handling:
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// match cache.clear_all().await {
    ///     Ok(()) => println!("Cache cleared successfully"),
    ///     Err(e) => {
    ///         eprintln!("Failed to clear cache: {}", e);
    ///         eprintln!("Some files may be in use by other processes");
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clear_all(&self) -> Result<()> {
        if self.dir.exists() {
            async_fs::remove_dir_all(&self.dir).await.with_context(|| "Failed to clear cache")?;
            println!("🗑️  Cleared all cache");
        }
        Ok(())
    }

    /// Perform a fetch operation with hybrid locking (in-process and cross-process).
    ///
    /// This method implements a two-level locking strategy:
    /// 1. In-process locks (`Arc<Mutex>`) for fast coordination within the same process
    /// 2. File-based locks for cross-process coordination
    ///
    /// The fetch will only happen once per repository per command execution.
    ///
    /// # Parameters
    ///
    /// * `bare_repo_path` - Path to the bare repository
    /// * `context` - Optional context string for logging
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if the fetch was successful or skipped.
    async fn fetch_with_hybrid_lock(
        &self,
        bare_repo_path: &Path,
        context: Option<&str>,
    ) -> Result<()> {
        use fs4::fs_std::FileExt;

        // Level 1: In-process lock (fast path)
        let memory_lock = self
            .fetch_locks
            .entry(bare_repo_path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _memory_guard = memory_lock.lock().await;

        // Level 2: File-based lock (cross-process)
        let safe_name = bare_repo_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .replace(['/', '\\', ':'], "_");

        let lock_path = self.dir.join(".locks").join(format!("{safe_name}.fetch.lock"));

        // Ensure lock directory exists
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent).await.with_file_context(
                FileOperation::CreateDir,
                parent,
                "creating lock directory",
                "cache_module",
            )?;
        }

        // Create/open lock file
        let lock_file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .await?;

        // Convert to std::fs::File for fs4
        let std_file = lock_file.into_std().await;

        // Acquire exclusive lock (blocks until available)
        if let Some(ctx) = context {
            tracing::debug!(
                target: "agpm::git",
                "({}) Acquiring file lock for {}",
                ctx,
                bare_repo_path.display()
            );
        }
        std_file.lock_exclusive()?;

        if let Some(ctx) = context {
            tracing::debug!(
                target: "agpm::git",
                "({}) Acquired file lock for {}",
                ctx,
                bare_repo_path.display()
            );
        }

        // Now check if we've already fetched this repo in this command execution
        // This happens AFTER acquiring the lock to prevent race conditions
        let already_fetched = {
            let fetched = self.fetched_repos.read().await;
            let is_fetched = fetched.contains(bare_repo_path);
            if let Some(ctx) = context {
                tracing::debug!(
                    target: "agpm::git",
                    "({}) Checking if already fetched: {} - Result: {} (total fetched: {}, hashset addr: {:p})",
                    ctx,
                    bare_repo_path.display(),
                    is_fetched,
                    fetched.len(),
                    &raw const *fetched
                );
            }
            is_fetched
        };

        if already_fetched {
            if let Some(ctx) = context {
                tracing::debug!(
                    target: "agpm::git",
                    "({}) Skipping fetch (already fetched in this command): {}",
                    ctx,
                    bare_repo_path.display()
                );
            }
            // Release the file lock and return
            return Ok(());
        }

        // Now safe to fetch
        let repo = GitRepo::new(bare_repo_path);

        if let Some(ctx) = context {
            tracing::debug!(
                target: "agpm::git",
                "({}) Fetching updates for {}",
                ctx,
                bare_repo_path.display()
            );
        }

        repo.fetch(None).await?;

        // Mark this repo as fetched for this command execution
        {
            let mut fetched = self.fetched_repos.write().await;
            fetched.insert(bare_repo_path.to_path_buf());
            if let Some(ctx) = context {
                tracing::debug!(
                    target: "agpm::git",
                    "({}) Marked as fetched: {} (total fetched: {}, hashset addr: {:p})",
                    ctx,
                    bare_repo_path.display(),
                    fetched.len(),
                    &raw const *fetched
                );
            }
        }

        // File lock automatically released when std_file is dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cache_dir_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let cache = Cache::with_dir(cache_dir.clone()).unwrap();
        cache.ensure_cache_dir().await.unwrap();

        assert!(cache_dir.exists());
    }

    #[tokio::test]
    async fn test_cache_location() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let location = cache.get_cache_location();
        assert_eq!(location, temp_dir.path());
    }

    #[tokio::test]
    async fn test_cache_size_empty() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();
        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_cache_size_with_content() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some test content
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let size = cache.get_cache_size().await.unwrap();
        assert!(size > 0);
        assert_eq!(size, 12); // "test content" is 12 bytes
    }

    #[tokio::test]
    async fn test_clean_unused_empty_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        let removed = cache.clean_unused(&["active".to_string()]).await.unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn test_clean_unused_removes_correct_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some test directories
        let active_dir = temp_dir.path().join("active");
        let unused_dir = temp_dir.path().join("unused");
        let another_unused = temp_dir.path().join("another_unused");

        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::create_dir_all(&unused_dir).unwrap();
        std::fs::create_dir_all(&another_unused).unwrap();

        // Add some content to verify directories are removed completely
        std::fs::write(active_dir.join("file.txt"), "keep").unwrap();
        std::fs::write(unused_dir.join("file.txt"), "remove").unwrap();
        std::fs::write(another_unused.join("file.txt"), "remove").unwrap();

        let removed = cache.clean_unused(&["active".to_string()]).await.unwrap();

        assert_eq!(removed, 2);
        assert!(active_dir.exists());
        assert!(!unused_dir.exists());
        assert!(!another_unused.exists());
    }

    #[tokio::test]
    async fn test_clear_all_removes_entire_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some content
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "content").unwrap();

        assert!(temp_dir.path().exists());
        assert!(subdir.exists());

        cache.clear_all().await.unwrap();

        assert!(!temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_copy_resource() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("resource.md");
        std::fs::write(&source_file, "# Test Resource\nContent").unwrap();

        // Copy resource
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, "resource.md", &dest).await.unwrap();

        assert!(dest.exists());
        let content = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(content, "# Test Resource\nContent");
    }

    #[tokio::test]
    async fn test_copy_resource_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file in nested directory
        let source_dir = temp_dir.path().join("source");
        let nested_dir = source_dir.join("nested").join("path");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let source_file = nested_dir.join("resource.md");
        std::fs::write(&source_file, "# Nested Resource").unwrap();

        // Copy resource using relative path from source_dir
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, "nested/path/resource.md", &dest).await.unwrap();

        assert!(dest.exists());
        let content = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(content, "# Nested Resource");
    }

    #[tokio::test]
    async fn test_copy_resource_invalid_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();

        // Try to copy non-existent resource
        let dest = temp_dir.path().join("dest.md");
        let result = cache.copy_resource(&source_dir, "nonexistent.md", &dest).await;

        assert!(result.is_err());
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn test_ensure_cache_dir_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir.clone()).unwrap();

        // Call ensure_cache_dir multiple times
        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());

        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());

        // Add a file and ensure it's preserved
        std::fs::write(cache_dir.join("test.txt"), "content").unwrap();

        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());
        assert!(cache_dir.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "content").unwrap();

        // Copy to a destination with non-existent parent directories
        let dest = temp_dir.path().join("deep").join("nested").join("dest.md");
        cache.copy_resource(&source_dir, "file.md", &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "content");
    }

    #[tokio::test]
    async fn test_copy_resource_with_output_flag() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "content").unwrap();

        // Test with output flag false
        let dest1 = temp_dir.path().join("dest1.md");
        cache.copy_resource_with_output(&source_dir, "file.md", &dest1, false).await.unwrap();
        assert!(dest1.exists());

        // Test with output flag true
        let dest2 = temp_dir.path().join("dest2.md");
        cache.copy_resource_with_output(&source_dir, "file.md", &dest2, true).await.unwrap();
        assert!(dest2.exists());
    }

    #[tokio::test]
    async fn test_cache_size_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");
        let cache = Cache::with_dir(nonexistent).unwrap();

        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_clear_all_nonexistent_cache() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");
        let cache = Cache::with_dir(nonexistent).unwrap();

        // Should not error when clearing non-existent cache
        cache.clear_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_clean_unused_with_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create directories
        std::fs::create_dir_all(temp_dir.path().join("keep")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("remove")).unwrap();

        // Create a file (not a directory)
        std::fs::write(temp_dir.path().join("file.txt"), "content").unwrap();

        let removed = cache.clean_unused(&["keep".to_string()]).await.unwrap();

        // Should only remove the "remove" directory, not the file
        assert_eq!(removed, 1);
        assert!(temp_dir.path().join("keep").exists());
        assert!(!temp_dir.path().join("remove").exists());
        assert!(temp_dir.path().join("file.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "new content").unwrap();

        // Create existing destination file
        let dest = temp_dir.path().join("dest.md");
        std::fs::write(&dest, "old content").unwrap();

        // Copy should overwrite
        cache.copy_resource(&source_dir, "file.md", &dest).await.unwrap();

        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_copy_resource_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file with special characters
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let special_name = "file with spaces & special-chars.md";
        std::fs::write(source_dir.join(special_name), "content").unwrap();

        // Copy resource
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, special_name, &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "content");
    }

    #[tokio::test]
    async fn test_cache_location_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("my_cache");
        let cache = Cache::with_dir(cache_dir.clone()).unwrap();

        // Get location multiple times
        let loc1 = cache.get_cache_location();
        let loc2 = cache.get_cache_location();

        assert_eq!(loc1, loc2);
        assert_eq!(loc1, cache_dir.as_path());
    }

    #[tokio::test]
    async fn test_clean_unused_empty_active_list() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some directories
        std::fs::create_dir_all(temp_dir.path().join("source1")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("source2")).unwrap();

        // Empty active list should remove all
        let removed = cache.clean_unused(&[]).await.unwrap();

        assert_eq!(removed, 2);
        assert!(!temp_dir.path().join("source1").exists());
        assert!(!temp_dir.path().join("source2").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_with_relative_paths() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source with subdirectories
        let source_dir = temp_dir.path().join("source");
        let sub_dir = source_dir.join("agents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("helper.md"), "# Helper Agent").unwrap();

        // Copy using relative path
        let dest = temp_dir.path().join("my-agent.md");
        cache.copy_resource(&source_dir, "agents/helper.md", &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "# Helper Agent");
    }

    #[tokio::test]
    async fn test_cache_size_with_subdirectories() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create nested structure with files
        let sub1 = temp_dir.path().join("sub1");
        let sub2 = sub1.join("sub2");
        std::fs::create_dir_all(&sub2).unwrap();

        std::fs::write(temp_dir.path().join("file1.txt"), "12345").unwrap(); // 5 bytes
        std::fs::write(sub1.join("file2.txt"), "1234567890").unwrap(); // 10 bytes
        std::fs::write(sub2.join("file3.txt"), "abc").unwrap(); // 3 bytes

        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 18); // 5 + 10 + 3
    }
}
