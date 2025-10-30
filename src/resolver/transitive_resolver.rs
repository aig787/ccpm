//! Transitive dependency resolution for AGPM.
//!
//! This module handles the discovery and resolution of transitive dependencies,
//! building dependency graphs, detecting cycles, and providing high-level
//! orchestration for the entire transitive resolution process. It processes
//! dependencies declared within resource files and resolves them in topological order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::metadata::MetadataExtractor;
use crate::utils;

use super::dependency_graph::{DependencyGraph, DependencyNode};
use super::pattern_expander::generate_dependency_name;
use super::types::{
    DependencyKey, TransitiveContext, apply_manifest_override, compute_dependency_variant_hash,
};
use super::version_resolver::{PreparedSourceVersion, VersionResolutionService};
use super::{PatternExpansionService, ResourceFetchingService, is_file_relative_path};

/// Container for resolution services to reduce parameter count.
pub struct ResolutionServices<'a> {
    /// Service for version resolution and commit SHA lookup
    pub version_service: &'a mut VersionResolutionService,
    /// Service for pattern expansion (glob patterns)
    pub pattern_service: &'a mut PatternExpansionService,
}

/// Process a single transitive dependency specification.
#[allow(clippy::too_many_arguments)]
async fn process_transitive_dependency_spec(
    ctx: &TransitiveContext<'_>,
    core: &super::ResolutionCore,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    version_service: &mut VersionResolutionService,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<(ResourceDependency, String)> {
    // Get the canonical path to the parent resource file
    let parent_file_path =
        ResourceFetchingService::get_canonical_path(core, parent_dep, version_service)
            .await
            .with_context(|| {
                format!(
                    "Failed to get parent path for transitive dependencies of '{}'",
                    parent_name
                )
            })?;

    // Resolve the transitive dependency path
    let trans_canonical = resolve_transitive_path(&parent_file_path, &dep_spec.path, parent_name)?;

    // Create the transitive dependency
    let trans_dep = create_transitive_dependency(
        ctx,
        parent_dep,
        dep_resource_type,
        parent_resource_type,
        parent_name,
        dep_spec,
        &parent_file_path,
        &trans_canonical,
        prepared_versions,
    )
    .await?;

    // Generate a name for the transitive dependency using source context
    let trans_name = if trans_dep.get_source().is_none() {
        // Local dependency - use manifest directory as source context
        // Use trans_dep.get_path() which is already relative to manifest directory
        // (computed in create_path_only_transitive_dep)
        let manifest_dir = ctx
            .base
            .manifest
            .manifest_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Manifest directory not available"))?;

        let source_context = crate::resolver::source_context::SourceContext::local(manifest_dir);
        generate_dependency_name(trans_dep.get_path(), &source_context)
    } else {
        // Git dependency - use remote source context
        let source_name = trans_dep
            .get_source()
            .ok_or_else(|| anyhow::anyhow!("Git dependency missing source name"))?;
        let source_context = crate::resolver::source_context::SourceContext::remote(source_name);
        generate_dependency_name(trans_dep.get_path(), &source_context)
    };

    Ok((trans_dep, trans_name))
}

/// Resolve a transitive dependency path relative to its parent.
fn resolve_transitive_path(
    parent_file_path: &Path,
    dep_path: &str,
    parent_name: &str,
) -> Result<PathBuf> {
    // Check if this is a glob pattern
    let is_pattern = dep_path.contains('*') || dep_path.contains('?') || dep_path.contains('[');

    if is_pattern {
        // For patterns, normalize (resolve .. and .) but don't canonicalize
        let parent_dir = parent_file_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to resolve transitive dependency '{}' for '{}': parent file has no directory",
                dep_path,
                parent_name
            )
        })?;
        let resolved = parent_dir.join(dep_path);

        // Preserve the root component when normalizing
        let mut result = PathBuf::new();
        for component in resolved.components() {
            match component {
                std::path::Component::RootDir => result.push(component),
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::CurDir => {}
                _ => result.push(component),
            }
        }
        Ok(result)
    } else if is_file_relative_path(dep_path) || !dep_path.contains('/') {
        // File-relative path (starts with ./ or ../) or bare filename
        // For bare filenames, treat as file-relative by resolving from parent directory
        let parent_dir = parent_file_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to resolve transitive dependency '{}' for '{}': parent file has no directory",
                dep_path,
                parent_name
            )
        })?;

        let resolved = parent_dir.join(dep_path);
        resolved.canonicalize().map_err(|e| {
            // Create a FileOperationError for canonicalization failures
            let file_error = crate::core::file_error::FileOperationError::new(
                crate::core::file_error::FileOperationContext::new(
                    crate::core::file_error::FileOperation::Canonicalize,
                    &resolved,
                    format!("resolving transitive dependency '{}' for '{}'", dep_path, parent_name),
                    "transitive_resolver::resolve_transitive_path",
                ),
                e,
            );
            anyhow::Error::from(file_error)
        })
    } else {
        // Repo-relative path
        resolve_repo_relative_path(parent_file_path, dep_path, parent_name)
    }
}

