//! Display dependency trees for installed resources.
//!
//! This module provides the `tree` command which visualizes dependencies and their
//! transitive dependencies in a hierarchical tree format, similar to `cargo tree`.
//! It helps users understand the dependency graph and identify duplicate or
//! redundant dependencies.
//!
//! # Features
//!
//! - **Hierarchical Display**: Shows dependencies in a tree structure
//! - **Transitive Dependencies**: Visualizes the full dependency graph
//! - **Deduplication**: Marks duplicate dependencies with (*)
//! - **Filtering**: Filter by resource type (agents, snippets, commands, etc.)
//! - **Multiple Formats**: Tree, JSON, and text output formats
//! - **Depth Limiting**: Control how deep to traverse the tree
//! - **Colored Output**: Uses colors to highlight different elements
//!
//! # Examples
//!
//! Display the full dependency tree:
//! ```bash
//! agpm tree
//! ```
//!
//! Limit tree depth:
//! ```bash
//! agpm tree --depth 2
//! ```
//!
//! Show tree for a specific package:
//! ```bash
//! agpm tree --package my-agent
//! ```
//!
//! Show only duplicates:
//! ```bash
//! agpm tree --duplicates
//! ```
//!
//! Output as JSON:
//! ```bash
//! agpm tree --format json
//! ```
//!
//! # Output Format
//!
//! ## Tree Format (Default)
//! ```text
//! my-project
//! ├── agent/code-reviewer v1.0.0 (community)
//! │   ├── agent/rust-helper v1.0.0 (community)
//! │   └── snippet/utils v2.1.0 (community)
//! ├── command/git-commit v1.0.0 (local)
//! │   ├── agent/rust-helper v1.0.0 (community) (*)
//! │   └── snippet/commit-msg v1.0.0 (local)
//! └── snippet/logging v1.5.0 (community)
//!
//! (*) = duplicate dependency
//! ```

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::cache::Cache;
use crate::core::ResourceType;
use crate::lockfile::patch_display::extract_patch_displays;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::find_manifest_with_optional;

/// Command to display dependency trees.
///
/// This command reads the lockfile to show the complete dependency tree,
/// including transitive dependencies. It provides various filtering and
/// formatting options to help users understand their dependency structure.
#[derive(Args, Debug)]
pub struct TreeCommand {
    /// Maximum depth to display (unlimited if not specified)
    ///
    /// Limits how many levels deep the tree will traverse. This is useful
    /// for large dependency graphs where you only want to see top-level
    /// dependencies.
    ///
    /// # Examples
    ///
    /// ```bash
    /// agpm tree --depth 1    # Show only direct dependencies
    /// agpm tree --depth 3    # Show up to 3 levels
    /// ```
    #[arg(short = 'd', long)]
    depth: Option<usize>,

    /// Output format (tree, json, text)
    ///
    /// Controls how the dependency information is displayed:
    /// - `tree`: Hierarchical tree with box-drawing characters (default)
    /// - `json`: JSON format for scripting and programmatic access
    /// - `text`: Simple indented text format
    #[arg(short = 'f', long, default_value = "tree")]
    format: String,

    /// Show only duplicate dependencies
    ///
    /// When enabled, only shows dependencies that appear multiple times
    /// in the tree. This helps identify redundant dependencies.
    #[arg(long)]
    duplicates: bool,

    /// Don't deduplicate repeated dependencies
    ///
    /// By default, repeated dependencies are marked with (*) and only
    /// shown in full the first time. This flag shows them in full every time.
    #[arg(long)]
    no_dedupe: bool,

    /// Show tree for specific package only
    ///
    /// Displays the dependency tree starting from the specified package.
    /// The package name should match a dependency name from the manifest.
    ///
    /// # Examples
    ///
    /// ```bash
    /// agpm tree --package my-agent
    /// agpm tree -p code-reviewer
    /// ```
    #[arg(short = 'p', long)]
    package: Option<String>,

    /// Show only agents
    #[arg(long)]
    agents: bool,

    /// Show only snippets
    #[arg(long)]
    snippets: bool,

    /// Show only commands
    #[arg(long)]
    commands: bool,

    /// Show only scripts
    #[arg(long)]
    scripts: bool,

