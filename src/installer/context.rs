//! Installation context and helper utilities.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::lockfile::LockFile;
use crate::manifest::Manifest;

/// Installation context containing common parameters for resource installation.
///
/// This struct bundles frequently-used installation parameters to reduce
/// function parameter counts and improve code readability. It's used throughout
/// the installation pipeline to pass configuration and context information.
///
/// # Fields
///
/// * `project_dir` - Root directory of the project where resources will be installed
/// * `cache` - Cache instance for managing Git repositories and worktrees
/// * `force_refresh` - Whether to force refresh of cached worktrees
/// * `manifest` - Optional reference to the project manifest for template context
/// * `lockfile` - Optional reference to the lockfile for template context
/// * `old_lockfile` - Optional reference to the previous lockfile for early-exit optimization
/// * `project_patches` - Optional project-level patches from agpm.toml
/// * `private_patches` - Optional user-level patches from agpm.private.toml
pub struct InstallContext<'a> {
    pub project_dir: &'a Path,
    pub cache: &'a Cache,
    pub force_refresh: bool,
    pub verbose: bool,
    pub manifest: Option<&'a Manifest>,
    pub lockfile: Option<&'a Arc<LockFile>>,
    pub old_lockfile: Option<&'a LockFile>,
    pub project_patches: Option<&'a crate::manifest::ManifestPatches>,
    pub private_patches: Option<&'a crate::manifest::ManifestPatches>,
    pub gitignore_lock: Option<&'a Arc<Mutex<()>>>,
    pub max_content_file_size: Option<u64>,
    /// Shared template context builder for all resources
    pub template_context_builder: Arc<crate::templating::TemplateContextBuilder>,
}

impl<'a> InstallContext<'a> {
    /// Create a new installation context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_dir: &'a Path,
        cache: &'a Cache,
        force_refresh: bool,
        verbose: bool,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
        old_lockfile: Option<&'a LockFile>,
        project_patches: Option<&'a crate::manifest::ManifestPatches>,
        private_patches: Option<&'a crate::manifest::ManifestPatches>,
        gitignore_lock: Option<&'a Arc<Mutex<()>>>,
        max_content_file_size: Option<u64>,
    ) -> Self {
        // Create shared template context builder
        // Use lockfile if available, otherwise create with empty lockfile
        let (lockfile_for_builder, project_config) = if let Some(lf) = lockfile {
            (lf.clone(), manifest.and_then(|m| m.project.clone()))
        } else {
            // No lockfile - create an empty one for the builder
            (Arc::new(LockFile::default()), None)
        };

        let template_context_builder = Arc::new(crate::templating::TemplateContextBuilder::new(
            lockfile_for_builder,
            project_config,
            Arc::new(cache.clone()),
            project_dir.to_path_buf(),
        ));

        Self {
            project_dir,
            cache,
            force_refresh,
            verbose,
            manifest,
            lockfile,
            old_lockfile,
            project_patches,
            private_patches,
            gitignore_lock,
            max_content_file_size,
            template_context_builder,
        }
    }
}

/// Read a file with retry logic to handle cross-process filesystem cache coherency issues.
///
/// This function wraps `tokio::fs::read_to_string` with retry logic to handle cases where
/// files created by Git subprocesses are not immediately visible to the parent Rust process
/// due to filesystem cache propagation delays. This is particularly important in CI
/// environments with network-attached storage where cache coherency delays can be significant.
///
/// # Arguments
///
/// * `path` - The file path to read
///
/// # Returns
///
/// Returns the file content as a `String`, or an error if the file cannot be read after retries.
///
/// # Retry Strategy
///
/// - Initial delay: 10ms
/// - Max delay: 500ms
/// - Factor: 2x (exponential backoff)
/// - Max attempts: 10
/// - Total max time: ~10 seconds
///
/// Only `NotFound` errors are retried, as these indicate cache coherency issues.
/// Other errors (permissions, I/O errors) fail immediately by returning Ok to bypass retry.
pub(crate) async fn read_with_cache_retry(path: &Path) -> Result<String> {
    // Handle skill directories by reading SKILL.md
    if path.is_dir() {
        let skill_md_path = path.join("SKILL.md");
        if !skill_md_path.exists() {
            return Err(anyhow::anyhow!(
                "Skill directory missing required SKILL.md file: {} (expected at: {})",
                path.display(),
                skill_md_path.display()
            ));
        }
        tracing::debug!("Reading skill directory {} via SKILL.md file", path.display());
        // Call the non-recursive helper with the SKILL.md file path
        return read_file_with_cache_retry(&skill_md_path).await;
    }

    read_file_with_cache_retry(path).await
}

/// Read a file with retry logic to handle cross-process filesystem cache coherency issues.
///
/// This is the actual retry implementation without directory handling
/// to avoid recursion.
async fn read_file_with_cache_retry(path: &Path) -> Result<String> {
    use std::io;

    let retry_strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(10)
        .max_delay(Duration::from_millis(500))
        .factor(2)
        .take(10);

    let path_buf = path.to_path_buf();

    tokio_retry::Retry::spawn(retry_strategy, || {
        let path = path_buf.clone();
        async move {
            tokio::fs::read_to_string(&path).await.map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    tracing::debug!(
                        "File not yet visible (likely cache coherency issue): {}",
                        path.display()
                    );
                    format!("File not found: {}", path.display())
                } else {
                    // Non-retriable error - return error message that will fail fast
                    format!("I/O error (non-retriable): {}", e)
                }
            })
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to read resource file: {}: {}", path.display(), e))
}