/// Resolve a repository-relative transitive dependency path.
fn resolve_repo_relative_path(
    parent_file_path: &Path,
    dep_path: &str,
    parent_name: &str,
) -> Result<PathBuf> {
    // For Git sources, find the worktree root; for local sources, find the source root
    let repo_root = parent_file_path
        .ancestors()
        .find(|p| {
            // Worktree directories have format: owner_repo_sha8
            p.file_name().and_then(|n| n.to_str()).map(|s| s.contains('_')).unwrap_or(false)
        })
        .or_else(|| parent_file_path.ancestors().nth(2)) // Fallback for local sources
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to find repository root for transitive dependency '{}'",
                dep_path
            )
        })?;

    let full_path = repo_root.join(dep_path);
    full_path.canonicalize().with_context(|| {
        format!(
            "Failed to resolve repo-relative transitive dependency '{}' for '{}': {} (repo root: {})",
            dep_path,
            parent_name,
            full_path.display(),
            repo_root.display()
        )
    })
}

/// Create a ResourceDependency for a transitive dependency.
#[allow(clippy::too_many_arguments)]
async fn create_transitive_dependency(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    parent_file_path: &Path,
    trans_canonical: &Path,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<ResourceDependency> {
    use super::types::{OverrideKey, compute_dependency_variant_hash, normalize_lookup_path};

    // Create the dependency as before
    let mut dep = if parent_dep.get_source().is_none() {
        create_path_only_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            dep_spec,
            trans_canonical,
        )?
    } else {
        create_git_backed_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            parent_name,
            dep_spec,
            parent_file_path,
            trans_canonical,
            prepared_versions,
        )
        .await?
    };

    // Check for manifest override
    let normalized_path = normalize_lookup_path(dep.get_path());
    let source = dep.get_source().map(std::string::ToString::to_string);

    // Determine tool for the dependency
    let tool = dep
        .get_tool()
        .map(str::to_string)
        .unwrap_or_else(|| ctx.base.manifest.get_default_tool(dep_resource_type));

    let variant_hash = compute_dependency_variant_hash(&dep);

    let override_key = OverrideKey {
        resource_type: dep_resource_type,
        normalized_path: normalized_path.clone(),
        source,
        tool,
        variant_hash,
    };

    // Apply manifest override if found
    if let Some(override_info) = ctx.manifest_overrides.get(&override_key) {
        apply_manifest_override(&mut dep, override_info, &normalized_path);
    }

    Ok(dep)
}