    /// Show only hooks
    #[arg(long)]
    hooks: bool,

    /// Show only MCP servers
    #[arg(long, name = "mcp-servers")]
    mcp_servers: bool,

    /// Show only skills
    #[arg(long)]
    skills: bool,

    /// Invert tree to show what depends on each package
    ///
    /// Instead of showing what each package depends on, shows what depends
    /// on each package. Useful for understanding the impact of changes.
    #[arg(short = 'i', long)]
    invert: bool,

    /// Show detailed information including installation paths and applied patches
    ///
    /// When enabled, displays the absolute installation path for each resource
    /// and lists all applied patches with their values. This is useful for
    /// debugging patch application and understanding where resources are installed.
    ///
    /// # Examples
    ///
    /// ```bash
    /// agpm tree --detailed
    /// ```
    #[arg(long)]
    detailed: bool,
}

impl TreeCommand {
    /// Execute the tree command with an optional manifest path.
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // Find manifest file
        let manifest_path = find_manifest_with_optional(manifest_path)
            .context("No agpm.toml found. Please create one to define your dependencies.")?;

        self.execute_from_path(manifest_path).await
    }

    async fn execute_from_path(self, manifest_path: PathBuf) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // Require the manifest to exist
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!("Manifest file {} not found", manifest_path.display()));
        }

        let project_dir = manifest_path.parent().unwrap();
        let lockfile_path = project_dir.join("agpm.lock");

        // Derive project name from directory
        let project_name =
            project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("project").to_string();

        // Check if lockfile exists
        if !lockfile_path.exists() {
            if self.format == "json" {
                println!("{{}}");
            } else {
                println!("No lockfile found.");
                println!("⚠️  Run 'agpm install' first to generate agpm.lock");
            }
            return Ok(());
        }

        // Create command context for enhanced lockfile loading
        let manifest_path = project_dir.join("agpm.toml");
        let manifest = crate::manifest::Manifest::load(&manifest_path)?;
        let command_context =
            crate::cli::common::CommandContext::new(manifest, project_dir.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let lockfile = match command_context.load_lockfile_with_regeneration(true, "tree")? {
            Some(lockfile) => lockfile,
            None => {
                // Lockfile was regenerated and doesn't exist yet
                if self.format == "json" {
                    println!("{{}}");
                } else {
                    println!("No lockfile found.");
                    println!(
                        "⚠️  Lockfile was invalid and has been removed. Run 'agpm install' to regenerate it."
                    );
                }
                return Ok(());
            }
        };

        // Create cache if needed for detailed mode with patches
        let cache = if self.detailed {
            Some(Cache::new().context("Failed to initialize cache")?)
        } else {
            None
        };

        // Build and display tree
        let builder = TreeBuilder::new(&lockfile, project_name);
        let tree = builder.build(&self)?;

        match self.format.as_str() {
            "json" => self.output_json(&tree)?,
            "text" => self.output_text(&tree),
            _ => self.output_tree_with_cache(&tree, &lockfile, cache.as_ref()).await,
        }

        Ok(())
    }

    fn validate_arguments(&self) -> Result<()> {
        // Validate format
        match self.format.as_str() {
            "tree" | "json" | "text" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid format '{}'. Valid formats are: tree, json, text",
                    self.format
                ));
            }
        }

        // Validate depth
        if let Some(depth) = self.depth
            && depth == 0
        {
            return Err(anyhow::anyhow!("Depth must be at least 1"));
        }

        Ok(())
    }

    /// Check if a resource type should be shown based on filters
    const fn should_show_resource_type(&self, resource_type: ResourceType) -> bool {
        // If no type filters are set, show all types
        let any_filter = self.agents
            || self.snippets
            || self.commands
            || self.scripts
            || self.hooks
            || self.mcp_servers;

        if !any_filter {
            return true;
        }

        // Check individual flags
        match resource_type {
            ResourceType::Agent => self.agents,
            ResourceType::Snippet => self.snippets,
            ResourceType::Command => self.commands,
            ResourceType::Script => self.scripts,
            ResourceType::Hook => self.hooks,
            ResourceType::McpServer => self.mcp_servers,
            ResourceType::Skill => self.skills,
        }
    }

    async fn output_tree_with_cache(
        &self,
        tree: &DependencyTree,
        lockfile: &LockFile,
        cache: Option<&Cache>,
    ) {
        if tree.roots.is_empty() {
            println!("No dependencies found.");
            return;
        }

        // Print project root
        println!("{}", tree.project_name.cyan().bold());

        // Track which nodes we've already displayed in full
        let mut displayed = HashSet::new();

        for (i, root) in tree.roots.iter().enumerate() {
            let is_last = i == tree.roots.len() - 1;
            self.print_node_with_cache(root, "", is_last, &mut displayed, tree, 0, lockfile, cache)
                .await;
        }

        // Print legend if there are duplicates
        if !self.no_dedupe && tree.has_duplicates() {
            println!();
            println!("{}", "(*) = duplicate dependency (already shown above)".blue());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn print_node_with_cache<'a>(
        &'a self,
        node: &'a TreeNode,
        prefix: &'a str,
        is_last: bool,
        displayed: &'a mut HashSet<String>,
        tree: &'a DependencyTree,
        current_depth: usize,
        lockfile: &'a LockFile,
        cache: Option<&'a Cache>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
        Box::pin(async move {
            // Check depth limit
            if let Some(max_depth) = self.depth
                && current_depth >= max_depth
            {
                return;
            }

            let node_id = format!("{}/{}", node.resource_type, node.name);
            let is_duplicate = !self.no_dedupe && displayed.contains(&node_id);

            // Print connector
            let connector = if is_last {
                "└── "
            } else {
                "├── "
            };

            // Format node display: type/name version (source) [tool] (patched)
            // Use blue for secondary text - readable on both light and dark backgrounds
            let type_str = format!("{}", node.resource_type).blue();
            let name_str = node.name.cyan();
            let version_str =
                node.version.as_deref().map(|v| format!(" {}", v.blue())).unwrap_or_default();
            let source_str = node
                .source
                .as_deref()
                .map_or_else(|| " (local)".blue().to_string(), |s| format!(" ({})", s.blue()));
            let tool_str = node
                .tool
                .as_deref()
                .map(|tool| format!(" [{}]", tool.bright_yellow()))
                .unwrap_or_default();
            let patch_marker = if node.has_patches {
                format!(" {}", "(patched)".blue())
            } else {
                String::new()
            };
            let dup_marker = if is_duplicate {
                " (*)".blue().to_string()
            } else {
                String::new()
            };

            println!(
                "{prefix}{connector}{type_str}/{name_str}{version_str}{source_str}{tool_str}{patch_marker}{dup_marker}"
            );

            // Show detailed information if --detailed flag is set
            if self.detailed && !is_duplicate {
                let detail_prefix = if is_last {
                    format!("{prefix}    ")
                } else {
                    format!("{prefix}│   ")
                };

                // Show installation path
                if !node.installed_at.is_empty() {
                    println!(
                        "{}    {}: {}",
                        detail_prefix,
                        "Installed at".bright_yellow(),
                        node.installed_at.bright_black()
                    );
                }

                // Show applied patches with original → overridden comparison
                if !node.applied_patches.is_empty() {
                    println!("{}    {}", detail_prefix, "Applied patches:".bright_yellow());

                    // If we have cache, try to get original values
                    if let Some(cache) = cache {
                        // Find the locked resource for this node
                        if let Some(locked_resource) =
                            self.find_locked_resource_for_node(node, lockfile)
                        {
                            let patch_displays =
                                extract_patch_displays(locked_resource, cache).await;
                            for display in patch_displays {
                                let formatted = display.format();
                                // Indent each line of the multi-line diff output
                                for (i, line) in formatted.lines().enumerate() {
                                    if i == 0 {
                                        // First line: bullet point
                                        println!("{}       • {}", detail_prefix, line);
                                    } else {
                                        // Subsequent lines: indent to align with content
                                        println!("{}         {}", detail_prefix, line);
                                    }
                                }
                            }
                        } else {
                            // Fallback: just show overridden values
                            self.print_patches_fallback_tree(&node.applied_patches, &detail_prefix);
                        }
                    } else {
                        // No cache: just show overridden values
                        self.print_patches_fallback_tree(&node.applied_patches, &detail_prefix);
                    }
                }
            }

            // If this is a duplicate and we're deduplicating, don't show children
            if is_duplicate {
                return;
            }

            // Mark as displayed
            displayed.insert(node_id);

            // Print children
            if !node.dependencies.is_empty() {
                let child_prefix = if is_last {
                    format!("{prefix}    ")
                } else {
                    format!("{prefix}│   ")
                };

                for (i, dep_id) in node.dependencies.iter().enumerate() {
                    if let Some(child_node) = tree.nodes.get(dep_id) {
                        let is_last_child = i == node.dependencies.len() - 1;
                        self.print_node_with_cache(
                            child_node,
                            &child_prefix,
                            is_last_child,
                            displayed,
                            tree,
                            current_depth + 1,
                            lockfile,
                            cache,
                        )
                        .await;
                    }
                }
            }
        })
    }

    /// Fallback patch display without original values for tree output
    fn print_patches_fallback_tree(&self, patches: &BTreeMap<String, toml::Value>, prefix: &str) {
        let mut patch_keys: Vec<_> = patches.keys().collect();
        patch_keys.sort();
        for key in patch_keys {
            let value = &patches[key];
            let value_str = format_patch_value(value);
            println!("{}       • {}: {}", prefix, key.blue(), value_str);
        }
    }

    /// Find the locked resource corresponding to a tree node
    fn find_locked_resource_for_node<'a>(
        &self,
        node: &TreeNode,
        lockfile: &'a LockFile,
    ) -> Option<&'a LockedResource> {
        // Find matching resource in lockfile by name and resource type
        lockfile.get_resources(&node.resource_type).iter().find(|r| {
            // Extract display name from locked resource's unique name
            let display_name = TreeBuilder::extract_display_name(&r.name);
            display_name == node.name && r.source == node.source && r.version == node.version
        })
    }

    fn output_json(&self, tree: &DependencyTree) -> Result<()> {
        let json = serde_json::json!({
            "project": tree.project_name,
            "roots": tree.roots.iter().map(|n| self.node_to_json(n, tree, 0)).collect::<Vec<_>>(),
        });

        println!("{}", serde_json::to_string_pretty(&json)?);
        Ok(())
    }

    fn node_to_json(
        &self,
        node: &TreeNode,
        tree: &DependencyTree,
        depth: usize,
    ) -> serde_json::Value {
        // Check depth limit
        let children = if let Some(max_depth) = self.depth {
            if depth >= max_depth {
                vec![]
            } else {
                node.dependencies
                    .iter()
                    .filter_map(|id| tree.nodes.get(id))
                    .map(|child| self.node_to_json(child, tree, depth + 1))
                    .collect()
            }
        } else {
            node.dependencies
                .iter()
                .filter_map(|id| tree.nodes.get(id))
                .map(|child| self.node_to_json(child, tree, depth + 1))
                .collect()
        };

        serde_json::json!({
            "name": node.name,
            "type": node.resource_type.to_string(),
            "version": node.version,
            "source": node.source,
            "tool": node.tool.as_deref().unwrap_or("claude-code"),
            "has_patches": node.has_patches,
            "dependencies": children,
        })
    }

    fn output_text(&self, tree: &DependencyTree) {
        if tree.roots.is_empty() {
            println!("No dependencies found.");
            return;
        }

        println!("{}", tree.project_name);

        let mut displayed = HashSet::new();
        for root in &tree.roots {
            self.print_text_node(root, 0, &mut displayed, tree, 0);
        }
    }

    fn print_text_node(
        &self,
        node: &TreeNode,
        indent: usize,
        displayed: &mut HashSet<String>,
        tree: &DependencyTree,
        current_depth: usize,
    ) {
        // Check depth limit
        if let Some(max_depth) = self.depth
            && current_depth >= max_depth
        {
            return;
        }

        let node_id = format!("{}/{}", node.resource_type, node.name);
        let is_duplicate = !self.no_dedupe && displayed.contains(&node_id);

        let indent_str = "  ".repeat(indent);
        let version_str = node.version.as_deref().unwrap_or("latest");
        let source_str = node.source.as_deref().unwrap_or("local");
        let patch_marker = if node.has_patches {
            format!(" {}", "(patched)".blue())
        } else {
            String::new()
        };
        let dup_marker = if is_duplicate {
            " (*)"
        } else {
            ""
        };

        println!(
            "{}{}/{} {} ({}) {}{}{}",
            indent_str,
            node.resource_type,
            node.name,
            version_str,
            source_str,
            node.tool.as_deref().map(|tool| format!("[{}] ", tool)).unwrap_or_default(),
            patch_marker,
            dup_marker
        );

        if is_duplicate {
            return;
        }

        displayed.insert(node_id);

        for dep_id in &node.dependencies {
            if let Some(child_node) = tree.nodes.get(dep_id) {
                self.print_text_node(child_node, indent + 1, displayed, tree, current_depth + 1);
            }
        }
    }
}

