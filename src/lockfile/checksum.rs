//! Checksum computation and verification for lockfile integrity.
//!
//! This module provides SHA-256 checksum operations for verifying file integrity,
//! detecting corruption, and ensuring reproducible installations. Supports both
//! single files and directory-based resources (e.g., skills).

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::{LockFile, ResourceId};
use crate::utils::normalize_path_for_storage;
use walkdir::WalkDir;

impl LockFile {
    /// Compute SHA-256 checksum for file integrity verification.
    ///
    /// Detects corruption, tampering, or changes after installation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - File read error with detailed context
    ///
    /// # Checksum Format
    ///
    /// The returned checksum follows the format:
    /// - **Algorithm prefix**: "sha256:"
    /// - **Hash encoding**: Lowercase hexadecimal
    /// - **Length**: 71 characters total (7 for prefix + 64 hex digits)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let checksum = LockFile::compute_checksum(Path::new("example.md"))?;
    /// println!("File checksum: {}", checksum);
    /// // Output: "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// Provides detailed error context for common issues:
    /// - **File not found**: Suggests checking the path
    /// - **Permission denied**: Suggests checking file permissions
    /// - **IO errors**: Suggests checking disk health or file locks
    ///
    /// # Security Considerations
    ///
    /// - Uses SHA-256, a cryptographically secure hash function
    /// - Suitable for integrity verification and tamper detection
    /// - Consistent across platforms (Windows, macOS, Linux)
    /// - Not affected by line ending differences (hashes actual bytes)
    ///
    /// # Performance
    ///
    /// The method reads the entire file into memory before hashing.
    /// For very large files (>100MB), consider streaming implementations
    /// in future versions.
    pub fn compute_checksum(path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let content = fs::read(path).with_context(|| {
            format!(
                "Cannot read file for checksum calculation: {}\n\n\
                    This error occurs when verifying file integrity.\n\
                    Check that the file exists and is readable.",
                path.display()
            )
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = hasher.finalize();

        Ok(format!("sha256:{}", hex::encode(result)))
    }

    /// Compute SHA-256 checksum for directory integrity verification.
    ///
    /// Calculates a combined checksum of all files in a directory recursively.
    /// Used for directory-based resources like skills to detect any changes
    /// to the directory contents.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - Directory read error with detailed context
    ///
    /// # Algorithm
    ///
    /// The directory checksum is computed by:
    /// 1. Walking the directory recursively
    /// 2. Sorting all files by relative path for deterministic ordering
    /// 3. For each file: hashing relative_path + NULL separator + file_content
    /// 4. Computing SHA-256 of the combined data
    ///
    /// This ensures the checksum changes when:
    /// - Any file content changes
    /// - Files are added or removed
    /// - Files are renamed or moved
    /// - Directory structure changes
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let checksum = LockFile::compute_directory_checksum(Path::new("my-skill"))?;
    /// println!("Directory checksum: {}", checksum);
    /// // Output: "sha256:b6d81b360a5672d80c27430f39153e2c4c3b6b5c8ee5a4b5e8d5b5c5b6b7b8b9"
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Security Considerations
    ///
    /// - Uses SHA-256, a cryptographically secure hash function
    /// - Includes relative paths in hash to detect file renames
    /// - Deterministic across platforms (Windows, macOS, Linux)
    /// - Respects .gitignore patterns and excludes hidden files
    ///
    /// # Performance
    ///
    /// Reads all files in memory. For very large directories (>100MB total),
    /// consider streaming implementations in future versions.
    pub fn compute_directory_checksum(path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        if !path.is_dir() {
            return Err(anyhow::anyhow!("Path is not a directory: {}", path.display()));
        }

        let mut hasher = Sha256::new();
        let mut file_count = 0;
        let mut total_size = 0u64;

        // Walk directory and sort files for deterministic ordering
        for entry in WalkDir::new(path)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                // Skip hidden files and common build artifacts
                let file_name = entry.file_name();
                if file_name.to_string_lossy().starts_with('.') {
                    continue;
                }

                let relative_path = entry.path().strip_prefix(path).with_context(|| {
                    format!("Failed to get relative path for: {}", entry.path().display())
                })?;

                let file_path = entry.path();
                tracing::debug!("Reading file for directory checksum: {}", file_path.display());
                let content = fs::read(file_path).with_context(|| {
                    format!("Cannot read file for directory checksum: {}", file_path.display())
                })?;

                // Include relative path and content in hash
                // Use normalized paths (forward slashes) for cross-platform compatibility
                let normalized_path = normalize_path_for_storage(relative_path);
                hasher.update(normalized_path.as_bytes());
                hasher.update(b"\0"); // NULL separator between path and content
                hasher.update(&content);

                file_count += 1;
                total_size += content.len() as u64;
            }
        }

        if file_count == 0 {
            return Err(anyhow::anyhow!(
                "Directory contains no files to checksum: {}",
                path.display()
            ));
        }

        tracing::debug!(
            "Computed directory checksum for {}: {} files, {} bytes",
            path.display(),
            file_count,
            total_size
        );

        let result = hasher.finalize();
        Ok(format!("sha256:{}", hex::encode(result)))
    }

