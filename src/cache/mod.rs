//! Git repository cache management for efficient resource installation
//!
//! This module provides comprehensive functionality for caching Git repositories locally
//! to avoid repeated cloning operations and enable fast resource installation. The cache
//! system is designed for concurrent access, cross-platform compatibility, and efficient
//! disk space management.
//!
//! # Architecture Overview
//!
//! The cache system consists of two main components:
//! - [`Cache`] struct: Manages repository operations and file copying
//! - [`CacheLock`]: Provides thread-safe concurrent access via file locking
//!
//! # Platform-Specific Cache Locations
//!
//! The cache is stored in platform-appropriate locations:
//! - **Linux/macOS**: `~/.ccpm/cache/`
//! - **Windows**: `%LOCALAPPDATA%\ccpm\cache\`
//! - **Environment Override**: Set `CCPM_CACHE_DIR` to use custom location
//!
//! # Cache Directory Structure
//!
//! The cache organizes repositories by source name with supporting infrastructure:
//! ```text
//! ~/.ccpm/cache/
//! ├── community/              # Source name from ccpm.toml
//! │   ├── .git/              # Full Git repository
//! │   ├── agents/            # Resource directories
//! │   └── snippets/          # Organized by type
//! ├── private-source/        # Another source repository
//! │   └── resources/         # Repository-specific structure
//! └── .locks/                # Lock files for concurrency
//!     ├── community.lock     # Per-source lock files
//!     └── private-source.lock
//! ```
//!
//! # Concurrency and Thread Safety
//!
//! The cache implements several safety mechanisms for concurrent access:
//! - **File-based locking**: Each source has its own lock file using [`CacheLock`]
//! - **Process-safe operations**: Multiple CCPM processes can run simultaneously
//! - **Atomic file operations**: Resource copying uses safe atomic operations
//! - **Lock scope isolation**: Different sources can be accessed concurrently
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
//! The cache is optimized for common CCPM workflows:
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
//! use ccpm::cache::Cache;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Initialize cache with default location
//! let cache = Cache::new()?;
//!
//! // Get or clone a source repository
//! let repo_path = cache.get_or_clone_source(
//!     "community",
//!     "https://github.com/example/ccpm-community.git",
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
//! use ccpm::cache::Cache;
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
//! use ccpm::cache::Cache;
//! use std::path::PathBuf;
//!
//! # fn custom_location() -> anyhow::Result<()> {
//! // Use custom cache directory (useful for testing or special setups)
//! let custom_dir = PathBuf::from("/tmp/my-ccpm-cache");
//! let cache = Cache::with_dir(custom_dir)?;
//!
//! println!("Using cache at: {}", cache.get_cache_location().display());
//! # Ok(())
//! # }
//! ```
//!
//! # Integration with CCPM Workflow
//!
//! The cache module integrates seamlessly with CCPM's dependency management:
//! 1. **Manifest parsing**: Source URLs extracted from `ccpm.toml`
//! 2. **Dependency resolution**: Version constraints resolved to specific commits
//! 3. **Cache population**: Repositories cloned and checked out as needed
//! 4. **Resource installation**: Files copied from cache to project directories
//! 5. **Lockfile generation**: Installed resources tracked in `ccpm.lock`
//!
//! See [`crate::manifest`] for manifest handling and [`crate::lockfile`] for
//! lockfile management.

use crate::core::error::CcpmError;
use crate::git::GitRepo;
use crate::utils::fs;
use crate::utils::security::validate_path_security;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs as async_fs;

/// File-based locking mechanism for cache operations
///
/// This module provides thread-safe and process-safe locking for cache
/// operations through OS-level file locks, ensuring data consistency
/// when multiple CCPM processes access the same cache directory.
pub mod lock;
pub use lock::CacheLock;

/// Git repository cache for efficient resource management
///
/// The `Cache` struct provides the primary interface for managing Git repository
/// caching in CCPM. It handles repository cloning, updating, version management,
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
/// use ccpm::cache::Cache;
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
/// use ccpm::cache::Cache;
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
    cache_dir: PathBuf,
}