/// A node in the dependency tree
#[derive(Debug, Clone)]
struct TreeNode {
    name: String,
    resource_type: ResourceType,
    version: Option<String>,
    source: Option<String>,
    tool: Option<String>,
    dependencies: Vec<String>, // IDs of dependency nodes
    has_patches: bool,         // True if resource has applied patches
    installed_at: String,      // Installation path for detailed output
    applied_patches: std::collections::BTreeMap<String, toml::Value>, // Patch field -> value mapping
}

/// The complete dependency tree structure
#[derive(Debug)]
struct DependencyTree {
    project_name: String,
    nodes: HashMap<String, TreeNode>,
    roots: Vec<TreeNode>,
}

impl DependencyTree {
    fn has_duplicates(&self) -> bool {
        let mut seen = HashSet::new();
        for root in &self.roots {
            if self.has_duplicates_recursive(root, &mut seen) {
                return true;
            }
        }
        false
    }

    fn has_duplicates_recursive(&self, node: &TreeNode, seen: &mut HashSet<String>) -> bool {
        let node_id = format!("{}/{}", node.resource_type, node.name);

        if !seen.insert(node_id) {
            return true;
        }

        for dep_id in &node.dependencies {
            if let Some(child) = self.nodes.get(dep_id)
                && self.has_duplicates_recursive(child, seen)
            {
                return true;
            }
        }

        false
    }
}

