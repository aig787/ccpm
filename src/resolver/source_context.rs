//! Source context for dependency name generation.
//!
//! This module provides types and utilities for handling different source contexts
//! when generating canonical names for dependencies. It distinguishes between
//! local filesystem paths, Git repositories, and other remote sources.

use crate::manifest::ResourceDependency;
use crate::utils::{compute_relative_path, normalize_path_for_storage};
use std::path::{Path, PathBuf};

/// Context for determining how to generate canonical dependency names.
///
/// Different source contexts require different naming strategies:
/// - Local dependencies use paths relative to the manifest directory
/// - Git dependencies use paths relative to the repository root
/// - Remote dependencies may have other naming conventions
#[derive(Debug, Clone)]
pub enum SourceContext {
    /// Local filesystem dependency relative to manifest directory
    Local(PathBuf),
    /// Git repository dependency with repository root path
    Git(PathBuf),
    /// Remote source with source name (for backward compatibility)
    Remote(String),
}

impl SourceContext {
    /// Create a local source context from a manifest directory path
    pub fn local(manifest_dir: impl Into<PathBuf>) -> Self {
        Self::Local(manifest_dir.into())
    }

    /// Create a Git source context from a repository root path
    pub fn git(repo_root: impl Into<PathBuf>) -> Self {
        Self::Git(repo_root.into())
    }

    /// Create a remote source context from a source name
    pub fn remote(source_name: impl Into<String>) -> Self {
        Self::Remote(source_name.into())
    }

    /// Check if this context represents a local source
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Check if this context represents a Git source
    pub fn is_git(&self) -> bool {
        matches!(self, Self::Git(_))
    }