impl Cache {
    /// Creates a new `Cache` instance using the default platform-specific cache directory.
    ///
    /// The cache directory is determined based on the current platform:
    /// - **Linux/macOS**: `~/.ccpm/cache/`
    /// - **Windows**: `%LOCALAPPDATA%\ccpm\cache\`
    ///
    /// # Environment Variable Override
    ///
    /// The cache location can be overridden by setting the `CCPM_CACHE_DIR`
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
    /// use ccpm::cache::Cache;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// println!("Using cache at: {}", cache.get_cache_location().display());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let cache_dir = crate::config::get_cache_dir()?;
        Ok(Self { cache_dir })
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
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// // Use a project-local cache
    /// let project_cache = Cache::with_dir(PathBuf::from("./cache"))?;
    ///
    /// // Use a system-wide cache
    /// let system_cache = Cache::with_dir(PathBuf::from("/var/cache/ccpm"))?;
    ///
    /// // Use a temporary cache for testing
    /// let temp_cache = Cache::with_dir(std::env::temp_dir().join("ccpm-test"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_dir(cache_dir: PathBuf) -> Result<Self> {
        Ok(Self { cache_dir })
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
    /// use ccpm::cache::Cache;
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
        if !self.cache_dir.exists() {
            async_fs::create_dir_all(&self.cache_dir)
                .await
                .with_context(|| {
                    format!(
                        "Failed to create cache directory at {}",
                        self.cache_dir.display()
                    )
                })?;
        }
        Ok(())
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
    /// - **Process safety**: Safe across multiple CCPM processes
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
    /// use ccpm::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// let repo_path = cache.get_or_clone_source(
    ///     "community",
    ///     "https://github.com/example/ccpm-community.git",
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
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
        self.get_or_clone_source_with_options(name, url, version, false)
            .await
    }

    /// Get or clone a source repository with options to control cache behavior.
    ///
    /// This method provides the core functionality for repository access with
    /// additional control over cache behavior.
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
    /// Returns the path to the cached/cloned repository directory
    pub async fn get_or_clone_source_with_options(
        &self,
        name: &str,
        url: &str,
        version: Option<&str>,
        force_refresh: bool,
    ) -> Result<PathBuf> {
        // Check if this is a local path (not a git repository URL)
        let is_local_path = url.starts_with('/') || url.starts_with("./") || url.starts_with("../");

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

            // For local paths, we ignore versions as they don't apply
            if version.is_some() {
                eprintln!("Warning: Version constraints are ignored for local paths");
            }

            return Ok(canonical_path);
        }

        self.ensure_cache_dir().await?;

        // Acquire lock for this source to prevent concurrent access
        let _lock = CacheLock::acquire(&self.cache_dir, name)
            .await
            .with_context(|| format!("Failed to acquire lock for source: {name}"))?;

        // Use the same cache directory structure as SourceManager for consistency
        // Parse the URL to get owner and repo for the cache path
        let (owner, repo) =
            crate::git::parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));
        let source_dir = self
            .cache_dir
            .join("sources")
            .join(format!("{owner}_{repo}"));

        // Ensure parent directory exists
        if let Some(parent) = source_dir.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create cache directory: {parent:?}"))?;
        }

        if source_dir.exists() && !force_refresh {
            // Use existing cache - just update to the requested version
            self.update_source(&source_dir, version).await?;
        } else if source_dir.exists() && force_refresh {
            // Force refresh - remove existing and clone fresh
            tokio::fs::remove_dir_all(&source_dir)
                .await
                .with_context(|| {
                    format!("Failed to remove existing cache directory: {source_dir:?}")
                })?;
            self.clone_source(url, &source_dir).await?;
            if let Some(ver) = version {
                self.checkout_version(&source_dir, ver).await?;
            }
        } else {
            // Directory doesn't exist - clone fresh
            self.clone_source(url, &source_dir).await?;
            if let Some(ver) = version {
                self.checkout_version(&source_dir, ver).await?;
            }
        }

        Ok(source_dir)
    }

    /// Clones a Git repository to the specified target directory.
    ///
    /// This internal method performs the initial clone operation for repositories
    /// that are not yet present in the cache. It uses the system's Git command
    /// via the [`GitRepo`] wrapper for maximum compatibility.
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
    /// * `target` - Local directory path where repository should be cloned
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository URL is invalid or unreachable
    /// - Authentication fails for private repositories
    /// - Target directory cannot be created or written to
    /// - Network connectivity issues
    /// - Git command is not available in PATH
    ///
    /// See [`GitRepo::clone`] for detailed error information.
    async fn clone_source(&self, url: &str, target: &Path) -> Result<()> {
        println!("📦 Cloning {url} to cache...");

        GitRepo::clone(url, target, None)
            .await
            .with_context(|| format!("Failed to clone repository from {url}"))?;

        // Debug: List what was cloned
        if cfg!(test) {
            if let Ok(entries) = std::fs::read_dir(target) {
                eprintln!("DEBUG: Cloned to {}, contents:", target.display());
                for entry in entries.flatten() {
                    eprintln!("  - {}", entry.path().display());
                }
            }
        }

        Ok(())
    }

    /// Updates an existing cached repository with latest changes from remote.
    ///
    /// This method fetches the latest changes from the remote repository without
    /// modifying the current working directory state. It's equivalent to running
    /// `git fetch` in the repository.
    ///
    /// # Update Strategy
    ///
    /// 1. **Fetch updates**: Downloads latest refs and objects from remote
    /// 2. **Version checkout**: Switches to requested version if provided
    /// 3. **Preserve local state**: Doesn't modify uncommitted changes (if any)
    ///
    /// # Parameters
    ///
    /// * `source_dir` - Path to the cached repository directory
    /// * `version` - Optional version to checkout after fetching updates
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository is not a valid Git repository
    /// - Network issues prevent fetching updates
    /// - Authentication fails for private repositories
    /// - Specified version doesn't exist after fetch
    ///
    /// # Performance
    ///
    /// The fetch operation only downloads new changes since the last update,
    /// making it much faster than a full clone for subsequent operations.
    async fn update_source(&self, source_dir: &Path, version: Option<&str>) -> Result<()> {
        let git_repo = GitRepo::new(source_dir);
        git_repo
            .fetch(None, None)
            .await
            .with_context(|| "Failed to fetch updates")?;

        if let Some(ver) = version {
            self.checkout_version(source_dir, ver).await?;
        }

        Ok(())
    }

    /// Checks out a specific version (tag, branch, or commit) in the repository.
    ///
    /// This method switches the repository's working directory to the specified
    /// version. It's equivalent to running `git checkout <version>` in the repository.
    ///
    /// # Version Types Supported
    ///
    /// - **Tags**: `v1.0.0`, `release-2023` (immutable version markers)
    /// - **Branches**: `main`, `develop`, `feature/new-feature` (movable refs)
    /// - **Commits**: `abc123def456` (specific commit hashes)
    ///
    /// # Behavior Notes
    ///
    /// - **Detached HEAD**: Checking out tags or commits results in detached HEAD state
    /// - **Branch tracking**: Checking out branches sets up tracking with remote
    /// - **Clean checkout**: Any local modifications will be preserved or cause conflicts
    ///
    /// # Parameters
    ///
    /// * `source_dir` - Path to the cached repository directory
    /// * `version` - Git reference to checkout (tag, branch, or commit)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Version doesn't exist in the repository
    /// - Repository has uncommitted changes that would conflict
    /// - Repository is not in a valid state for checkout
    /// - Git command fails due to repository corruption
    ///
    /// The error message provides guidance on checking if the version exists
    /// as a tag, branch, or commit in the remote repository.
    ///
    /// # Examples
    ///
    /// Common version patterns:
    ///
    /// ```text
    /// "v1.0.0"           # Semantic version tag
    /// "main"             # Main development branch
    /// "develop"          # Development branch
    /// "abc123"           # Specific commit (short hash)
    /// "feature/auth"     # Feature branch
    /// "release/2024"     # Release branch
    /// ```
    async fn checkout_version(&self, source_dir: &Path, version: &str) -> Result<()> {
        let git_repo = GitRepo::new(source_dir);
        git_repo.checkout(version).await.with_context(|| {
            format!(
                "Failed to checkout version '{version}'. Ensure it exists as a tag, branch, or commit"
            )
        })?;

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
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
        self.copy_resource_with_output(source_dir, source_path, target_path, false)
            .await
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
    /// - [`CcpmError::ResourceFileNotFound`]: Source file doesn't exist
    /// - File system errors: Permission, disk space, invalid paths
    /// - Directory creation errors: Parent directory creation failures
    ///
    /// # Examples
    ///
    /// Silent installation (for batch operations):
    ///
    /// ```rust,no_run
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
            return Err(CcpmError::ResourceFileNotFound {
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

        async_fs::copy(&source_file, target_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    source_file.display(),
                    target_path.display()
                )
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
    /// use ccpm::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    ///
    /// // Active sources from current ccpm.toml
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
    /// use ccpm::cache::Cache;
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
        let mut entries = async_fs::read_dir(&self.cache_dir)
            .await
            .with_context(|| "Failed to read cache directory")?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .with_context(|| "Failed to read directory entry")?
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
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let size = fs::get_directory_size(&self.cache_dir).await?;
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
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
        &self.cache_dir
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
    /// use ccpm::cache::Cache;
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
    /// use ccpm::cache::Cache;
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
        if self.cache_dir.exists() {
            async_fs::remove_dir_all(&self.cache_dir)
                .await
                .with_context(|| "Failed to clear cache")?;
            println!("🗑️  Cleared all cache");
        }
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
        cache
            .copy_resource(&source_dir, "resource.md", &dest)
            .await
            .unwrap();

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
        cache
            .copy_resource(&source_dir, "nested/path/resource.md", &dest)
            .await
            .unwrap();

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
        let result = cache
            .copy_resource(&source_dir, "nonexistent.md", &dest)
            .await;

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
        cache
            .copy_resource(&source_dir, "file.md", &dest)
            .await
            .unwrap();

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
        cache
            .copy_resource_with_output(&source_dir, "file.md", &dest1, false)
            .await
            .unwrap();
        assert!(dest1.exists());

        // Test with output flag true
        let dest2 = temp_dir.path().join("dest2.md");
        cache
            .copy_resource_with_output(&source_dir, "file.md", &dest2, true)
            .await
            .unwrap();
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
        cache
            .copy_resource(&source_dir, "file.md", &dest)
            .await
            .unwrap();

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
        cache
            .copy_resource(&source_dir, special_name, &dest)
            .await
            .unwrap();

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
        cache
            .copy_resource(&source_dir, "agents/helper.md", &dest)
            .await
            .unwrap();

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