/// Builds the dependency tree from the lockfile
struct TreeBuilder<'a> {
    lockfile: &'a LockFile,
    project_name: String,
}

impl<'a> TreeBuilder<'a> {
    const fn new(lockfile: &'a LockFile, project_name: String) -> Self {
        Self {
            lockfile,
            project_name,
        }
    }

    fn build(&self, cmd: &TreeCommand) -> Result<DependencyTree> {
        let mut nodes = HashMap::new();
        let mut roots = Vec::new();

        // If a specific package is requested, find it
        if let Some(ref package_name) = cmd.package {
            let found = self.find_package(package_name)?;
            let node = self.build_node(found, cmd)?;
            let node_id = self.node_id(&node);

            nodes.insert(node_id, node.clone());
            self.build_dependencies(&node, &mut nodes, cmd)?;
            roots.push(node);
        } else {
            // First, build all nodes and their dependencies
            for resource_type in ResourceType::all() {
                if !cmd.should_show_resource_type(*resource_type) {
                    continue;
                }

                for resource in self.lockfile.get_resources(resource_type) {
                    let node = self.build_node(resource, cmd)?;
                    let node_id = self.node_id(&node);

                    nodes.insert(node_id.clone(), node.clone());
                    self.build_dependencies(&node, &mut nodes, cmd)?;
                }
            }

            // Determine if any resource type filters are active
            let has_type_filter = cmd.agents
                || cmd.snippets
                || cmd.commands
                || cmd.scripts
                || cmd.hooks
                || cmd.mcp_servers;

            if has_type_filter {
                // When filtering by resource type, show ALL resources of that type as roots
                // (don't exclude dependencies)
                for node in nodes.values() {
                    if cmd.should_show_resource_type(node.resource_type) {
                        roots.push(node.clone());
                    }
                }
            } else {
                // Normal mode: identify roots as resources that are NOT dependencies of anything else
                // Build a set of all dependency IDs (already in singular "type/name" format)
                let mut all_dependencies = HashSet::new();
                for resource_type in ResourceType::all() {
                    for resource in self.lockfile.get_resources(resource_type) {
                        for dep_id in &resource.dependencies {
                            // Dependencies are already in singular form (e.g., "agent/foo")
                            all_dependencies.insert(dep_id.clone());
                        }
                    }
                }

                // Roots are nodes that are not in the dependencies set
                // All IDs use singular "type/name" format
                for node in nodes.values() {
                    let simple_id = format!("{}/{}", node.resource_type, node.name);
                    if !all_dependencies.contains(&simple_id) {
                        roots.push(node.clone());
                    }
                }
            }

            // Sort roots by tool, then by resource type alphabetically, then by name
            roots.sort_by(|a, b| {
                a.tool
                    .cmp(&b.tool)
                    .then_with(|| a.resource_type.to_string().cmp(&b.resource_type.to_string()))
                    .then_with(|| a.name.cmp(&b.name))
            });
        }

        // Filter to only duplicates if requested
        if cmd.duplicates {
            let duplicate_ids = self.find_duplicates(&roots, &nodes);
            roots.retain(|n| duplicate_ids.contains(&self.node_id(n)));
        }

        Ok(DependencyTree {
            project_name: self.project_name.clone(),
            nodes,
            roots,
        })
    }

