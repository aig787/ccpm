//! Dependency resolution and conflict detection for AGPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, transitive dependency resolution,
//! parallel source synchronization, and relative path preservation during installation.
//!
//! # Service-Based Architecture
//!
//! This resolver has been refactored to use a service-based architecture:
//! - **ResolutionCore**: Shared immutable state
//! - **VersionResolutionService**: Git operations and version resolution
//! - **PatternExpansionService**: Glob pattern expansion
//! - **TransitiveDependencyService**: Transitive dependency resolution
//! - **ConflictService**: Conflict detection
//! - **ResourceFetchingService**: Resource content fetching

// Declare service modules
pub mod conflict_service;
pub mod dependency_graph;
pub mod lockfile_builder;
pub mod path_resolver;
pub mod pattern_expander;
pub mod resource_service;
pub mod skills;
pub mod source_context;
pub mod transitive_resolver;
pub mod types;
pub mod version_resolver;

// Re-export utility functions for compatibility
pub use path_resolver::{extract_meaningful_path, is_file_relative_path, normalize_bare_filename};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::cache::Cache;
use crate::core::{OperationContext, ResourceType};
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::source::SourceManager;

// Re-export services for external use
pub use conflict_service::ConflictService;
pub use pattern_expander::PatternExpansionService;
pub use resource_service::ResourceFetchingService;
pub use types::ResolutionCore;
pub use version_resolver::{
    VersionResolutionService, VersionResolver as VersionResolverExport, find_best_matching_tag,
    is_version_constraint, parse_tags_to_versions,
};

// Legacy re-exports for compatibility
pub use dependency_graph::{DependencyGraph, DependencyNode};
pub use lockfile_builder::LockfileBuilder;
pub use pattern_expander::{expand_pattern_to_concrete_deps, generate_dependency_name};
pub use types::{
    DependencyKey, ManifestOverride, ManifestOverrideIndex, OverrideKey, ResolutionContext,
    TransitiveContext,
};

pub use version_resolver::{PreparedSourceVersion, VersionResolver, WorktreeManager};

/// Main dependency resolver with service-based architecture.
///
/// This orchestrates multiple specialized services to handle different aspects
/// of the dependency resolution process while maintaining compatibility
/// with existing interfaces.
pub struct DependencyResolver {
    /// Core shared context with immutable state
    core: ResolutionCore,

    /// Version resolution and Git operations service
    version_service: VersionResolutionService,

    /// Pattern expansion service for glob dependencies
    pattern_service: PatternExpansionService,

    /// Conflict detector for version conflicts
    conflict_detector: crate::version::conflict::ConflictDetector,

    /// Dependency tracking state
    dependency_map: HashMap<DependencyKey, Vec<String>>,

    /// Pattern alias tracking for expanded patterns
    pattern_alias_map: HashMap<(ResourceType, String), String>,

    /// Transitive dependency custom names
    transitive_custom_names: HashMap<DependencyKey, String>,

    /// Track if sources have been pre-synced to avoid duplicate work
    sources_pre_synced: bool,
}