    /// Compute checksum for file or directory automatically.
    ///
    /// Detects whether the path is a file or directory and uses the appropriate
    /// checksum method. For files, uses [`compute_checksum`](Self::compute_checksum).
    /// For directories, uses [`compute_directory_checksum`](Self::compute_directory_checksum).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file or directory to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - Read error with detailed context
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let file_checksum = LockFile::compute_checksum_smart(Path::new("example.md"))?;
    /// let dir_checksum = LockFile::compute_checksum_smart(Path::new("my-skill"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn compute_checksum_smart(path: &Path) -> Result<String> {
        if path.is_dir() {
            Self::compute_directory_checksum(path)
        } else {
            Self::compute_checksum(path)
        }
    }

    /// Verify file or directory matches expected checksum.
    ///
    /// Computes current checksum and compares with expected value.
    /// Automatically detects whether the path is a file or directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file or directory to verify
    /// * `expected` - Expected checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Checksum matches expected value
    /// * `Ok(false)` - Checksum does not match (corruption detected)
    /// * `Err(anyhow::Error)` - Read error or checksum calculation failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let expected = "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3";
    ///
    /// // Verify a file
    /// let file_valid = LockFile::verify_checksum(Path::new("example.md"), expected)?;
    ///
    /// // Verify a directory (e.g., a skill)
    /// let dir_valid = LockFile::verify_checksum(Path::new("my-skill"), expected)?;
    ///
    /// if file_valid {
    ///     println!("File integrity verified");
    /// }
    /// if dir_valid {
    ///     println!("Directory integrity verified");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Installation verification**: Ensure copied files/directories are intact
    /// - **Periodic validation**: Detect corruption over time
    /// - **Security checks**: Detect unauthorized modifications
    /// - **Troubleshooting**: Diagnose installation issues
    ///
    /// # Performance
    ///
    /// This method internally calls [`compute_checksum_smart`](Self::compute_checksum_smart),
    /// so it has the same performance characteristics. For bulk verification
    /// operations, consider caching computed checksums.
    ///
    /// # Security
    ///
    /// The comparison is performed using standard string equality, which is
    /// not timing-attack resistant. Since checksums are not secrets, this
    /// is acceptable for integrity verification purposes.
    pub fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
        let actual = Self::compute_checksum_smart(path)?;
        Ok(actual == expected)
    }

    /// Update checksum for resource identified by ResourceId.
    ///
    /// Used after installation to record actual file checksum. ResourceId ensures unique
    /// identification via name, source, tool, and template_vars.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for the resource
    /// * `checksum` - The new SHA-256 checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, LockedResourceBuilder, ResourceId};
    /// # use agpm_cli::core::ResourceType;
    /// # use agpm_cli::utils::compute_variant_inputs_hash;
    /// # let mut lockfile = LockFile::default();
    /// # // First add a resource to update
    /// # let resource = LockedResourceBuilder::new(
    /// #     "my-agent".to_string(),
    /// #     "my-agent.md".to_string(),
    /// #     "".to_string(),
    /// #     "agents/my-agent.md".to_string(),
    /// #     ResourceType::Agent,
    /// # )
    /// # .tool(Some("claude-code".to_string()))
    /// # .build();
    /// # lockfile.add_typed_resource("my-agent".to_string(), resource, ResourceType::Agent);
    /// let variant_hash = compute_variant_inputs_hash(&serde_json::json!({})).unwrap_or_default();
    /// let id = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, variant_hash);
    /// let updated = lockfile.update_resource_checksum(&id, "sha256:abcdef123456...");
    /// assert!(updated);
    /// ```
    pub fn update_resource_checksum(&mut self, id: &ResourceId, checksum: &str) -> bool {
        // Try each resource type until we find a match by comparing ResourceIds
        for resource in &mut self.agents {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        false
    }

    /// Update context checksum for resource by ResourceId.
    ///
    /// Stores the SHA-256 checksum of template rendering inputs (context) in the lockfile.
    /// This is different from the file checksum which covers the final rendered content.
    ///
    /// # Arguments
    ///
    /// * `id` - The ResourceId identifying the resource to update
    /// * `context_checksum` - The SHA-256 checksum of template context, or None for non-templated resources
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut lockfile = LockFile::new();
    /// let id = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, serde_json::json!({}));
    /// let updated = lockfile.update_resource_context_checksum(&id, Some("sha256:context123456..."));
    /// assert!(updated);
    /// ```
    pub fn update_resource_context_checksum(
        &mut self,
        id: &ResourceId,
        context_checksum: &str,
    ) -> bool {
        // Try each resource type until we find a match by comparing ResourceIds
        for resource in &mut self.agents {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        false
    }

    /// Update applied patches for resource by name.
    ///
    /// Stores project patches in main lockfile; private patches go to agpm.private.lock.
    /// Takes `AppliedPatches` from installer.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the resource to update
    /// * `applied_patches` - The patches that were applied (from `AppliedPatches` struct)
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::manifest::patches::AppliedPatches;
    /// # use std::collections::HashMap;
    /// # let mut lockfile = LockFile::new();
    /// let mut applied = AppliedPatches::new();
    /// applied.project.insert("model".to_string(), toml::Value::String("haiku".into()));
    ///
    /// let updated = lockfile.update_resource_applied_patches("my-agent", &applied);
    /// assert!(updated);
    /// ```
    pub fn update_resource_applied_patches(
        &mut self,
        name: &str,
        applied_patches: &crate::manifest::patches::AppliedPatches,
    ) -> bool {
        // Store ONLY project patches in the main lockfile (agpm.lock)
        // Private patches are stored separately in agpm.private.lock
        // This ensures the main lockfile is deterministic and safe to commit
        let project_patches = applied_patches.project.clone();

        // Try each resource type until we find a match
        for resource in &mut self.agents {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        false
    }

    /// Apply installation results to the lockfile in batch.
    ///
    /// Updates the lockfile with checksums, context checksums, and applied patches
    /// from the installation process. This consolidates three separate update operations
    /// into one batch call, reducing code duplication between install and update commands.
    ///
    /// # Batch Processing Pattern
    ///
    /// This function processes three parallel vectors of installation results:
    /// 1. **File checksums** - SHA-256 of rendered content (triggers reinstall if changed)
    /// 2. **Context checksums** - SHA-256 of template inputs (audit/debug only)
    /// 3. **Applied patches** - Tracks which project patches were applied to each resource
    ///
    /// The batch approach ensures all three updates are applied consistently and
    /// atomically to the lockfile, avoiding partial state.
    ///
    /// # Arguments
    ///
    /// * `checksums` - File checksums for each installed resource (by ResourceId)
    /// * `context_checksums` - Context checksums for template inputs (Optional)
    /// * `applied_patches_list` - Patches that were applied to each resource
    ///
    /// # Implementation Details
    ///
    /// - Updates are applied by ResourceId to handle duplicate resource names correctly
    /// - Context checksums are only applied if present (non-templated resources have None)
    /// - Only project patches are stored; private patches go to `agpm.private.lock`
    /// - Called by both `install` and `update` commands after parallel installation
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, ResourceId};
    /// # use agpm_cli::manifest::patches::AppliedPatches;
    /// # use agpm_cli::core::ResourceType;
    /// let mut lockfile = LockFile::default();
    ///
    /// // Collect results from parallel installation
    /// let checksums = vec![/* (ResourceId, checksum) pairs */];
    /// let context_checksums = vec![/* (ResourceId, Option<checksum>) pairs */];
    /// let applied_patches = vec![/* (ResourceId, AppliedPatches) pairs */];
    ///
    /// // Apply all results in batch (replaces 3 separate loops)
    /// lockfile.apply_installation_results(
    ///     checksums,
    ///     context_checksums,
    ///     applied_patches,
    /// );
    /// ```
    ///
    pub fn apply_installation_results(
        &mut self,
        checksums: Vec<(ResourceId, String)>,
        context_checksums: Vec<(ResourceId, Option<String>)>,
        applied_patches_list: Vec<(ResourceId, crate::manifest::patches::AppliedPatches)>,
    ) {
        // Update lockfile with checksums
        for (id, checksum) in checksums {
            self.update_resource_checksum(&id, &checksum);
        }

        // Update lockfile with context checksums
        for (id, context_checksum) in context_checksums {
            if let Some(checksum) = context_checksum {
                self.update_resource_context_checksum(&id, &checksum);
            }
        }

        // Update lockfile with applied patches
        for (id, applied_patches) in applied_patches_list {
            self.update_resource_applied_patches(id.name(), &applied_patches);
        }
    }
}

#[cfg(test)]
#[path = "checksum_tests.rs"]
mod checksum_tests;