    fn find_package(&self, name: &str) -> Result<&LockedResource> {
        for resource_type in ResourceType::all() {
            for resource in self.lockfile.get_resources(resource_type) {
                if resource.name == name {
                    return Ok(resource);
                }
            }
        }

        Err(anyhow::anyhow!("Package '{name}' not found in lockfile"))
    }

    fn build_node(&self, resource: &LockedResource, _cmd: &TreeCommand) -> Result<TreeNode> {
        // Extract display name from unique lockfile name
        // Unique name format: "source:name@version" or "name@version"
        // We want just "name" for display
        let display_name = Self::extract_display_name(&resource.name);

        // Convert dependencies to use the node ID format
        // Pass parent resource's source to correctly resolve dependencies from the same source
        let dependency_node_ids: Vec<String> = resource
            .dependencies
            .iter()
            .filter_map(|dep_id| {
                // Find the resource for this dependency (prefer same source as parent)
                if let Some(dep_resource) =
                    self.find_resource_by_id(dep_id, resource.source.as_deref())
                {
                    // Build a temporary node to get its ID in the same format used by tree.nodes
                    let dep_node = TreeNode {
                        name: Self::extract_display_name(&dep_resource.name),
                        resource_type: dep_resource.resource_type,
                        version: dep_resource.version.clone(),
                        source: dep_resource.source.clone(),
                        tool: dep_resource.tool.clone(),
                        dependencies: vec![], // Don't need dependencies for ID generation
                        has_patches: !dep_resource.applied_patches.is_empty(),
                        installed_at: dep_resource.installed_at.clone(),
                        applied_patches: dep_resource.applied_patches.clone(),
                    };
                    Some(self.node_id(&dep_node))
                } else {
                    None
                }
            })
            .collect();

        Ok(TreeNode {
            name: display_name,
            resource_type: resource.resource_type,
            version: resource.version.clone(),
            source: resource.source.clone(),
            tool: resource.tool.clone(),
            dependencies: dependency_node_ids,
            has_patches: !resource.applied_patches.is_empty(),
            installed_at: resource.installed_at.clone(),
            applied_patches: resource.applied_patches.clone(),
        })
    }

