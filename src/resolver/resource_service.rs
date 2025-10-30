//! Resource fetching service for dependency resolution.
//!
//! This service handles fetching resource content from local files or Git worktrees
//! and resolving canonical paths for dependencies.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::file_error::{FileOperation, FileResultExt};
use crate::manifest::ResourceDependency;

use super::types::ResolutionCore;
use super::version_resolver::VersionResolutionService;

/// Service for fetching resource content and resolving paths.
pub struct ResourceFetchingService;

impl ResourceFetchingService {
    /// Create a new resource fetching service.
    pub fn new() -> Self {
        Self
    }

    /// Fetch the content of a resource for metadata extraction.
    ///
    /// This method retrieves the file content from either:
    /// - Local filesystem (for path-only dependencies)
    /// - Git worktree (for Git-backed dependencies with version)
    ///
    /// This method can prepare versions on-demand if they haven't been prepared yet,
    /// which is necessary for transitive dependencies discovered during resolution.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with manifest and cache
    /// * `dep` - The resource dependency to fetch
    /// * `version_service` - Version service to get/prepare worktree paths
    /// * `resource_type` - The type of resource (optional, used for special handling like skills)
    ///
    /// # Returns
    ///
    /// The file content as a string
    pub async fn fetch_content(
        core: &ResolutionCore,
        dep: &ResourceDependency,
        version_service: &mut VersionResolutionService,
        resource_type: Option<crate::core::ResourceType>,
    ) -> Result<String> {
        match dep {
            ResourceDependency::Simple(path) => {
                // Local file - resolve relative to manifest directory
                let manifest_dir = core
                    .manifest
                    .manifest_dir
                    .as_ref()
                    .context("Manifest directory not available for local dependency")?;

                let full_path = manifest_dir.join(path);
                let canonical_path = full_path.canonicalize().with_file_context(
                    FileOperation::Canonicalize,
                    &full_path,
                    format!("resolving local dependency path: {}", path),
                    "resource_service",
                )?;

                Self::read_with_cache_retry(&canonical_path).await
            }
            ResourceDependency::Detailed(detailed) => {
                if let Some(source) = &detailed.source {
                    // Git-backed dependency
                    // Use dep.get_version() to handle branch/rev/version precedence
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = format!("{}::{}", source, version_key);

                    // Check if version is already prepared, if not prepare it on-demand
                    if version_service.get_prepared_version(&group_key).is_none() {
                        // Prepare this version on-demand (common with transitive dependencies)
                        // Use dep.get_version() to properly handle branch/rev/version precedence
                        version_service
                            .prepare_additional_version(core, source, dep.get_version())
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to prepare version on-demand for source '{}' @ '{}'",
                                    source, version_key
                                )
                            })?;
                    }

                    let prepared = version_service.get_prepared_version(&group_key).unwrap();
                    let worktree_path = &prepared.worktree_path;

                    // For skills, the path points to a directory, but we need to read SKILL.md
                    let file_path = if resource_type == Some(crate::core::ResourceType::Skill) {
                        let skill_dir = worktree_path.join(&detailed.path);
                        let skill_md = skill_dir.join("SKILL.md");

                        // Check if SKILL.md exists
                        if !skill_md.exists() {
                            return Err(anyhow::anyhow!(
                                "Skill at {} missing required SKILL.md file",
                                skill_dir.display()
                            ));
                        }

                        skill_md
                    } else {
                        worktree_path.join(&detailed.path)
                    };

                    // Don't canonicalize Git-backed files - worktrees may have coherency delays
                    Self::read_with_cache_retry(&file_path).await
                } else {
                    // Local path-only dependency
                    let manifest_dir = core
                        .manifest
                        .manifest_dir
                        .as_ref()
                        .context("Manifest directory not available")?;

                    // For skills, the path points to a directory, but we need to read SKILL.md
                    let full_path = if resource_type == Some(crate::core::ResourceType::Skill) {
                        let skill_dir = manifest_dir.join(&detailed.path);
                        let skill_md = skill_dir.join("SKILL.md");

                        // Check if SKILL.md exists
                        if !skill_md.exists() {
                            return Err(anyhow::anyhow!(
                                "Skill at {} missing required SKILL.md file",
                                skill_dir.display()
                            ));
                        }

                        skill_md
                    } else {
                        manifest_dir.join(&detailed.path)
                    };

                    let canonical_path = full_path.canonicalize().with_file_context(
                        FileOperation::Canonicalize,
                        &full_path,
                        format!("resolving local dependency path: {}", detailed.path),
                        "resource_service",
                    )?;

                    Self::read_with_cache_retry(&canonical_path).await
                }
            }
        }
    }

    /// Get the canonical path for a dependency.
    ///
    /// Resolves dependency path to its canonical form on the filesystem.
    /// Can prepare versions on-demand if needed.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with manifest and cache
    /// * `dep` - The resource dependency
    /// * `version_service` - Version service to get/prepare worktree paths
    ///
    /// # Returns
    ///
    /// The canonical absolute path to the resource
    pub async fn get_canonical_path(
        core: &ResolutionCore,
        dep: &ResourceDependency,
        version_service: &mut VersionResolutionService,
    ) -> Result<PathBuf> {
        match dep {
            ResourceDependency::Simple(path) => {
                let manifest_dir = core
                    .manifest
                    .manifest_dir
                    .as_ref()
                    .context("Manifest directory not available")?;

                let full_path = manifest_dir.join(path);
                full_path.canonicalize().map_err(|e| {
                    // Create a FileOperationError for canonicalization failures
                    let file_error = crate::core::file_error::FileOperationError::new(
                        crate::core::file_error::FileOperationContext::new(
                            crate::core::file_error::FileOperation::Canonicalize,
                            &full_path,
                            format!("canonicalizing local dependency path: {}", path),
                            "resource_service::get_canonical_path",
                        ),
                        e,
                    );
                    anyhow::Error::from(file_error)
                })
            }
            ResourceDependency::Detailed(detailed) => {
                if let Some(source) = &detailed.source {
                    // Git-backed dependency
                    // Use dep.get_version() to handle branch/rev/version precedence
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = format!("{}::{}", source, version_key);

                    // Check if version is already prepared, if not prepare it on-demand
                    if version_service.get_prepared_version(&group_key).is_none() {
                        version_service
                            .prepare_additional_version(core, source, detailed.version.as_deref())
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to prepare version on-demand for source '{}' @ '{}'",
                                    source, version_key
                                )
                            })?;
                    }

                    let prepared = version_service.get_prepared_version(&group_key).unwrap();

                    let worktree_path = &prepared.worktree_path;
                    let file_path = worktree_path.join(&detailed.path);

                    // Return the path without canonicalizing - Git worktrees may have coherency delays
                    Ok(file_path)
                } else {
                    // Local path-only dependency
                    let manifest_dir = core
                        .manifest
                        .manifest_dir
                        .as_ref()
                        .context("Manifest directory not available")?;

                    let full_path = manifest_dir.join(&detailed.path);
                    full_path.canonicalize().map_err(|e| {
                        // Create a FileOperationError for canonicalization failures
                        let file_error = crate::core::file_error::FileOperationError::new(
                            crate::core::file_error::FileOperationContext::new(
                                crate::core::file_error::FileOperation::Canonicalize,
                                &full_path,
                                format!("canonicalizing dependency path: {}", detailed.path),
                                "resource_service::get_canonical_path",
                            ),
                            e,
                        );
                        anyhow::Error::from(file_error)
                    })
                }
            }
        }
    }

    /// Read file with retry logic for cache coherency issues.
    ///
    /// Git worktrees can have filesystem coherency delays after creation.
    /// For skill directories, reads the SKILL.md file instead of the directory.
    async fn read_with_cache_retry(path: &Path) -> Result<String> {
        // Check if this is a skill directory
        if path.is_dir() {
            let skill_md_path = path.join("SKILL.md");
            if !skill_md_path.exists() {
                anyhow::bail!(
                    "Skill directory missing required SKILL.md file: {} (expected at: {})",
                    path.display(),
                    skill_md_path.display()
                );
            }
            tracing::debug!("Reading skill directory {} via SKILL.md file", path.display());
            let content = Self::read_file_with_retry(&skill_md_path).await?;

            // Validate skill frontmatter to provide early error detection
            crate::skills::validate_skill_frontmatter(&content).with_context(|| {
                format!("Invalid skill frontmatter in: {}", skill_md_path.display())
            })?;

            return Ok(content);
        }

        Self::read_file_with_retry(path).await
    }

    /// Read file with retry logic for cache coherency issues.
    ///
    /// This is the actual retry implementation without directory handling
    /// to avoid recursion.
    async fn read_file_with_retry(path: &Path) -> Result<String> {
        use tokio::time::{Duration, sleep};

        const MAX_ATTEMPTS: u32 = 10;
        const RETRY_DELAY_MS: u64 = 100;

        for attempt in 0..MAX_ATTEMPTS {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => return Ok(content),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound && attempt < MAX_ATTEMPTS - 1 =>
                {
                    // File not found, but we have retries left
                    tracing::debug!(
                        "File not found at {}, retrying ({}/{})",
                        path.display(),
                        attempt + 1,
                        MAX_ATTEMPTS
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }
                Err(e) => {
                    // Other error or final attempt
                    return Err(e).with_file_context(
                        FileOperation::Read,
                        path,
                        "reading dependency content in resource service",
                        "resource_service",
                    )?;
                }
            }
        }

        // This should never be reached, but provide a fallback with proper error context
        let file_error = crate::core::file_error::FileOperationError::new(
            crate::core::file_error::FileOperationContext::new(
                crate::core::file_error::FileOperation::Read,
                path,
                format!("reading file after {} attempts", MAX_ATTEMPTS),
                "resource_service::read_with_cache_retry",
            ),
            std::io::Error::new(std::io::ErrorKind::NotFound, "file not found after retries"),
        );
        Err(anyhow::Error::from(file_error))
    }
}

impl Default for ResourceFetchingService {
    fn default() -> Self {
        Self::new()
    }
}