/// Create a path-only transitive dependency (parent is path-only).
fn create_path_only_transitive_dep(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    dep_spec: &crate::manifest::DependencySpec,
    trans_canonical: &Path,
) -> Result<ResourceDependency> {
    let manifest_dir = ctx.base.manifest.manifest_dir.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Manifest directory not available for path-only transitive dep")
    })?;

    // Always compute relative path from manifest to target
    let dep_path_str = match manifest_dir.canonicalize() {
        Ok(canonical_manifest) => {
            utils::compute_relative_path(&canonical_manifest, trans_canonical)
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not canonicalize manifest directory {}: {}. Using non-canonical path.",
                manifest_dir.display(),
                e
            );
            utils::compute_relative_path(manifest_dir, trans_canonical)
        }
    };

    // Determine tool for transitive dependency
    let trans_tool = determine_transitive_tool(
        ctx,
        parent_dep,
        dep_spec,
        parent_resource_type,
        dep_resource_type,
    );

    Ok(ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: None,
        path: utils::normalize_path_for_storage(dep_path_str),
        version: None,
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: trans_tool,
        flatten: None,
        install: dep_spec.install.or(Some(true)),
        template_vars: Some(super::lockfile_builder::build_merged_variant_inputs(
            ctx.base.manifest,
            parent_dep,
        )),
    })))
}

/// Create a Git-backed transitive dependency (parent is Git-backed).
#[allow(clippy::too_many_arguments)]
async fn create_git_backed_transitive_dep(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    _parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    parent_file_path: &Path,
    trans_canonical: &Path,
    _prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<ResourceDependency> {
    let source_name = parent_dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Expected source for Git-backed dependency"))?;
    let source_url = ctx
        .base
        .source_manager
        .get_source_url(source_name)
        .ok_or_else(|| anyhow::anyhow!("Source '{source_name}' not found"))?;

    // Get repo-relative path by stripping the appropriate prefix
    let repo_relative = if utils::is_local_path(&source_url) {
        strip_local_source_prefix(&source_url, trans_canonical)?
    } else {
        // For remote Git sources, derive the worktree root from the parent file path
        strip_git_worktree_prefix_from_parent(parent_file_path, trans_canonical)?
    };

    // Determine tool for transitive dependency
    let trans_tool = determine_transitive_tool(
        ctx,
        parent_dep,
        dep_spec,
        parent_resource_type,
        dep_resource_type,
    );

    Ok(ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some(source_name.to_string()),
        path: utils::normalize_path_for_storage(repo_relative.to_string_lossy().to_string()),
        version: dep_spec
            .version
            .clone()
            .or_else(|| parent_dep.get_version().map(|v| v.to_string())),
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: trans_tool,
        flatten: None,
        install: dep_spec.install.or(Some(true)),
        template_vars: Some(super::lockfile_builder::build_merged_variant_inputs(
            ctx.base.manifest,
            parent_dep,
        )),
    })))
}

/// Strip the local source prefix from a transitive dependency path.
fn strip_local_source_prefix(source_url: &str, trans_canonical: &Path) -> Result<PathBuf> {
    let source_path = PathBuf::from(source_url).canonicalize()?;

    // Check if this is a pattern path (contains glob characters)
    let trans_str = trans_canonical.to_string_lossy();
    let is_pattern = trans_str.contains('*') || trans_str.contains('?') || trans_str.contains('[');

    if is_pattern {
        // For patterns, canonicalize the directory part while keeping the pattern filename intact
        let parent_dir = trans_canonical.parent().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no parent directory: {}", trans_canonical.display())
        })?;
        let filename = trans_canonical.file_name().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no filename: {}", trans_canonical.display())
        })?;

        // Canonicalize the directory part
        let canonical_dir = parent_dir.canonicalize().with_context(|| {
            format!("Failed to canonicalize pattern directory: {}", parent_dir.display())
        })?;

        // Reconstruct the full path with canonical directory and pattern filename
        let canonical_pattern = canonical_dir.join(filename);

        // Now strip the source prefix
        canonical_pattern
            .strip_prefix(&source_path)
            .with_context(|| {
                format!(
                    "Transitive pattern dep outside parent's source: {} not under {}",
                    canonical_pattern.display(),
                    source_path.display()
                )
            })
            .map(|p| p.to_path_buf())
    } else {
        trans_canonical
            .strip_prefix(&source_path)
            .with_context(|| {
                format!(
                    "Transitive dep resolved outside parent's source directory: {} not under {}",
                    trans_canonical.display(),
                    source_path.display()
                )
            })
            .map(|p| p.to_path_buf())
    }
}