    /// Extracts the display name from a unique lockfile identifier.
    ///
    /// Converts from:
    /// - "source:name@version" → "name" (e.g., "community:api-designer@main" → "api-designer")
    /// - "source:name" → "name" (e.g., "local-deps:rust-haiku" → "rust-haiku")
    /// - "name@version" → "name"
    /// - "name" → "name"
    pub fn extract_display_name(unique_name: &str) -> String {
        // Remove source prefix if present (e.g., "local-deps:name" → "name")
        let after_source = if let Some((_source, rest)) = unique_name.split_once(':') {
            rest
        } else {
            unique_name
        };

        // Remove version suffix if present (e.g., "name@version" → "name")
        if let Some((name, _version)) = after_source.split_once('@') {
            name.to_string()
        } else {
            after_source.to_string()
        }
    }

    fn build_dependencies(
        &self,
        node: &TreeNode,
        nodes: &mut HashMap<String, TreeNode>,
        cmd: &TreeCommand,
    ) -> Result<()> {
        // Dependencies are already in tree node ID format (type/name)
        for dep_node_id in &node.dependencies {
            if nodes.contains_key(dep_node_id) {
                continue; // Already processed
            }

            // Find the dependency in lockfile (prefer same source as parent)
            if let Some(dep_resource) =
                self.find_resource_by_id(dep_node_id, node.source.as_deref())
            {
                let dep_node = self.build_node(dep_resource, cmd)?;
                let actual_dep_node_id = self.node_id(&dep_node);

                nodes.insert(actual_dep_node_id.clone(), dep_node.clone());
                self.build_dependencies(&dep_node, nodes, cmd)?;
            }
        }

        Ok(())
    }

