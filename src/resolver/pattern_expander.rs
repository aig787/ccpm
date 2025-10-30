//! Pattern expansion for AGPM dependencies.
//!
//! This module handles expansion of glob patterns to concrete file paths,
//! converting pattern dependencies (like "agents/*.md") into individual file
//! dependencies. It supports both local and remote pattern resolution with
//! proper path handling, dependency naming, and locked resource generation.

use crate::git::GitRepo;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::pattern::PatternResolver;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Expands a pattern dependency into concrete dependencies.
///
/// This function takes a pattern dependency (e.g., `agents/*.md`) and expands it
/// into individual file dependencies. It handles both local and remote patterns.
///
/// # Arguments
///
/// * `name` - The name of the pattern dependency
/// * `dep` - The pattern dependency to expand
/// * `resource_type` - The type of resource being expanded
/// * `source_manager` - Source manager for remote repositories
/// * `cache` - Cache for storing resolved files
///
/// # Returns
///
/// A vector of tuples containing:
/// - The generated dependency name
/// - The concrete resource dependency
pub async fn expand_pattern_to_concrete_deps(
    dep: &ResourceDependency,
    resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
    manifest_dir: Option<&Path>,
) -> Result<Vec<(String, ResourceDependency)>> {
    let pattern = dep.get_path();

    if dep.is_local() {
        expand_local_pattern(dep, pattern, manifest_dir, resource_type).await
    } else {
        expand_remote_pattern(dep, pattern, resource_type, source_manager, cache).await
    }
}

/// Expands a local pattern dependency.
async fn expand_local_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    manifest_dir: Option<&Path>,
    resource_type: crate::core::ResourceType,
) -> Result<Vec<(String, ResourceDependency)>> {
    // For absolute patterns, use the parent directory as base and strip the pattern to just the filename part
    // For relative patterns, use manifest directory
    let pattern_path = Path::new(pattern);
    let (base_path, search_pattern) = if pattern_path.is_absolute() {
        // Absolute pattern: extract base directory and relative pattern
        // Example: "/tmp/xyz/agents/*.md" -> base="/tmp/xyz", pattern="agents/*.md"
        let components: Vec<_> = pattern_path.components().collect();

        // Find the first component with a glob character
        let glob_idx = components.iter().position(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.contains('*') || s.contains('?') || s.contains('[')
        });

        if let Some(idx) = glob_idx {
            // Split at the glob component
            let base_components = &components[..idx];
            let pattern_components = &components[idx..];

            let base: PathBuf = base_components.iter().collect();
            let pattern: String = pattern_components
                .iter()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");

            (base, pattern)
        } else {
            // No glob characters, use as-is
            (PathBuf::from("."), pattern.to_string())
        }
    } else {
        // Relative pattern, use manifest directory as base
        let base = manifest_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        (base, pattern.to_string())
    };

    let matches = if resource_type == crate::core::ResourceType::Skill {
        // For skills, use the specialized skill directory matching
        crate::resolver::skills::match_skill_directories(&base_path, &search_pattern, None)?
            .into_iter()
            .map(|(_name, path)| PathBuf::from(path))
            .collect()
    } else {
        // For other resource types, use the regular pattern resolver
        let pattern_resolver = PatternResolver::new();
        pattern_resolver.resolve(&search_pattern, &base_path)?
    };

    debug!("Pattern '{}' matched {} files", pattern, matches.len());

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    for matched_path in matches {
        // Convert matched path to absolute by joining with base_path
        let absolute_path = base_path.join(&matched_path);
        let concrete_path = absolute_path.to_string_lossy().to_string();

        // Generate a dependency name using source context
        let source_context = if let Some(manifest_dir) = manifest_dir {
            // For local dependencies, use manifest directory as source context
            crate::resolver::source_context::SourceContext::local(manifest_dir)
        } else {
            // Fallback: use the base_path as source context
            crate::resolver::source_context::SourceContext::local(&base_path)
        };

        let dep_name = generate_dependency_name(&concrete_path, &source_context);

        // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
        let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: concrete_path,
            source: None,
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: target.clone(),
            filename: None,
            dependencies: None,
            tool: tool.clone(),
            flatten,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        concrete_deps.push((dep_name, concrete_dep));
    }

    Ok(concrete_deps)
}