/// Strip the Git worktree prefix from a transitive dependency path by deriving
/// the worktree root from the parent file path.
fn strip_git_worktree_prefix_from_parent(
    parent_file_path: &Path,
    trans_canonical: &Path,
) -> Result<PathBuf> {
    // Find the worktree root by looking for a directory with the pattern: owner_repo_sha8
    // Start from the parent file and walk up the directory tree
    let worktree_root = parent_file_path
        .ancestors()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| {
                    // Worktree directories have format: owner_repo_sha8 (contains underscores)
                    s.contains('_')
                })
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to find worktree root from parent file: {}",
                parent_file_path.display()
            )
        })?;

    // Canonicalize worktree root to handle symlinks
    let canonical_worktree = worktree_root.canonicalize().with_context(|| {
        format!("Failed to canonicalize worktree root: {}", worktree_root.display())
    })?;

    // Check if this is a pattern path (contains glob characters)
    let trans_str = trans_canonical.to_string_lossy();
    let is_pattern = trans_str.contains('*') || trans_str.contains('?') || trans_str.contains('[');

    if is_pattern {
        // For patterns, canonicalize the directory part while keeping the pattern filename intact
        let parent_dir = trans_canonical.parent().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no parent directory: {}", trans_canonical.display())
        })?;
        let filename = trans_canonical.file_name().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no filename: {}", trans_canonical.display())
        })?;

        // Canonicalize the directory part
        let canonical_dir = parent_dir.canonicalize().with_context(|| {
            format!("Failed to canonicalize pattern directory: {}", parent_dir.display())
        })?;

        // Reconstruct the full path with canonical directory and pattern filename
        let canonical_pattern = canonical_dir.join(filename);

        // Now strip the worktree prefix
        canonical_pattern
            .strip_prefix(&canonical_worktree)
            .with_context(|| {
                format!(
                    "Transitive pattern dep outside parent's worktree: {} not under {}",
                    canonical_pattern.display(),
                    canonical_worktree.display()
                )
            })
            .map(|p| p.to_path_buf())
    } else {
        trans_canonical
            .strip_prefix(&canonical_worktree)
            .with_context(|| {
                format!(
                    "Transitive dep outside parent's worktree: {} not under {}",
                    trans_canonical.display(),
                    canonical_worktree.display()
                )
            })
            .map(|p| p.to_path_buf())
    }
}

/// Determine the tool for a transitive dependency.
fn determine_transitive_tool(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_spec: &crate::manifest::DependencySpec,
    parent_resource_type: ResourceType,
    dep_resource_type: ResourceType,
) -> Option<String> {
    if let Some(explicit_tool) = &dep_spec.tool {
        Some(explicit_tool.clone())
    } else {
        let parent_tool = parent_dep
            .get_tool()
            .map(str::to_string)
            .unwrap_or_else(|| ctx.base.manifest.get_default_tool(parent_resource_type));
        if ctx.base.manifest.is_resource_supported(&parent_tool, dep_resource_type) {
            Some(parent_tool)
        } else {
            Some(ctx.base.manifest.get_default_tool(dep_resource_type))
        }
    }
}

/// Add a dependency to the conflict detector.
fn add_to_conflict_detector(
    ctx: &mut TransitiveContext<'_>,
    name: &str,
    dep: &ResourceDependency,
    requester: &str,
) {
    if let Some(version) = dep.get_version() {
        ctx.conflict_detector.add_requirement(name, requester, version);
    }
}