    fn find_resource_by_id(
        &self,
        id: &str,
        preferred_source: Option<&str>,
    ) -> Option<&LockedResource> {
        // Dependencies use LockfileDependencyRef format: "type:name" or "source/type:name@version"
        // Parse using centralized LockfileDependencyRef logic
        use std::str::FromStr;
        let dep_ref =
            crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef::from_str(id).ok()?;
        let resource_type = dep_ref.resource_type;
        let name = &dep_ref.path;

        // Find the resource by matching the display name extracted from the unique name
        // Prefer resources from the same source as the parent (transitive deps should be same-source)
        let resources = self.lockfile.get_resources(&resource_type);

        // First try to find a match with the preferred source
        if let Some(source) = preferred_source {
            for resource in resources {
                let display_name = Self::extract_display_name(&resource.name);
                if display_name == *name && resource.source.as_deref() == Some(source) {
                    return Some(resource);
                }
            }
        }

        // Fall back to any match if no preferred source match found
        for resource in resources {
            let display_name = Self::extract_display_name(&resource.name);
            if display_name == *name {
                return Some(resource);
            }
        }

        None
    }

    fn node_id(&self, node: &TreeNode) -> String {
        // Generate unique ID matching the lockfile name format:
        // - "source:name@version" for remote sources (e.g., "community:api-designer@main")
        // - "source:name" for local sources (e.g., "local-deps:rust-haiku")
        // - "name" for resources without a source (e.g., "rust-haiku")

        match (&node.source, &node.version) {
            (Some(source), Some(version)) if version != "local" => {
                // Remote source with version: source:name@version
                format!("{}:{}@{}", source, node.name, version)
            }
            (Some(source), _) => {
                // Local source (version is "local" or None): source:name
                format!("{}:{}", source, node.name)
            }
            (None, Some(version)) if version != "local" => {
                // No source but has version: name@version
                format!("{}@{}", node.name, version)
            }
            (None, _) => {
                // No source and no version (or version is "local"): name
                node.name.clone()
            }
        }
    }

    fn find_duplicates(
        &self,
        roots: &[TreeNode],
        nodes: &HashMap<String, TreeNode>,
    ) -> HashSet<String> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut seen = HashSet::new();

        for root in roots {
            self.count_occurrences(root, &mut counts, &mut seen, nodes);
        }

        counts.iter().filter(|&(_, &count)| count > 1).map(|(id, _)| id.clone()).collect()
    }

    fn count_occurrences(
        &self,
        node: &TreeNode,
        counts: &mut HashMap<String, usize>,
        seen: &mut HashSet<String>,
        nodes: &HashMap<String, TreeNode>,
    ) {
        let node_id = self.node_id(node);
        *counts.entry(node_id.clone()).or_insert(0) += 1;

        if seen.contains(&node_id) {
            return; // Prevent infinite loops
        }
        seen.insert(node_id);

        for dep_id in &node.dependencies {
            if let Some(child) = nodes.get(dep_id) {
                self.count_occurrences(child, counts, seen, nodes);
            }
        }
    }
}