impl DependencyResolver {
    /// Create a new dependency resolver.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn new(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_context(manifest, cache, None).await
    }

    /// Create a new dependency resolver with operation context.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    /// * `operation_context` - Optional context for warning deduplication
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn new_with_context(
        manifest: Manifest,
        cache: Cache,
        operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        // Create source manager from manifest
        let source_manager = SourceManager::from_manifest(&manifest)?;

        // Create resolution core with shared state
        let core = ResolutionCore::new(manifest, cache, source_manager, operation_context);

        // Initialize all services
        let version_service = VersionResolutionService::new(core.cache().clone());
        let pattern_service = PatternExpansionService::new();

        Ok(Self {
            core,
            version_service,
            pattern_service,
            conflict_detector: crate::version::conflict::ConflictDetector::new(),
            dependency_map: HashMap::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            sources_pre_synced: false,
        })
    }

    /// Create a new resolver with global configuration support.
    ///
    /// This loads both manifest sources and global sources from `~/.agpm/config.toml`.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if global configuration cannot be loaded
    pub async fn new_with_global(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_global_context(manifest, cache, None).await
    }

    /// Creates a new dependency resolver with custom cache directory.
    ///
    /// # Arguments
    ///
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn with_cache(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_context(manifest, cache, None).await
    }

    /// Create a new resolver with global configuration and operation context.
    ///
    /// This loads both manifest sources and global sources from `~/.agpm/config.toml`.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    /// * `operation_context` - Optional context for warning deduplication
    ///
    /// # Errors
    ///
    /// Returns an error if global configuration cannot be loaded
    pub async fn new_with_global_context(
        manifest: Manifest,
        cache: Cache,
        _operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        let source_manager = SourceManager::from_manifest_with_global(&manifest).await?;

        let core = ResolutionCore::new(manifest, cache, source_manager, _operation_context);

        let version_service = VersionResolutionService::new(core.cache().clone());
        let pattern_service = PatternExpansionService::new();

        Ok(Self {
            core,
            version_service,
            pattern_service,
            conflict_detector: crate::version::conflict::ConflictDetector::new(),
            dependency_map: HashMap::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            sources_pre_synced: false,
        })
    }

    /// Get a reference to the resolution core.
    pub fn core(&self) -> &ResolutionCore {
        &self.core
    }

    /// Resolve all dependencies and generate a complete lockfile.
    ///
    /// This is the main resolution method.
    ///
    /// # Errors
    ///
    /// Returns an error if any step of resolution fails
    pub async fn resolve(&mut self) -> Result<LockFile> {
        self.resolve_with_options(true).await
    }

    /// Resolve dependencies with transitive resolution option.
    ///
    /// # Arguments
    ///
    /// * `enable_transitive` - Whether to resolve transitive dependencies
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails
    pub async fn resolve_with_options(&mut self, enable_transitive: bool) -> Result<LockFile> {
        let mut lockfile = LockFile::new();

        // Add sources to lockfile
        for (name, url) in &self.core.manifest().sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Phase 1: Extract dependencies from manifest with types
        let base_deps: Vec<(String, ResourceDependency, ResourceType)> = self
            .core
            .manifest()
            .all_dependencies_with_types()
            .into_iter()
            .map(|(name, dep, resource_type)| (name.to_string(), dep.into_owned(), resource_type))
            .collect();

        // Add direct dependencies to conflict detector
        for (name, dep, _) in &base_deps {
            self.add_to_conflict_detector(name, dep, "manifest");
        }

        // Phase 2: Pre-sync all sources if not already done
        if !self.sources_pre_synced {
            let deps_for_sync: Vec<(String, ResourceDependency)> =
                base_deps.iter().map(|(name, dep, _)| (name.clone(), dep.clone())).collect();
            self.version_service.pre_sync_sources(&self.core, &deps_for_sync).await?;
            self.sources_pre_synced = true;
        }

        // Phase 3: Resolve transitive dependencies
        let all_deps = if enable_transitive {
            self.resolve_transitive_dependencies(&base_deps).await?
        } else {
            base_deps.clone()
        };

        // Phase 4: Resolve each dependency to a locked resource
        for (name, dep, resource_type) in &all_deps {
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(name, dep, *resource_type).await?;

                // Add each resolved entry with deduplication
                for entry in entries {
                    let entry_name = entry.name.clone();
                    self.add_or_update_lockfile_entry(&mut lockfile, &entry_name, entry);
                }
            } else {
                // Regular single dependency
                let entry = self.resolve_dependency(name, dep, *resource_type).await?;
                self.add_or_update_lockfile_entry(&mut lockfile, name, entry);
            }
        }

        // Phase 5: Detect conflicts
        let conflicts = self.conflict_detector.detect_conflicts();
        if !conflicts.is_empty() {
            let mut error_msg = String::from("Version conflicts detected:\n\n");
            for conflict in &conflicts {
                error_msg.push_str(&format!("{conflict}\n"));
            }
            return Err(anyhow::anyhow!("{}", error_msg));
        }

        // Phase 6: Post-process dependencies and detect target conflicts
        self.add_version_to_dependencies(&mut lockfile)?;
        self.detect_target_conflicts(&lockfile)?;

        Ok(lockfile)
    }

    /// Pre-sync sources for the given dependencies.
    ///
    /// This performs Git operations to ensure all required sources are available
    /// before the main resolution process begins.
    ///
    /// # Arguments
    ///
    /// * `deps` - List of (name, dependency) pairs to sync sources for
    ///
    /// # Errors
    ///
    /// Returns an error if source synchronization fails
    pub async fn pre_sync_sources(&mut self, deps: &[(String, ResourceDependency)]) -> Result<()> {
        // Pre-sync all sources using version service
        self.version_service.pre_sync_sources(&self.core, deps).await?;
        self.sources_pre_synced = true;
        Ok(())
    }

    /// Update dependencies with existing lockfile and specific dependencies to update.
    ///
    /// # Arguments
    ///
    /// * `existing` - Existing lockfile to update
    /// * `deps_to_update` - Optional specific dependency names to update (None = all)
    ///
    /// # Errors
    ///
    /// Returns an error if update process fails
    pub async fn update(
        &mut self,
        existing: &LockFile,
        deps_to_update: Option<Vec<String>>,
    ) -> Result<LockFile> {
        // For now, just resolve all dependencies
        // TODO: Implement proper incremental update logic using deps_to_update names
        let _existing = existing; // Suppress unused warning for now
        let _deps_to_update = deps_to_update; // Suppress unused warning for now
        self.resolve_with_options(true).await
    }

    /// Get available versions for a repository.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the Git repository
    ///
    /// # Returns
    ///
    /// List of available version strings (tags and branches)
    pub async fn get_available_versions(&self, repo_path: &Path) -> Result<Vec<String>> {
        VersionResolutionService::get_available_versions(&self.core, repo_path).await
    }

    /// Verify that existing lockfile is still valid.
    ///
    /// # Arguments
    ///
    /// * `_lockfile` - Existing lockfile to verify
    ///
    /// # Errors
    ///
    /// Returns an error if verification fails
    pub async fn verify(&self, _lockfile: &LockFile) -> Result<()> {
        // TODO: Implement verification logic using services
        Ok(())
    }

    /// Get current operation context if available.
    pub fn operation_context(&self) -> Option<&Arc<OperationContext>> {
        self.core.operation_context()
    }

    /// Set the operation context for warning deduplication.
    ///
    /// # Arguments
    ///
    /// * `context` - The operation context to use
    pub fn set_operation_context(&mut self, context: Arc<OperationContext>) {
        self.core.operation_context = Some(context);
    }
}