/// Build the final ordered result from the dependency graph.
fn build_ordered_result(
    all_deps: HashMap<DependencyKey, ResourceDependency>,
    ordered_nodes: Vec<DependencyNode>,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
    let mut result = Vec::new();
    let mut added_keys = HashSet::new();

    tracing::debug!(
        "Transitive resolution - topological order has {} nodes, all_deps has {} entries",
        ordered_nodes.len(),
        all_deps.len()
    );

    for node in ordered_nodes {
        tracing::debug!(
            "Processing ordered node: {}/{} (source: {:?})",
            node.resource_type,
            node.name,
            node.source
        );

        // Find matching dependency
        for (key, dep) in &all_deps {
            if key.0 == node.resource_type && key.1 == node.name && key.2 == node.source {
                tracing::debug!(
                    "  -> Found match in all_deps, adding to result with type {:?}",
                    node.resource_type
                );
                result.push((node.name.clone(), dep.clone(), node.resource_type));
                added_keys.insert(key.clone());
                break;
            }
        }
    }

    // Add remaining dependencies that weren't in the graph (no transitive deps)
    for (key, dep) in all_deps {
        if !added_keys.contains(&key) && !dep.is_pattern() {
            tracing::debug!(
                "Adding non-graph dependency: {}/{} (source: {:?}) with type {:?}",
                key.0,
                key.1,
                key.2,
                key.0
            );
            result.push((key.1.clone(), dep.clone(), key.0));
        }
    }

    tracing::debug!("Transitive resolution returning {} dependencies", result.len());

    Ok(result)
}

/// Generate unique key for grouping dependencies by source and version.
pub fn group_key(source: &str, version: &str) -> String {
    format!("{source}::{version}")
}