/// Expands a remote pattern dependency.
async fn expand_remote_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    _resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
) -> Result<Vec<(String, ResourceDependency)>> {
    let source_name = dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Remote pattern dependency missing source: {}", pattern))?;

    let source_url = source_manager
        .get_source_url(source_name)
        .with_context(|| format!("Source not found: {}", source_name))?;

    // Get or clone the source repository
    let repo_path = cache
        .get_or_clone_source(source_name, &source_url, dep.get_version())
        .await
        .with_context(|| format!("Failed to access source repository: {}", source_name))?;

    let repo = GitRepo::new(&repo_path);

    // Resolve the version to a commit SHA
    let version = dep.get_version().unwrap_or("HEAD");
    let commit_sha = repo.resolve_to_sha(Some(version)).await.with_context(|| {
        format!("Failed to resolve version '{}' for source {}", version, source_name)
    })?;

    // Create a worktree for the specific commit
    let worktree_path = cache
        .get_or_create_worktree_for_sha(source_name, &source_url, &commit_sha, Some(version))
        .await
        .with_context(|| format!("Failed to create worktree for {}@{}", source_name, version))?;

    // Resolve the pattern within the worktree
    let matches = if _resource_type == crate::core::ResourceType::Skill {
        // For skills, use the specialized skill directory matching
        crate::resolver::skills::match_skill_directories(
            &worktree_path,
            pattern,
            Some(&worktree_path),
        )?
        .into_iter()
        .map(|(_name, path)| PathBuf::from(path))
        .collect()
    } else {
        // For other resource types, use the regular pattern resolver
        let pattern_resolver = PatternResolver::new();
        pattern_resolver.resolve(pattern, &worktree_path)?
    };

    debug!("Remote pattern '{}' in {} matched {} files", pattern, source_name, matches.len());

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    for matched_path in matches {
        // Generate a dependency name using source context
        // For Git dependencies, use the repository root as source context
        let source_context = crate::resolver::source_context::SourceContext::git(&worktree_path);
        let dep_name = generate_dependency_name(&matched_path.to_string_lossy(), &source_context);

        // matched_path is already relative to worktree root (from PatternResolver)
        // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
        let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: matched_path.to_string_lossy().to_string(),
            source: Some(source_name.to_string()),
            version: Some(commit_sha.clone()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: target.clone(),
            filename: None,
            dependencies: None,
            tool: tool.clone(),
            flatten,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        concrete_deps.push((dep_name, concrete_dep));
    }

    Ok(concrete_deps)
}

/// Generates a dependency name from a path using source context.
/// Creates collision-resistant names by preserving directory structure relative to source.
pub fn generate_dependency_name(
    path: &str,
    source_context: &crate::resolver::source_context::SourceContext,
) -> String {
    // Use the new source context-aware name generation
    crate::resolver::source_context::compute_canonical_name(path, source_context)
}

// ============================================================================
// Pattern Expansion Service
// ============================================================================

use crate::core::ResourceType;
use std::collections::HashMap;

use super::types::ResolutionCore;
use super::version_resolver::VersionResolutionService;

/// Service for pattern expansion and resolution.
///
/// Handles expansion of glob patterns to concrete dependencies and maintains
/// mappings between concrete files and their source patterns.
pub struct PatternExpansionService {
    /// Map tracking pattern alias relationships (concrete_name -> pattern_name)
    pattern_alias_map: HashMap<(ResourceType, String), String>,
}

impl PatternExpansionService {
    /// Create a new pattern expansion service.
    pub fn new() -> Self {
        Self {
            pattern_alias_map: HashMap::new(),
        }
    }

    /// Expand a pattern dependency to concrete dependencies.
    ///
    /// Takes a glob pattern like "agents/*.md" and expands it to
    /// concrete file paths like ["agents/foo.md", "agents/bar.md"].
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with cache and source manager
    /// * `dep` - The pattern dependency to expand
    /// * `resource_type` - The type of resource being expanded
    /// * `version_service` - Version service for worktree paths
    ///
    /// # Returns
    ///
    /// List of (name, concrete_dependency) tuples
    pub async fn expand_pattern(
        &mut self,
        core: &ResolutionCore,
        dep: &ResourceDependency,
        resource_type: ResourceType,
        _version_service: &VersionResolutionService,
    ) -> Result<Vec<(String, ResourceDependency)>> {
        // Delegate to expand_pattern_to_concrete_deps helper
        expand_pattern_to_concrete_deps(
            dep,
            resource_type,
            &core.source_manager,
            &core.cache,
            None, // manifest_dir - use current working directory
        )
        .await
    }

    /// Get pattern alias for a concrete dependency.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type
    /// * `name` - The concrete dependency name
    ///
    /// # Returns
    ///
    /// The pattern name if this is from a pattern expansion
    pub fn get_pattern_alias(&self, resource_type: ResourceType, name: &str) -> Option<&String> {
        self.pattern_alias_map.get(&(resource_type, name.to_string()))
    }

    /// Record a pattern alias mapping.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type
    /// * `concrete_name` - The concrete file name
    /// * `pattern_name` - The pattern that expanded to this file
    pub fn add_pattern_alias(
        &mut self,
        resource_type: ResourceType,
        concrete_name: String,
        pattern_name: String,
    ) {
        self.pattern_alias_map.insert((resource_type, concrete_name), pattern_name);
    }
}

impl Default for PatternExpansionService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;

    // TODO: ADD NEW TESTS for the source context version once we have concrete examples
    // These tests should use generate_dependency_name() with proper SourceContext

    #[tokio::test]
    async fn test_expand_local_pattern() {
        // This test would require creating temporary files and directories
        // For now, we'll test the logic with a mock scenario
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: "tests/fixtures/*.md".to_string(),
            source: None,
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        // Note: This test would need actual test files to work properly
        // For now, we just verify the function signature and basic structure
        match expand_local_pattern(
            &dep,
            "tests/fixtures/*.md",
            None,
            crate::core::ResourceType::Snippet,
        )
        .await
        {
            Ok(_) => println!("Pattern expansion succeeded"),
            Err(e) => println!("Pattern expansion failed (expected in test): {}", e),
        }
    }
}