/// Format a toml::Value for display in patch output.
///
/// Produces clean, readable output:
/// - Strings: wrapped in quotes `"value"`
/// - Numbers/Booleans: plain text
/// - Arrays/Tables: formatted as TOML syntax
fn format_patch_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(format_patch_value).collect();
            format!("[{}]", elements.join(", "))
        }
        toml::Value::Table(_) | toml::Value::Datetime(_) => {
            // For complex types, use to_string() as fallback
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_default_command() -> TreeCommand {
        TreeCommand {
            depth: None,
            format: "tree".to_string(),
            duplicates: false,
            no_dedupe: false,
            package: None,
            agents: false,
            snippets: false,
            commands: false,
            scripts: false,
            hooks: false,
            mcp_servers: false,
            skills: false,
            invert: false,
            detailed: false,
        }
    }

    #[test]
    fn test_validate_arguments_valid_format() {
        let valid_formats = ["tree", "json", "text"];

        for format in valid_formats {
            let cmd = TreeCommand {
                format: format.to_string(),
                ..create_default_command()
            };
            assert!(cmd.validate_arguments().is_ok());
        }
    }

    #[test]
    fn test_validate_arguments_invalid_format() {
        let cmd = TreeCommand {
            format: "invalid".to_string(),
            ..create_default_command()
        };

        let result = cmd.validate_arguments();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid format"));
    }

    #[test]
    fn test_validate_arguments_zero_depth() {
        let cmd = TreeCommand {
            depth: Some(0),
            ..create_default_command()
        };

        let result = cmd.validate_arguments();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be at least 1"));
    }

    #[test]
    fn test_should_show_resource_type_no_filters() {
        let cmd = create_default_command();

        // With no filters, all types should be shown
        assert!(cmd.should_show_resource_type(ResourceType::Agent));
        assert!(cmd.should_show_resource_type(ResourceType::Snippet));
        assert!(cmd.should_show_resource_type(ResourceType::Command));
    }

    #[test]
    fn test_should_show_resource_type_with_filters() {
        let cmd = TreeCommand {
            agents: true,
            ..create_default_command()
        };

        assert!(cmd.should_show_resource_type(ResourceType::Agent));
        assert!(!cmd.should_show_resource_type(ResourceType::Snippet));
        assert!(!cmd.should_show_resource_type(ResourceType::Command));
    }

    #[test]
    fn test_node_id() {
        let lockfile = LockFile::new();
        let builder = TreeBuilder::new(&lockfile, "test-project".to_string());

        // Test with source and version
        let node = TreeNode {
            name: "test-agent".to_string(),
            resource_type: ResourceType::Agent,
            version: Some("v1.0.0".to_string()),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            dependencies: vec![],
            has_patches: false,
            installed_at: ".claude/agents/test-agent.md".to_string(),
            applied_patches: BTreeMap::new(),
        };
        assert_eq!(builder.node_id(&node), "community:test-agent@v1.0.0");

        // Test with source and local version (should omit @local)
        let node_local_source = TreeNode {
            name: "local-agent".to_string(),
            resource_type: ResourceType::Agent,
            version: Some("local".to_string()),
            source: Some("local-deps".to_string()),
            tool: Some("claude-code".to_string()),
            dependencies: vec![],
            has_patches: false,
            installed_at: ".claude/agents/local-agent.md".to_string(),
            applied_patches: BTreeMap::new(),
        };
        assert_eq!(builder.node_id(&node_local_source), "local-deps:local-agent");

        // Test without source but with local version (should omit @local)
        let node_local = TreeNode {
            name: "local-agent".to_string(),
            resource_type: ResourceType::Agent,
            version: Some("local".to_string()),
            source: None,
            tool: Some("claude-code".to_string()),
            dependencies: vec![],
            has_patches: false,
            installed_at: ".claude/agents/local-agent.md".to_string(),
            applied_patches: BTreeMap::new(),
        };
        assert_eq!(builder.node_id(&node_local), "local-agent");

        // Test without version but with source
        let node_no_version = TreeNode {
            name: "test-agent".to_string(),
            resource_type: ResourceType::Agent,
            version: None,
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            dependencies: vec![],
            has_patches: false,
            installed_at: ".claude/agents/test-agent.md".to_string(),
            applied_patches: BTreeMap::new(),
        };
        assert_eq!(builder.node_id(&node_no_version), "community:test-agent");
    }
}