/// Service-based wrapper for transitive dependency resolution.
///
/// This provides a simpler API for internal use that takes service references
/// directly instead of requiring closure-based dependency injection.
pub async fn resolve_with_services(
    ctx: &mut TransitiveContext<'_>,
    core: &super::ResolutionCore,
    base_deps: &[(String, ResourceDependency, ResourceType)],
    enable_transitive: bool,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
    pattern_alias_map: &mut HashMap<(ResourceType, String), String>,
    services: &mut ResolutionServices<'_>,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
    // Clear state from any previous resolution
    ctx.dependency_map.clear();

    if !enable_transitive {
        return Ok(base_deps.to_vec());
    }

    let mut graph = DependencyGraph::new();
    let mut all_deps: HashMap<DependencyKey, ResourceDependency> = HashMap::new();
    let mut processed: HashSet<DependencyKey> = HashSet::new();
    let mut queue: Vec<(String, ResourceDependency, Option<ResourceType>, String)> = Vec::new();

    // Add initial dependencies to queue with their threaded types
    for (name, dep, resource_type) in base_deps {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);

        // Compute variant_hash from MERGED variant_inputs (dep + global config)
        // This ensures consistency with how LockedResource computes its hash
        let merged_variant_inputs =
            super::lockfile_builder::build_merged_variant_inputs(ctx.base.manifest, dep);
        let variant_hash = crate::utils::compute_variant_inputs_hash(&merged_variant_inputs)
            .unwrap_or_else(|_| crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string());

        tracing::debug!(
            "[DEBUG] Adding base dep to queue: '{}' (type: {:?}, source: {:?}, tool: {:?}, is_local: {})",
            name,
            resource_type,
            source,
            tool,
            dep.is_local()
        );
        // Store pre-computed hash in queue to avoid duplicate computation
        queue.push((name.clone(), dep.clone(), Some(*resource_type), variant_hash.clone()));
        all_deps.insert((*resource_type, name.clone(), source, tool, variant_hash), dep.clone());
    }

    // Process queue to discover transitive dependencies
    while let Some((name, dep, resource_type, variant_hash)) = queue.pop() {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);

        let resource_type =
            resource_type.expect("resource_type should always be threaded through queue");
        let key = (resource_type, name.clone(), source.clone(), tool.clone(), variant_hash.clone());

        tracing::debug!(
            "[TRANSITIVE] Processing: '{}' (type: {:?}, source: {:?})",
            name,
            resource_type,
            source
        );

        // Check if this queue entry is stale (superseded by conflict resolution)
        if let Some(current_dep) = all_deps.get(&key) {
            if current_dep.get_version() != dep.get_version() {
                tracing::debug!("[TRANSITIVE] Skipped stale: '{}'", name);
                continue;
            }
        }

        if processed.contains(&key) {
            tracing::debug!("[TRANSITIVE] Already processed: '{}'", name);
            continue;
        }

        processed.insert(key.clone());

        // Handle pattern dependencies by expanding them to concrete files
        if dep.is_pattern() {
            tracing::debug!("[TRANSITIVE] Expanding pattern: '{}'", name);
            match services
                .pattern_service
                .expand_pattern(core, &dep, resource_type, services.version_service)
                .await
            {
                Ok(concrete_deps) => {
                    for (concrete_name, concrete_dep) in concrete_deps {
                        pattern_alias_map
                            .insert((resource_type, concrete_name.clone()), name.clone());

                        let concrete_source =
                            concrete_dep.get_source().map(std::string::ToString::to_string);
                        let concrete_tool =
                            concrete_dep.get_tool().map(std::string::ToString::to_string);
                        let concrete_variant_hash = compute_dependency_variant_hash(&concrete_dep);
                        let concrete_key = (
                            resource_type,
                            concrete_name.clone(),
                            concrete_source,
                            concrete_tool,
                            concrete_variant_hash.clone(),
                        );

                        if let std::collections::hash_map::Entry::Vacant(e) =
                            all_deps.entry(concrete_key)
                        {
                            e.insert(concrete_dep.clone());
                            queue.push((
                                concrete_name,
                                concrete_dep,
                                Some(resource_type),
                                concrete_variant_hash,
                            ));
                        }
                    }
                }
                Err(e) => {
                    anyhow::bail!("Failed to expand pattern '{}': {}", dep.get_path(), e);
                }
            }
            continue;
        }

        // Fetch resource content for metadata extraction
        let content = ResourceFetchingService::fetch_content(
            core,
            &dep,
            services.version_service,
            Some(resource_type),
        )
        .await
        .with_context(|| {
            format!("Failed to fetch resource '{}' ({}) for transitive deps", name, dep.get_path())
        })?;

        tracing::debug!("[TRANSITIVE] Fetched content for '{}' ({} bytes)", name, content.len());

        // Build complete template_vars including global project config for metadata extraction
        // This ensures transitive dependencies can use template variables like {{ agpm.project.language }}
        let variant_inputs_value =
            super::lockfile_builder::build_merged_variant_inputs(ctx.base.manifest, &dep);
        let variant_inputs = Some(&variant_inputs_value);

        // Extract metadata from the resource with complete variant_inputs
        let mut path = PathBuf::from(dep.get_path());

        // For skills, we read SKILL.md but the metadata extractor needs the correct file path
        if resource_type == crate::core::ResourceType::Skill {
            path.push("SKILL.md");
        }

        let metadata = MetadataExtractor::extract(
            &path,
            &content,
            variant_inputs,
            ctx.base.operation_context.map(|arc| arc.as_ref()),
        )?;

        tracing::debug!(
            "[DEBUG] Extracted metadata for '{}': has_deps={}, content_len={}",
            name,
            metadata.get_dependencies().is_some(),
            content.len()
        );

        // Process transitive dependencies if present
        if let Some(deps_map) = metadata.get_dependencies() {
            tracing::debug!(
                "[DEBUG] Found {} dependency type(s) for '{}': {:?}",
                deps_map.len(),
                name,
                deps_map.keys().collect::<Vec<_>>()
            );

            for (dep_resource_type_str, dep_specs) in deps_map {
                let dep_resource_type: ResourceType =
                    dep_resource_type_str.parse().unwrap_or(ResourceType::Snippet);

                for dep_spec in dep_specs {
                    // Process each transitive dependency spec
                    let (trans_dep, trans_name) = process_transitive_dependency_spec(
                        ctx,
                        core,
                        &dep,
                        dep_resource_type,
                        resource_type,
                        &name,
                        dep_spec,
                        services.version_service,
                        prepared_versions,
                    )
                    .await?;

                    let trans_source = trans_dep.get_source().map(std::string::ToString::to_string);
                    let trans_tool = trans_dep.get_tool().map(std::string::ToString::to_string);
                    let trans_variant_hash = compute_dependency_variant_hash(&trans_dep);

                    // Store custom name if provided
                    if let Some(custom_name) = &dep_spec.name {
                        let trans_key = (
                            dep_resource_type,
                            trans_name.clone(),
                            trans_source.clone(),
                            trans_tool.clone(),
                            trans_variant_hash.clone(),
                        );
                        ctx.transitive_custom_names.insert(trans_key, custom_name.clone());
                        tracing::debug!(
                            "Storing custom name '{}' for transitive dep '{}'",
                            custom_name,
                            trans_name
                        );
                    }

                    // Add to dependency graph
                    let from_node =
                        DependencyNode::with_source(resource_type, &name, source.clone());
                    let to_node = DependencyNode::with_source(
                        dep_resource_type,
                        &trans_name,
                        trans_source.clone(),
                    );
                    graph.add_dependency(from_node, to_node);

                    // Track in dependency map
                    let from_key = (
                        resource_type,
                        name.clone(),
                        source.clone(),
                        tool.clone(),
                        variant_hash.clone(),
                    );
                    let dep_ref =
                        LockfileDependencyRef::local(dep_resource_type, trans_name.clone(), None)
                            .to_string();
                    tracing::debug!(
                        "[DEBUG] Adding to dependency_map: parent='{}' (type={:?}, source={:?}, tool={:?}, hash={}), child='{}' (type={:?})",
                        name,
                        resource_type,
                        source,
                        tool,
                        &variant_hash[..8],
                        dep_ref,
                        dep_resource_type
                    );
                    ctx.dependency_map.entry(from_key).or_default().push(dep_ref);

                    // Add to conflict detector
                    add_to_conflict_detector(ctx, &trans_name, &trans_dep, &name);

                    // Check for version conflicts
                    let trans_key = (
                        dep_resource_type,
                        trans_name.clone(),
                        trans_source.clone(),
                        trans_tool.clone(),
                        trans_variant_hash.clone(),
                    );

                    tracing::debug!(
                        "[TRANSITIVE] Found transitive dep '{}' (type: {:?}, tool: {:?}, parent: {})",
                        trans_name,
                        dep_resource_type,
                        trans_tool,
                        name
                    );

                    // Check if we already have this dependency
                    if let std::collections::hash_map::Entry::Vacant(e) = all_deps.entry(trans_key)
                    {
                        // No conflict, add the dependency
                        tracing::debug!(
                            "Adding transitive dep '{}' (parent: {})",
                            trans_name,
                            name
                        );
                        e.insert(trans_dep.clone());
                        queue.push((
                            trans_name,
                            trans_dep,
                            Some(dep_resource_type),
                            trans_variant_hash,
                        ));
                    } else {
                        // Dependency already exists - conflict detector will handle version requirement conflicts
                        tracing::debug!(
                            "[TRANSITIVE] Skipping duplicate transitive dep '{}' (already processed)",
                            trans_name
                        );
                    }
                }
            }
        }
    }

    // Check for circular dependencies
    graph.detect_cycles()?;

    // Get topological order
    let ordered_nodes = graph.topological_order()?;

    // Build result with topologically ordered dependencies
    build_ordered_result(all_deps, ordered_nodes)
}