// Private helper methods
impl DependencyResolver {
    /// Build an index of manifest overrides for deduplication with transitive deps.
    ///
    /// This method creates a mapping from resource identity (source, path, tool, variant_hash)
    /// to the customizations (filename, target, install, template_vars) specified in the manifest.
    /// When a transitive dependency is discovered that matches a manifest dependency, the manifest
    /// version's customizations will take precedence.
    fn build_manifest_override_index(
        &self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
    ) -> types::ManifestOverrideIndex {
        use crate::resolver::types::{ManifestOverride, OverrideKey, normalize_lookup_path};

        let mut index = HashMap::new();

        for (name, dep, resource_type) in base_deps {
            // Skip pattern dependencies (they expand later)
            if dep.is_pattern() {
                continue;
            }

            // Build the override key
            let normalized_path = normalize_lookup_path(dep.get_path());
            let source = dep.get_source().map(std::string::ToString::to_string);

            // Determine tool for this dependency
            let tool = dep
                .get_tool()
                .map(str::to_string)
                .unwrap_or_else(|| self.core.manifest().get_default_tool(*resource_type));

            // Compute variant_hash from MERGED variant_inputs (dep + global config)
            // This ensures manifest overrides use the same hash as LockedResources
            let merged_variant_inputs =
                lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep);
            let variant_hash = crate::utils::compute_variant_inputs_hash(&merged_variant_inputs)
                .unwrap_or_else(|_| crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string());

            let key = OverrideKey {
                resource_type: *resource_type,
                normalized_path,
                source,
                tool,
                variant_hash,
            };

            // Build the override info
            let override_info = ManifestOverride {
                filename: dep.get_filename().map(std::string::ToString::to_string),
                target: dep.get_target().map(std::string::ToString::to_string),
                install: dep.get_install(),
                manifest_alias: Some(name.clone()),
                template_vars: dep.get_template_vars().cloned(),
            };

            tracing::debug!(
                "Adding manifest override for {:?}:{} (tool={}, variant_hash={})",
                resource_type,
                dep.get_path(),
                key.tool,
                key.variant_hash
            );

            index.insert(key, override_info);
        }