    /// Check if this context represents a remote source
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

/// Compute a canonical dependency name relative to the appropriate source base.
///
/// This function generates canonical names based on the source context:
/// - Local: paths relative to manifest directory
/// - Git: paths relative to repository root
/// - Remote: paths relative to source name (for backward compatibility)
pub fn compute_canonical_name(path: &str, source_context: &SourceContext) -> String {
    let path = Path::new(path);

    // Remove file extension
    let without_ext = path.with_extension("");

    match source_context {
        SourceContext::Local(manifest_dir) => {
            // For local dependencies, try to strip the manifest directory prefix
            // If it strips successfully, the path was absolute (or rooted)
            // If it fails, the path is already relative
            let manifest_path = Path::new(manifest_dir);
            let relative = without_ext.strip_prefix(manifest_path).unwrap_or(&without_ext);
            normalize_path_for_storage(relative)
        }
        SourceContext::Git(repo_root) => compute_relative_to_repo(&without_ext, repo_root),
        SourceContext::Remote(_source_name) => {
            // For remote sources, use full path relative to repository root
            // This preserves the directory structure (e.g., "agents/helper.md" -> "agents/helper")
            // Uniqueness is ensured by (name, source) tuple
            normalize_path_for_storage(&without_ext)
        }
    }
}

/// Create appropriate source context for a resource dependency.
///
/// This function determines the correct source context based on the available
/// information in the dependency and manifest context.
pub fn create_source_context_for_dependency(
    dep: &ResourceDependency,
    manifest_dir: Option<&Path>,
    repo_root: Option<&Path>,
) -> SourceContext {
    // Priority: Git context > Local context > Remote context
    if let Some(source_name) = dep.get_source() {
        // Git-backed dependency
        if let Some(repo_root) = repo_root {
            SourceContext::git(repo_root)
        } else {
            // Fallback to remote context when repo root is not available
            SourceContext::remote(source_name)
        }
    } else if let Some(manifest_dir) = manifest_dir {
        // Local dependency
        SourceContext::local(manifest_dir)
    } else {
        // Last resort - use remote context with "unknown" source
        SourceContext::remote("unknown")
    }
}

/// Create source context from a locked resource.
///
/// This function determines the correct source context from a LockedResource.
pub fn create_source_context_from_locked_resource(
    resource: &crate::lockfile::LockedResource,
    manifest_dir: Option<&Path>,
) -> SourceContext {
    if let Some(source_name) = &resource.source {
        // Remote resource - use source name context
        SourceContext::remote(source_name.clone())
    } else {
        // Local resource - use manifest directory if available
        if let Some(manifest_dir) = manifest_dir {
            SourceContext::local(manifest_dir)
        } else {
            // Fallback - use local context with current directory
            SourceContext::local(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        }
    }
}

/// Compute relative path from Git repository root to the dependency file.
///
/// For Git dependencies, we want to preserve the repository structure.
fn compute_relative_to_repo(file_path: &Path, repo_root: &Path) -> String {
    // Use the existing utility function which properly handles Path operations
    compute_relative_path(repo_root, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    #[test]
    fn test_source_context_creation() {
        let local = SourceContext::local("/project");
        assert!(local.is_local());
        assert!(!local.is_git());
        assert!(!local.is_remote());

        let git = SourceContext::git("/repo");
        assert!(!git.is_local());
        assert!(git.is_git());
        assert!(!git.is_remote());

        let remote = SourceContext::remote("community");
        assert!(!remote.is_local());
        assert!(!remote.is_git());
        assert!(remote.is_remote());
    }

    #[test]
    fn test_compute_canonical_name_integration() {
        // Local context - the path is relative to manifest directory
        let local_ctx = SourceContext::local("/project");
        let name = compute_canonical_name("/project/local-deps/agents/helper.md", &local_ctx);
        assert_eq!(name, "local-deps/agents/helper");

        // Git context
        let git_ctx = SourceContext::git("/repo");
        let name = compute_canonical_name("/repo/agents/helper.md", &git_ctx);
        assert_eq!(name, "agents/helper");

        // Remote context - preserves full repo-relative path
        let remote_ctx = SourceContext::remote("community");
        let name = compute_canonical_name("agents/helper.md", &remote_ctx);
        assert_eq!(name, "agents/helper");
    }

    #[test]
    fn test_compute_canonical_name_with_already_relative_path() {
        // Regression test for Windows bug where relative paths were being passed to
        // compute_relative_path, causing it to generate incorrect paths with ../../..
        //
        // When trans_dep.get_path() returns an already-relative path like
        // "local-deps/snippets/agents/helper.md", it should not be passed through
        // compute_relative_path again, as that function expects absolute paths.
        let local_ctx = SourceContext::local("/project");

        // Test with a relative path (like what trans_dep.get_path() returns)
        let name = compute_canonical_name("local-deps/snippets/agents/helper.md", &local_ctx);
        assert_eq!(name, "local-deps/snippets/agents/helper");

        // Verify it doesn't generate paths with ../../../
        assert!(!name.contains(".."), "Generated name should not contain '..' sequences");

        // Test with nested relative path
        let name = compute_canonical_name("local-deps/claude/agents/rust-expert.md", &local_ctx);
        assert_eq!(name, "local-deps/claude/agents/rust-expert");
        assert!(!name.contains(".."));
    }

    // Tests for helper functions
    #[test]
    fn test_create_source_context_for_dependency() {
        use crate::manifest::{DetailedDependency, ResourceDependency};

        // Local dependency
        let local_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: "agents/helper.md".to_string(),
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
            template_vars: None,
        }));

        let manifest_dir = Path::new("/project");
        let ctx = create_source_context_for_dependency(&local_dep, Some(manifest_dir), None);
        assert!(ctx.is_local());

        // Git dependency with repo root
        let git_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: "agents/helper.md".to_string(),
            source: Some("community".to_string()),
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
            template_vars: None,
        }));

        let repo_root = Path::new("/repo");
        let ctx =
            create_source_context_for_dependency(&git_dep, Some(manifest_dir), Some(repo_root));
        assert!(ctx.is_git());

        // Git dependency without repo root (fallback to remote)
        let ctx = create_source_context_for_dependency(&git_dep, Some(manifest_dir), None);
        assert!(ctx.is_remote());
    }

    #[test]
    fn test_create_source_context_from_locked_resource() {
        use crate::lockfile::LockedResource;

        // Local resource
        let local_resource = LockedResource {
            name: "helper".to_string(),
            source: None,
            url: None,
            path: "agents/helper.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "abc123".to_string(),
            installed_at: "agents/helper.md".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        };

        let manifest_dir = Path::new("/project");
        let ctx = create_source_context_from_locked_resource(&local_resource, Some(manifest_dir));
        assert!(ctx.is_local());

        // Remote resource
        let mut remote_resource = local_resource.clone();
        remote_resource.source = Some("community".to_string());

        let ctx = create_source_context_from_locked_resource(&remote_resource, Some(manifest_dir));
        assert!(ctx.is_remote());
        assert_eq!(format!("{:?}", ctx), "Remote(\"community\")");
    }
}