        tracing::info!("Built manifest override index with {} entries", index.len());
        index
    }

    /// Resolve transitive dependencies starting from base dependencies.
    ///
    /// Discovers dependencies declared in resource files, expands patterns,
    /// builds dependency graph with cycle detection, and returns all dependencies
    /// in topological order.
    async fn resolve_transitive_dependencies(
        &mut self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
    ) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
        use crate::resolver::transitive_resolver;

        // Build override index FIRST from manifest dependencies
        let manifest_overrides = self.build_manifest_override_index(base_deps);

        // Build ResolutionContext for the transitive resolver
        let resolution_ctx = ResolutionContext {
            manifest: self.core.manifest(),
            cache: self.core.cache(),
            source_manager: self.core.source_manager(),
            operation_context: self.core.operation_context(),
        };

        // Build TransitiveContext with mutable state and the override index
        let mut ctx = TransitiveContext {
            base: resolution_ctx,
            dependency_map: &mut self.dependency_map,
            transitive_custom_names: &mut self.transitive_custom_names,
            conflict_detector: &mut self.conflict_detector,
            manifest_overrides: &manifest_overrides,
        };

        // Get prepared versions from version service (clone to avoid borrow conflicts)
        let prepared_versions = self.version_service.prepared_versions().clone();

        // Create services container
        let mut services = transitive_resolver::ResolutionServices {
            version_service: &mut self.version_service,
            pattern_service: &mut self.pattern_service,
        };

        // Call the service-based transitive resolver
        transitive_resolver::resolve_with_services(
            &mut ctx,
            &self.core,
            base_deps,
            true, // enable_transitive
            &prepared_versions,
            &mut self.pattern_alias_map,
            &mut services,
        )
        .await
    }

    /// Get the list of transitive dependencies for a resource.
    ///
    /// Returns the dependency IDs (format: "type/name") for all transitive
    /// dependencies discovered during resolution.
    fn get_dependencies_for(
        &self,
        name: &str,
        source: Option<&str>,
        resource_type: ResourceType,
        tool: Option<&str>,
        variant_hash: &str,
    ) -> Vec<String> {
        let key = (
            resource_type,
            name.to_string(),
            source.map(std::string::ToString::to_string),
            tool.map(std::string::ToString::to_string),
            variant_hash.to_string(),
        );
        let result = self.dependency_map.get(&key).cloned().unwrap_or_default();
        tracing::debug!(
            "[DEBUG] get_dependencies_for: name='{}', type={:?}, source={:?}, tool={:?}, hash={}, found={} deps",
            name,
            resource_type,
            source,
            tool,
            &variant_hash[..8],
            result.len()
        );
        result
    }

    /// Get pattern alias for a concrete dependency.
    ///
    /// Returns the pattern name if this dependency was created from a pattern expansion.
    fn get_pattern_alias_for_dependency(
        &self,
        name: &str,
        resource_type: ResourceType,
    ) -> Option<String> {
        // Check if this dependency was created from a pattern expansion
        self.pattern_alias_map.get(&(resource_type, name.to_string())).cloned()
    }

    /// Resolve a single dependency to a lockfile entry.
    ///
    /// Delegates to specialized resolvers based on dependency type.
    async fn resolve_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        tracing::debug!(
            "resolve_dependency: name={}, path={}, source={:?}, is_local={}",
            name,
            dep.get_path(),
            dep.get_source(),
            dep.is_local()
        );

        if dep.is_local() {
            self.resolve_local_dependency(name, dep, resource_type)
        } else {
            self.resolve_git_dependency(name, dep, resource_type).await
        }
    }

    /// Determine the filename for a dependency.
    ///
    /// Returns the custom filename if specified, otherwise extracts
    /// a meaningful name from the dependency path.
    fn resolve_filename(dep: &ResourceDependency) -> String {
        dep.get_filename()
            .map_or_else(|| extract_meaningful_path(Path::new(dep.get_path())), |f| f.to_string())
    }

    /// Get the tool/artifact type for a dependency.
    ///
    /// Returns the explicitly specified tool or the default tool for the resource type.
    fn resolve_tool(&self, dep: &ResourceDependency, resource_type: ResourceType) -> String {
        dep.get_tool()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type))
    }

    /// Determine manifest_alias for a dependency.
    ///
    /// Returns Some for direct manifest dependencies or pattern-expanded dependencies,
    /// None for transitive dependencies.
    fn resolve_manifest_alias(&self, name: &str, resource_type: ResourceType) -> Option<String> {
        let has_pattern_alias = self.get_pattern_alias_for_dependency(name, resource_type);
        let is_in_manifest = self
            .core
            .manifest()
            .get_dependencies(resource_type)
            .is_some_and(|deps| deps.contains_key(name));

        if let Some(pattern_alias) = has_pattern_alias {
            // Pattern-expanded dependency - use pattern name as manifest_alias
            Some(pattern_alias)
        } else if is_in_manifest {
            // Direct manifest dependency - use name as manifest_alias
            Some(name.to_string())
        } else {
            // Transitive dependency - no manifest_alias
            None
        }
    }

    /// Resolve local file system dependency to locked resource.
    fn resolve_local_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::resolver::lockfile_builder;
        use crate::resolver::path_resolver as install_path_resolver;
        use crate::utils::normalize_path_for_storage;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        // Generate canonical name for local dependencies FIRST
        // For skills, this extracts the name from SKILL.md frontmatter
        // For other resources, this normalizes the path
        let canonical_name =
            self.compute_local_canonical_name(name, dep, &manifest_alias, resource_type)?;

        // For skills, use the canonical name (from frontmatter) for the installation path
        // For other resources, use the extracted filename from the path
        let filename = if resource_type == ResourceType::Skill {
            canonical_name.clone()
        } else {
            Self::resolve_filename(dep)
        };

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        tracing::debug!(
            "Local dependency: name={}, path={}, manifest_alias={:?}",
            name,
            dep.get_path(),
            manifest_alias
        );

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        Ok(LockedResource {
            name: canonical_name,
            source: None,
            url: None,
            path: normalize_path_for_storage(dep.get_path()),
            version: None,
            resolved_commit: None,
            checksum: String::new(),
            installed_at,
            files: None, // Single file resources don't have files list
            dependencies: self.get_dependencies_for(
                name,
                None,
                resource_type,
                Some(&artifact_type_string),
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
        })
    }

    /// Compute canonical name for local dependencies.
    ///
    /// For transitive dependencies (manifest_alias=None), returns name as-is.
    /// For direct dependencies (manifest_alias=Some), normalizes path relative to manifest.
    fn compute_local_canonical_name(
        &self,
        name: &str,
        dep: &ResourceDependency,
        manifest_alias: &Option<String>,
        resource_type: ResourceType,
    ) -> Result<String> {
        if manifest_alias.is_none() {
            // Transitive dependency - name is already correct (e.g., "../snippets/agents/backend-engineer")
            Ok(name.to_string())
        } else if let Some(manifest_dir) = self.core.manifest().manifest_dir.as_ref() {
            // For skills, extract the name from SKILL.md frontmatter
            // This ensures the installed name matches the skill's declared name
            if resource_type == ResourceType::Skill {
                let skill_path = if Path::new(dep.get_path()).is_absolute() {
                    PathBuf::from(dep.get_path())
                } else {
                    manifest_dir.join(dep.get_path())
                };

                // Try to extract skill metadata to get the actual skill name
                if let Ok((frontmatter, _)) = crate::skills::extract_skill_metadata(&skill_path) {
                    return Ok(frontmatter.name);
                }

                // Fallback to basename if extraction fails (e.g., invalid skill structure)
                let basename = skill_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid skill path: {}", dep.get_path()))?
                    .to_string_lossy()
                    .to_string();
                return Ok(basename);
            }

            // Direct dependency - normalize path relative to manifest
            let full_path = if Path::new(dep.get_path()).is_absolute() {
                PathBuf::from(dep.get_path())
            } else {
                manifest_dir.join(dep.get_path())
            };

            // Normalize the path to handle ../ and ./ components deterministically
            let canonical_path = crate::utils::fs::normalize_path(&full_path);

            let source_context =
                crate::resolver::source_context::SourceContext::local(manifest_dir);
            Ok(generate_dependency_name(&canonical_path.to_string_lossy(), &source_context))
        } else {
            // Fallback to name if manifest_dir is not available
            Ok(name.to_string())
        }
    }

    /// Resolve Git-based dependency to locked resource.
    async fn resolve_git_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::resolver::lockfile_builder;
        use crate::resolver::path_resolver as install_path_resolver;
        use crate::utils::normalize_path_for_storage;

        let source_name = dep
            .get_source()
            .ok_or_else(|| anyhow::anyhow!("Dependency '{}' has no source specified", name))?;

        // Generate canonical name using remote source context
        let source_context = crate::resolver::source_context::SourceContext::remote(source_name);
        let canonical_name = generate_dependency_name(dep.get_path(), &source_context);

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        let filename = Self::resolve_filename(dep);
        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        Ok(LockedResource {
            name: canonical_name,
            source: Some(source_name.to_string()),
            url: Some(source_url.clone()),
            path: normalize_path_for_storage(dep.get_path()),
            version: prepared.resolved_version.clone(),
            resolved_commit: Some(prepared.resolved_commit.clone()),
            checksum: String::new(),
            installed_at,
            files: None, // Single file resources don't have files list
            dependencies: self.get_dependencies_for(
                name,
                Some(source_name),
                resource_type,
                Some(&artifact_type_string),
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
        })
    }

    /// Resolve a pattern dependency to multiple locked resources.
    ///
    /// Delegates to local or Git pattern resolvers based on dependency type.
    async fn resolve_pattern_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        if !dep.is_pattern() {
            return Err(anyhow::anyhow!(
                "Expected pattern dependency but no glob characters found in path"
            ));
        }

        if dep.is_local() {
            self.resolve_local_pattern(name, dep, resource_type)
        } else {
            self.resolve_git_pattern(name, dep, resource_type).await
        }
    }

    /// Resolve local pattern dependency to multiple locked resources.
    fn resolve_local_pattern(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{lockfile_builder, path_resolver};

        let pattern = dep.get_path();
        let (base_path, pattern_str) = path_resolver::parse_pattern_base_path(pattern);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);
            let full_relative_path =
                path_resolver::construct_full_relative_path(&base_path, &matched_path);
            let filename = path_resolver::extract_pattern_filename(&base_path, &matched_path);

            let installed_at = path_resolver::resolve_install_path(
                self.core.manifest(),
                dep,
                artifact_type,
                resource_type,
                &filename,
            )?;

            resources.push(LockedResource {
                name: resource_name.clone(),
                source: None,
                url: None,
                path: full_relative_path,
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at,
                files: None, // Pattern-matched resources are single files
                dependencies: vec![],
                resource_type,
                tool: Some(artifact_type_string.clone()),
                manifest_alias: Some(name.to_string()),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    &resource_name, // Use canonical resource name
                    Some(name),     // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
            });
        }

        Ok(resources)
    }

    /// Resolve Git-based pattern dependency to multiple locked resources.
    async fn resolve_git_pattern(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{lockfile_builder, path_resolver};
        use crate::utils::{
            compute_relative_install_path, normalize_path, normalize_path_for_storage,
        };

        let pattern = dep.get_path();
        let pattern_name = name;

        let source_name = dep.get_source().ok_or_else(|| {
            anyhow::anyhow!("Pattern dependency '{}' has no source specified", name)
        })?;

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        let repo_path = Path::new(&prepared.worktree_path);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(pattern, repo_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);

            // Compute installation path
            let installed_at = match resource_type {
                ResourceType::Hook | ResourceType::McpServer => {
                    path_resolver::resolve_merge_target_path(
                        self.core.manifest(),
                        artifact_type,
                        resource_type,
                    )
                }
                _ => {
                    let artifact_path = self
                        .core
                        .manifest()
                        .get_artifact_resource_path(artifact_type, resource_type)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Resource type '{}' is not supported by tool '{}'",
                                resource_type,
                                artifact_type
                            )
                        })?;

                    let dep_flatten = dep.get_flatten();
                    let tool_flatten = self
                        .core
                        .manifest()
                        .get_tool_config(artifact_type)
                        .and_then(|config| config.resources.get(resource_type.to_plural()))
                        .and_then(|resource_config| resource_config.flatten);

                    let flatten = dep_flatten.or(tool_flatten).unwrap_or(false);

                    let base_target = if let Some(custom_target) = dep.get_target() {
                        PathBuf::from(artifact_path.display().to_string())
                            .join(custom_target.trim_start_matches('/'))
                    } else {
                        artifact_path.to_path_buf()
                    };

                    let filename = repo_path.join(&matched_path).to_string_lossy().to_string();
                    let relative_path =
                        compute_relative_install_path(&base_target, Path::new(&filename), flatten);
                    normalize_path_for_storage(normalize_path(&base_target.join(relative_path)))
                }
            };

            resources.push(LockedResource {
                name: resource_name.clone(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: normalize_path_for_storage(matched_path.to_string_lossy().to_string()),
                version: prepared.resolved_version.clone(),
                resolved_commit: Some(prepared.resolved_commit.clone()),
                checksum: String::new(),
                installed_at,
                files: None, // Pattern-matched resources are single files
                dependencies: vec![],
                resource_type,
                tool: Some(artifact_type_string.clone()),
                manifest_alias: Some(pattern_name.to_string()),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    &resource_name,     // Use canonical resource name
                    Some(pattern_name), // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
            });
        }

        Ok(resources)
    }

    /// Add or update a lockfile entry with deduplication.
    fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        let resources = lockfile.get_resources_mut(&entry.resource_type);

        if let Some(existing) =
            resources.iter_mut().find(|e| lockfile_builder::is_duplicate_entry(e, &entry))
        {
            // Replace only if the new entry is more authoritative than the existing one
            // Priority: Direct (manifest_alias != None) > Transitive (manifest_alias == None)
            let existing_is_direct = existing.manifest_alias.is_some();
            let new_is_direct = entry.manifest_alias.is_some();

            if new_is_direct || !existing_is_direct {
                // Replace if:
                // - New is direct (always wins)
                // - Both are transitive (newer wins)
                tracing::debug!(
                    "Replacing {} (direct={}) with {} (direct={})",
                    existing.name,
                    existing_is_direct,
                    entry.name,
                    new_is_direct
                );
                *existing = entry;
            } else {
                // Keep existing direct entry, ignore transitive replacement
                tracing::debug!("Keeping direct {} over transitive {}", existing.name, entry.name);
            }
        } else {
            resources.push(entry);
        }
    }

    /// Add version information to dependency references in lockfile.
    fn add_version_to_dependencies(&self, lockfile: &mut LockFile) -> Result<()> {
        use crate::resolver::lockfile_builder;

        lockfile_builder::add_version_to_all_dependencies(lockfile);
        Ok(())
    }

    /// Detect target path conflicts between resources.
    fn detect_target_conflicts(&self, lockfile: &LockFile) -> Result<()> {
        use crate::resolver::lockfile_builder;

        lockfile_builder::detect_target_conflicts(lockfile)
    }

    /// Add a dependency to the conflict detector.
    fn add_to_conflict_detector(
        &mut self,
        _name: &str,
        dep: &ResourceDependency,
        required_by: &str,
    ) {
        use crate::resolver::types as dependency_helpers;

        // Skip local dependencies (no version conflicts possible)
        if dep.is_local() {
            return;
        }

        // Build resource identifier
        let resource_id = dependency_helpers::build_resource_id(dep);

        // Get version constraint (None means HEAD/unspecified)
        let version = dep.get_version().unwrap_or("HEAD");

        // Add to conflict detector
        self.conflict_detector.add_requirement(&resource_id, required_by, version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolver_creation() {
        let manifest = Manifest::default();
        let cache = Cache::new().unwrap();
        let resolver = DependencyResolver::new(manifest, cache).await;
        assert!(resolver.is_ok());
    }

    #[tokio::test]
    async fn test_resolver_with_global() {
        let manifest = Manifest::default();
        let cache = Cache::new().unwrap();
        let resolver = DependencyResolver::new_with_global(manifest, cache).await;
        assert!(resolver.is_ok());
    }
}
