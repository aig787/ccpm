//! List installed Claude Code resources from the lockfile.
//!
//! This module provides the `list` command which displays information about
//! currently installed dependencies as recorded in the lockfile (`agpm.lock`).
//! The command offers various output formats and filtering options to help
//! users understand their project's dependencies.
//!
//! # Features
//!
//! - **Multiple Output Formats**: JSON, table, or tree view
//! - **Filtering Options**: Show only agents, snippets, or specific dependencies
//! - **Detailed Information**: Source URLs, versions, installation paths, checksums
//! - **Dependency Analysis**: Shows unused dependencies and source statistics
//! - **Path Information**: Displays where resources are installed
//!
//! # Examples
//!
//! List all installed resources:
//! ```bash
//! agpm list
//! ```
//!
//! List only agents:
//! ```bash
//! agpm list --agents
//! ```
//!
//! List with detailed information:
//! ```bash
//! agpm list --details
//! ```
//!
//! Output in JSON format:
//! ```bash
//! agpm list --format json
//! ```
//!
//! Show dependency tree:
//! ```bash
//! agpm list --format tree
//! ```
//!
//! List specific dependencies:
//! ```bash
//! agpm list my-agent utils-snippet
//! ```
//!
//! # Output Formats
//!
//! ## Table Format (Default)
//! ```text
//! NAME          TYPE     SOURCE      VERSION   PATH
//! code-reviewer agent    official    v1.0.0    agents/code-reviewer.md
//! utils         snippet  community   v2.1.0    snippets/utils.md
//! ```
//!
//! ## JSON Format
//! ```json
//! {
//!   "agents": [...],
//!   "snippets": [...],
//!   "sources": [...]
//! }
//! ```
//!
//! ## Tree Format
//! ```text
//! Sources:
//! ├── official (https://github.com/org/official.git)
//! │   └── agents/code-reviewer.md@v1.0.0
//! └── community (https://github.com/org/community.git)
//!     └── snippets/utils.md@v2.1.0
//! ```
//!
//! # Data Sources
//!
//! The command primarily reads from:
//! - **Primary**: `agpm.lock` - Contains installed resource information
//! - **Secondary**: `agpm.toml` - Used for manifest comparison and validation
//!
//! # Error Conditions
//!
//! - No lockfile found (no dependencies installed)
//! - Lockfile is corrupted or has invalid format
//! - Requested dependency names not found in lockfile
//! - File system access issues

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::cache::Cache;
use crate::lockfile::LockFile;
use crate::lockfile::patch_display::extract_patch_displays;
use crate::manifest::{Manifest, find_manifest_with_optional};

/// Internal representation for list items used in various output formats.
///
/// This struct normalizes resource information from both agents and snippets
/// in the lockfile to provide a consistent view for display purposes.
#[derive(Debug, Clone)]
struct ListItem {
    /// The name/key of the resource as defined in the manifest
    name: String,
    /// The source repository name (if from a Git source)
    source: Option<String>,
    /// The version/tag/branch of the resource
    version: Option<String>,
    /// The path within the source repository
    path: Option<String>,
    /// The type of resource ("agent" or "snippet")
    resource_type: String,
    /// The local installation path where the resource file is located
    installed_at: Option<String>,
    /// The SHA-256 checksum of the installed resource file
    checksum: Option<String>,
    /// The resolved Git commit hash
    resolved_commit: Option<String>,
    /// The tool ("claude-code", "opencode", "agpm", or custom)
    tool: Option<String>,
    /// Patches that were applied to this resource
    applied_patches: std::collections::BTreeMap<String, toml::Value>,
}

/// Command to list installed Claude Code resources.
///
/// This command displays information about dependencies currently installed
/// in the project based on the lockfile. It supports various output formats,
/// filtering options, and detail levels to help users understand their
/// project's resource dependencies.
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::list::ListCommand;
///
/// // List all resources in default table format
/// let cmd = ListCommand {
///     agents: false,
///     snippets: false,
///     format: "table".to_string(),
///     manifest: false,
///     r#type: None,
///     source: None,
///     search: None,
///     detailed: false,
///     files: false,
///     verbose: false,
///     sort: None,
/// };
///
/// // List only agents with detailed information
/// let cmd = ListCommand {
///     agents: true,
///     snippets: false,
///     format: "table".to_string(),
///     manifest: false,
///     r#type: None,
///     source: None,
///     search: None,
///     detailed: true,
///     files: true,
///     verbose: false,
///     sort: Some("name".to_string()),
/// };
/// ```
#[derive(Args)]
pub struct ListCommand {
    /// Show only agents
    ///
    /// When specified, filters the output to show only agent resources,
    /// excluding snippets. Mutually exclusive with `--snippets`.
    #[arg(long)]
    agents: bool,

    /// Show only snippets
    ///
    /// When specified, filters the output to show only snippet resources,
    /// excluding agents and commands. Mutually exclusive with `--agents` and `--commands`.
    #[arg(long)]
    snippets: bool,

    /// Show only commands
    ///
    /// When specified, filters the output to show only command resources,
    /// excluding agents and snippets. Mutually exclusive with `--agents` and `--snippets`.
    #[arg(long)]
    commands: bool,

    /// Show only skills
    ///
    /// When specified, filters the output to show only skill resources,
    /// excluding other resource types.
    #[arg(long)]
    skills: bool,

    /// Output format (table, json, yaml, compact, simple)
    ///
    /// Controls how the resource information is displayed:
    /// - `table`: Formatted table with columns (default)
    /// - `json`: JSON object with structured data
    /// - `yaml`: YAML format for structured data
    /// - `compact`: Minimal single-line format
    /// - `simple`: Plain text list format
    #[arg(short = 'f', long, default_value = "table")]
    format: String,

    /// Show from manifest instead of lockfile
    ///
    /// When enabled, shows dependencies defined in the manifest (`agpm.toml`)
    /// rather than installed dependencies from the lockfile (`agpm.lock`).
    /// This is useful for comparing intended vs. actual installations.
    #[arg(long)]
    manifest: bool,

    /// Filter by resource type
    ///
    /// Filters resources by their type (agent, snippet). This is an
    /// alternative to using the `--agents` or `--snippets` flags.
    #[arg(long, value_name = "TYPE")]
    r#type: Option<String>,

    /// Filter by source name
    ///
    /// Shows only resources from the specified source repository.
    /// The source name should match one defined in the manifest.
    #[arg(long, value_name = "SOURCE")]
    source: Option<String>,

    /// Search by name pattern
    ///
    /// Filters resources whose names match the given pattern.
    /// Supports substring matching (case-insensitive).
    #[arg(long, value_name = "PATTERN")]
    search: Option<String>,

    /// Show detailed information
    ///
    /// Includes additional columns in the output such as checksums,
    /// resolved commits, and full source URLs. This provides more
    /// comprehensive information about each resource.
    #[arg(long)]
    detailed: bool,

    /// Show installed file paths
    ///
    /// Includes the local file system paths where resources are installed.
    /// Useful for understanding the project layout and locating resource files.
    #[arg(long)]
    files: bool,

    /// Verbose output (show all sections)
    ///
    /// Enables verbose mode which shows additional information including
    /// source statistics, dependency summaries, and extended metadata.
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Sort by field (name, version, source, type)
    ///
    /// Controls the sorting order of the resource list. Supported fields:
    /// - `name`: Sort alphabetically by resource name
    /// - `version`: Sort by version (semantic versioning aware)
    /// - `source`: Sort by source repository name
    /// - `type`: Sort by resource type (agents first, then snippets)
    #[arg(long, value_name = "FIELD")]
    sort: Option<String>,
}

impl ListCommand {
    /// Execute the list command to display installed resources.
    ///
    /// This method orchestrates the process of loading resource data, applying
    /// filters, and formatting the output according to the specified options.
    ///
    /// # Behavior
    ///
    /// 1. **Data Loading**: Loads resource data from lockfile or manifest
    /// 2. **Filtering**: Applies type, source, and search filters
    /// 3. **Sorting**: Orders results according to the specified sort field
    /// 4. **Formatting**: Outputs data in the requested format
    ///
    /// # Data Sources
    ///
    /// - **Default**: Uses lockfile (`agpm.lock`) to show installed resources
    /// - **Manifest Mode**: Uses manifest (`agpm.toml`) to show defined dependencies
    ///
    /// # Filtering Logic
    ///
    /// Filters are applied in this order:
    /// 1. Type filter (agents/snippets)
    /// 2. Source filter (specific repository)
    /// 3. Search pattern (name matching)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the list was displayed successfully
    /// - `Err(anyhow::Error)` if:
    ///   - No lockfile found (and not using manifest mode)
    ///   - Lockfile format is invalid
    /// - Unable to load manifest (in manifest mode)
    ///   - Output formatting fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::list::ListCommand;
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = ListCommand {
    ///     agents: false,
    ///     snippets: false,
    ///     format: "json".to_string(),
    ///     manifest: false,
    ///     r#type: None,
    ///     source: None,
    ///     search: None,
    ///     detailed: true,
    ///     files: false,
    ///     verbose: false,
    ///     sort: Some("name".to_string()),
    /// };
    /// // cmd.execute_with_manifest_path(None).await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # }));
    /// ```
    /// Execute the list command with an optional manifest path
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // Find manifest file
        let manifest_path = find_manifest_with_optional(manifest_path)
            .context("No agpm.toml found. Please create one to define your dependencies.")?;

        self.execute_from_path(manifest_path).await
    }

    pub async fn execute_from_path(self, manifest_path: PathBuf) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // For consistency with execute(), require the manifest to exist
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!("Manifest file {} not found", manifest_path.display()));
        }

        let project_dir = manifest_path.parent().unwrap();

        if self.manifest {
            // List from manifest
            self.list_from_manifest(&manifest_path)?;
        } else {
            // List from lockfile
            self.list_from_lockfile(project_dir).await?;
        }

        Ok(())
    }

    fn validate_arguments(&self) -> Result<()> {
        // Validate format
        match self.format.as_str() {
            "table" | "json" | "yaml" | "compact" | "simple" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid format '{}'. Valid formats are: table, json, yaml, compact, simple",
                    self.format
                ));
            }
        }

        // Validate type filter
        if let Some(ref t) = self.r#type {
            match t.as_str() {
                "agents" | "snippets" => {}
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid type '{t}'. Valid types are: agents, snippets"
                    ));
                }
            }
        }

        // Validate sort field
        if let Some(ref field) = self.sort {
            match field.as_str() {
                "name" | "version" | "source" | "type" => {}
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid sort field '{field}'. Valid fields are: name, version, source, type"
                    ));
                }
            }
        }

        Ok(())
    }

    fn list_from_manifest(&self, manifest_path: &std::path::Path) -> Result<()> {
        let manifest = Manifest::load(manifest_path)?;

        // Collect and filter dependencies
        let mut items = Vec::new();

        // Iterate through all resource types using the central definition
        for resource_type in crate::core::ResourceType::all() {
            // Check if we should show this resource type
            if !self.should_show_resource_type(*resource_type) {
                continue;
            }

            let type_str = resource_type.to_string();

            // Note: MCP servers are handled separately as they use a different dependency type
            if *resource_type == crate::core::ResourceType::McpServer {
                // Skip MCP servers in this generic iteration - they need special handling
                continue;
            }

            // Get dependencies for this resource type from the manifest
            if let Some(deps) = manifest.get_dependencies(*resource_type) {
                for (name, dep) in deps {
                    if self.matches_filters(name, Some(dep), &type_str) {
                        items.push(ListItem {
                            name: name.clone(),
                            source: dep.get_source().map(std::string::ToString::to_string),
                            version: dep.get_version().map(std::string::ToString::to_string),
                            path: Some(dep.get_path().to_string()),
                            resource_type: type_str.clone(),
                            installed_at: None,
                            checksum: None,
                            resolved_commit: None,
                            tool: Some(
                                dep.get_tool()
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| manifest.get_default_tool(*resource_type)),
                            ),
                            applied_patches: std::collections::BTreeMap::new(),
                        });
                    }
                }
            }
        }

        // Handle MCP servers (now using standard ResourceDependency)
        if self.should_show_resource_type(crate::core::ResourceType::McpServer) {
            for (name, mcp_dep) in &manifest.mcp_servers {
                // MCP servers now use standard ResourceDependency
                if self.matches_filters(name, Some(mcp_dep), "mcp-server") {
                    items.push(ListItem {
                        name: name.clone(),
                        source: mcp_dep.get_source().map(std::string::ToString::to_string),
                        version: mcp_dep.get_version().map(std::string::ToString::to_string),
                        path: Some(mcp_dep.get_path().to_string()),
                        resource_type: "mcp-server".to_string(),
                        installed_at: None,
                        checksum: None,
                        resolved_commit: None,
                        tool: Some(mcp_dep.get_tool().map(|s| s.to_string()).unwrap_or_else(
                            || manifest.get_default_tool(crate::core::ResourceType::McpServer),
                        )),
                        applied_patches: std::collections::BTreeMap::new(),
                    });
                }
            }
        }

        // Sort items
        self.sort_items(&mut items);

        // Output results
        self.output_items(&items, "Dependencies from agpm.toml:")?;

        Ok(())
    }

    async fn list_from_lockfile(&self, project_dir: &std::path::Path) -> Result<()> {
        let lockfile_path = project_dir.join("agpm.lock");

        if !lockfile_path.exists() {
            if self.format == "json" {
                println!("{{}}");
            } else {
                println!("No installed resources found.");
                println!("⚠️  agpm.lock not found. Run 'agpm install' first.");
            }
            return Ok(());
        }

        // Create a temporary manifest for CommandContext (we only need it for lockfile loading)
        let manifest_path = project_dir.join("agpm.toml");
        let manifest = crate::manifest::Manifest::load(&manifest_path)?;
        let command_context =
            crate::cli::common::CommandContext::new(manifest, project_dir.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let lockfile = match command_context.load_lockfile_with_regeneration(true, "list")? {
            Some(lockfile) => lockfile,
            None => {
                // Lockfile was regenerated and doesn't exist yet
                if self.format == "json" {
                    println!("{{}}");
                } else {
                    println!("No installed resources found.");
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

        // Collect and filter entries
        let mut items = Vec::new();

        // Iterate through all resource types using the central definition
        for resource_type in crate::core::ResourceType::all() {
            // Check if we should show this resource type
            if !self.should_show_resource_type(*resource_type) {
                continue;
            }

            let type_str = resource_type.to_string();

            // Get resources for this type from the lockfile
            for entry in lockfile.get_resources(resource_type) {
                if self.matches_lockfile_filters(&entry.name, entry, &type_str) {
                    items.push(self.lockentry_to_listitem(entry, &type_str));
                }
            }
        }

        // Sort items
        self.sort_items(&mut items);

        // Handle special flags

        // Output results
        if self.detailed {
            self.output_items_detailed(
                &items,
                "Installed resources from agpm.lock:",
                &lockfile,
                cache.as_ref(),
            )
            .await?;
        } else {
            self.output_items(&items, "Installed resources from agpm.lock:")?;
        }

        Ok(())
    }

    /// Determine if a resource type should be shown based on filters
    fn should_show_resource_type(&self, resource_type: crate::core::ResourceType) -> bool {
        use crate::core::ResourceType;

        // Check if there's a type filter
        if let Some(ref t) = self.r#type {
            let type_str = resource_type.to_string();
            return t == &type_str || t == &format!("{type_str}s");
        }

        // Check individual flags
        match resource_type {
            ResourceType::Agent => !self.snippets && !self.commands && !self.skills,
            ResourceType::Snippet => !self.agents && !self.commands && !self.skills,
            ResourceType::Command => !self.agents && !self.snippets && !self.skills,
            ResourceType::Script => {
                !self.agents && !self.snippets && !self.commands && !self.skills
            }
            ResourceType::Hook => !self.agents && !self.snippets && !self.commands && !self.skills,
            ResourceType::McpServer => {
                !self.agents && !self.commands && !self.snippets && !self.skills
            }
            ResourceType::Skill => !self.agents && !self.snippets && !self.commands,
        }
    }

    /// Check if an item matches all filters
    fn matches_filters(
        &self,
        name: &str,
        dep: Option<&crate::manifest::ResourceDependency>,
        _resource_type: &str,
    ) -> bool {
        // Source filter
        if let Some(ref source_filter) = self.source
            && let Some(dep) = dep
        {
            if let Some(source) = dep.get_source() {
                if source != source_filter {
                    return false;
                }
            } else {
                return false; // No source but filter specified
            }
        }

        // Search filter
        if let Some(ref search) = self.search
            && !name.contains(search)
        {
            return false;
        }

        true
    }

    /// Sort items based on sort criteria
    fn sort_items(&self, items: &mut [ListItem]) {
        if let Some(ref sort_field) = self.sort {
            match sort_field.as_str() {
                "name" => items.sort_by(|a, b| a.name.cmp(&b.name)),
                "version" => items.sort_by(|a, b| {
                    a.version.as_deref().unwrap_or("").cmp(b.version.as_deref().unwrap_or(""))
                }),
                "source" => items.sort_by(|a, b| {
                    a.source
                        .as_deref()
                        .unwrap_or("local")
                        .cmp(b.source.as_deref().unwrap_or("local"))
                }),
                "type" => items.sort_by(|a, b| a.resource_type.cmp(&b.resource_type)),
                _ => {} // Already validated
            }
        }
    }

    /// Output items in the specified format
    fn output_items(&self, items: &[ListItem], title: &str) -> Result<()> {
        if items.is_empty() {
            if self.format == "json" {
                println!("{{}}");
            } else {
                println!("No installed resources found.");
            }
            return Ok(());
        }

        match self.format.as_str() {
            "json" => self.output_json(items)?,
            "yaml" => self.output_yaml(items)?,
            "compact" => self.output_compact(items),
            "simple" => self.output_simple(items),
            _ => self.output_table(items, title),
        }

        Ok(())
    }

    /// Output in JSON format
    fn output_json(&self, items: &[ListItem]) -> Result<()> {
        let json_items: Vec<serde_json::Value> = items
            .iter()
            .map(|item| {
                let mut obj = serde_json::json!({
                    "name": item.name,
                    "type": item.resource_type,
                    "tool": item.tool
                });

                if let Some(ref source) = item.source {
                    obj["source"] = serde_json::Value::String(source.clone());
                }
                if let Some(ref version) = item.version {
                    obj["version"] = serde_json::Value::String(version.clone());
                }
                if let Some(ref path) = item.path {
                    obj["path"] = serde_json::Value::String(path.clone());
                }
                if let Some(ref installed_at) = item.installed_at {
                    obj["installed_at"] = serde_json::Value::String(installed_at.clone());
                }
                if let Some(ref checksum) = item.checksum {
                    obj["checksum"] = serde_json::Value::String(checksum.clone());
                }

                obj
            })
            .collect();

        println!("{}", serde_json::to_string_pretty(&json_items)?);
        Ok(())
    }

    /// Output in YAML format
    fn output_yaml(&self, items: &[ListItem]) -> Result<()> {
        let yaml_items: Vec<HashMap<String, serde_yaml::Value>> = items
            .iter()
            .map(|item| {
                let mut obj = HashMap::new();
                obj.insert("name".to_string(), serde_yaml::Value::String(item.name.clone()));
                obj.insert(
                    "type".to_string(),
                    serde_yaml::Value::String(item.resource_type.clone()),
                );
                obj.insert(
                    "tool".to_string(),
                    serde_yaml::Value::String(
                        item.tool.clone().expect("Tool should always be set"),
                    ),
                );

                if let Some(ref source) = item.source {
                    obj.insert("source".to_string(), serde_yaml::Value::String(source.clone()));
                }
                if let Some(ref version) = item.version {
                    obj.insert("version".to_string(), serde_yaml::Value::String(version.clone()));
                }
                if let Some(ref path) = item.path {
                    obj.insert("path".to_string(), serde_yaml::Value::String(path.clone()));
                }
                if let Some(ref installed_at) = item.installed_at {
                    obj.insert(
                        "installed_at".to_string(),
                        serde_yaml::Value::String(installed_at.clone()),
                    );
                }

                obj
            })
            .collect();

        println!("{}", serde_yaml::to_string(&yaml_items)?);
        Ok(())
    }

    /// Output in compact format
    fn output_compact(&self, items: &[ListItem]) {
        for item in items {
            let source = item.source.as_deref().unwrap_or("local");
            let version = item.version.as_deref().unwrap_or("latest");
            println!("{} {} {}", item.name, version, source);
        }
    }

    /// Output in simple format
    fn output_simple(&self, items: &[ListItem]) {
        for item in items {
            println!("{} ({}))", item.name, item.resource_type);
        }
    }

    /// Output in table format
    fn output_table(&self, items: &[ListItem], title: &str) {
        println!("{}", title.bold());
        println!();

        // Show headers for table format (but not verbose mode)
        if !items.is_empty() && self.format == "table" && !self.verbose {
            println!(
                "{:<32} {:<15} {:<15} {:<12} {:<15}",
                "Name".cyan().bold(),
                "Version".cyan().bold(),
                "Source".cyan().bold(),
                "Type".cyan().bold(),
                "Artifact".cyan().bold()
            );
            println!("{}", "-".repeat(92).bright_black());
        }

        if self.format == "table" && !self.files && !self.detailed && !self.verbose {
            // Print items directly in table format
            for item in items {
                self.print_item(item);
            }
        } else {
            // Simple listing
            let show_agents = self.should_show_resource_type(crate::core::ResourceType::Agent);
            let show_snippets = self.should_show_resource_type(crate::core::ResourceType::Snippet);

            if show_agents {
                let agents: Vec<_> = items.iter().filter(|i| i.resource_type == "agent").collect();
                if !agents.is_empty() {
                    println!("{}:", "Agents".cyan().bold());
                    for item in agents {
                        self.print_item(item);
                    }
                    println!();
                }
            }

            if show_snippets {
                let snippets: Vec<_> =
                    items.iter().filter(|i| i.resource_type == "snippet").collect();
                if !snippets.is_empty() {
                    println!("{}:", "Snippets".cyan().bold());
                    for item in snippets {
                        self.print_item(item);
                    }
                }
            }
        }

        println!("{}: {} resources", "Total".green().bold(), items.len());
    }

    /// Output items in detailed mode with patch comparisons
    async fn output_items_detailed(
        &self,
        items: &[ListItem],
        title: &str,
        lockfile: &LockFile,
        cache: Option<&Cache>,
    ) -> Result<()> {
        if items.is_empty() {
            if self.format == "json" {
                println!("{{}}");
            } else {
                println!("No installed resources found.");
            }
            return Ok(());
        }

        println!("{}", title.bold());
        println!();

        // Group by resource type
        let show_agents = self.should_show_resource_type(crate::core::ResourceType::Agent);
        let show_snippets = self.should_show_resource_type(crate::core::ResourceType::Snippet);

        if show_agents {
            let agents: Vec<_> = items.iter().filter(|i| i.resource_type == "agent").collect();
            if !agents.is_empty() {
                println!("{}:", "Agents".cyan().bold());
                for item in agents {
                    self.print_item_detailed(item, lockfile, cache).await;
                }
                println!();
            }
        }

        if show_snippets {
            let snippets: Vec<_> = items.iter().filter(|i| i.resource_type == "snippet").collect();
            if !snippets.is_empty() {
                println!("{}:", "Snippets".cyan().bold());
                for item in snippets {
                    self.print_item_detailed(item, lockfile, cache).await;
                }
            }
        }

        println!("{}: {} resources", "Total".green().bold(), items.len());

        Ok(())
    }

    /// Print a single item in detailed mode with patch comparison
    async fn print_item_detailed(
        &self,
        item: &ListItem,
        lockfile: &LockFile,
        cache: Option<&Cache>,
    ) {
        let source = item.source.as_deref().unwrap_or("local");
        let version = item.version.as_deref().unwrap_or("latest");

        println!("    {}", item.name.bright_white());
        println!("      Source: {}", source.bright_black());
        println!("      Version: {}", version.yellow());
        if let Some(ref path) = item.path {
            println!("      Path: {}", path.bright_black());
        }
        if let Some(ref installed_at) = item.installed_at {
            println!("      Installed at: {}", installed_at.bright_black());
        }
        if let Some(ref checksum) = item.checksum {
            println!("      Checksum: {}", checksum.bright_black());
        }

        // Show patches with original → overridden comparison
        if !item.applied_patches.is_empty() {
            println!("      Applied patches:");

            // If we have cache, try to get original values
            if let Some(cache) = cache {
                // Find the locked resource for this item
                if let Some(locked_resource) = self.find_locked_resource(item, lockfile) {
                    let patch_displays = extract_patch_displays(locked_resource, cache).await;
                    for display in patch_displays {
                        let formatted = display.format();
                        // Indent each line of the multi-line diff output
                        for (i, line) in formatted.lines().enumerate() {
                            if i == 0 {
                                // First line: bullet point
                                println!("        • {}", line);
                            } else {
                                // Subsequent lines: indent to align with content
                                println!("          {}", line);
                            }
                        }
                    }
                } else {
                    // Fallback: just show overridden values
                    self.print_patches_fallback(&item.applied_patches);
                }
            } else {
                // No cache: just show overridden values
                self.print_patches_fallback(&item.applied_patches);
            }
        }
        println!();
    }

    /// Fallback patch display without original values
    fn print_patches_fallback(&self, patches: &BTreeMap<String, toml::Value>) {
        let mut patch_keys: Vec<_> = patches.keys().collect();
        patch_keys.sort();
        for key in patch_keys {
            let value = &patches[key];
            let formatted_value = format_patch_value(value);
            println!("        • {}: {}", key.blue(), formatted_value);
        }
    }

    /// Find the locked resource corresponding to a list item
    fn find_locked_resource<'a>(
        &self,
        item: &ListItem,
        lockfile: &'a LockFile,
    ) -> Option<&'a crate::lockfile::LockedResource> {
        // Determine resource type
        let resource_type = match item.resource_type.as_str() {
            "agent" => crate::core::ResourceType::Agent,
            "snippet" => crate::core::ResourceType::Snippet,
            "command" => crate::core::ResourceType::Command,
            "script" => crate::core::ResourceType::Script,
            "hook" => crate::core::ResourceType::Hook,
            "mcp-server" => crate::core::ResourceType::McpServer,
            _ => return None,
        };

        // Find matching resource in lockfile
        lockfile.get_resources(&resource_type).iter().find(|r| r.name == item.name)
    }

    /// Print a single item
    fn print_item(&self, item: &ListItem) {
        let source = item.source.as_deref().unwrap_or("local");
        let version = item.version.as_deref().unwrap_or("latest");

        if self.format == "table" && !self.files && !self.detailed {
            // Table format with columns
            // Build the name field with proper padding before adding colors
            let name_with_indicator = if !item.applied_patches.is_empty() {
                format!("{} (patched)", item.name)
            } else {
                item.name.clone()
            };

            // Apply padding to plain text, then colorize
            let name_field = format!("{:<32}", name_with_indicator);
            let colored_name = name_field.bright_white();

            println!(
                "{} {:<15} {:<15} {:<12} {:<15}",
                colored_name,
                version.yellow(),
                source.bright_black(),
                item.resource_type.bright_white(),
                item.tool.clone().expect("Tool should always be set").bright_black()
            );
        } else if self.files {
            if let Some(ref installed_at) = item.installed_at {
                println!("    {}", installed_at.bright_black());
            } else if let Some(ref path) = item.path {
                println!("    {}", path.bright_black());
            }
        } else if self.detailed {
            println!("    {}", item.name.bright_white());
            println!("      Source: {}", source.bright_black());
            println!("      Version: {}", version.yellow());
            if let Some(ref path) = item.path {
                println!("      Path: {}", path.bright_black());
            }
            if let Some(ref installed_at) = item.installed_at {
                println!("      Installed at: {}", installed_at.bright_black());
            }
            if let Some(ref checksum) = item.checksum {
                println!("      Checksum: {}", checksum.bright_black());
            }
            if !item.applied_patches.is_empty() {
                println!("      {}", "Patches:".cyan());
                let mut patch_keys: Vec<_> = item.applied_patches.keys().collect();
                patch_keys.sort(); // Sort for consistent display
                for key in patch_keys {
                    let value = &item.applied_patches[key];
                    let formatted_value = format_patch_value(value);
                    println!("        {}: {}", key.yellow(), formatted_value.green());
                }
            }
            println!();
        } else {
            let commit_info = if let Some(ref commit) = item.resolved_commit {
                format!("@{}", &commit[..7.min(commit.len())])
            } else {
                String::new()
            };

            println!(
                "    {} {} {} {}",
                item.name.bright_white(),
                format!("({source}))").bright_black(),
                version.yellow(),
                commit_info.bright_black()
            );

            if let Some(ref installed_at) = item.installed_at {
                println!("      → {}", installed_at.bright_black());
            }
        }
    }

    /// Check if a lockfile entry matches all filters
    fn matches_lockfile_filters(
        &self,
        name: &str,
        entry: &crate::lockfile::LockedResource,
        _resource_type: &str,
    ) -> bool {
        // Source filter
        if let Some(ref source_filter) = self.source {
            if let Some(ref source) = entry.source {
                if source != source_filter {
                    return false;
                }
            } else {
                return false; // No source but filter specified
            }
        }

        // Search filter
        if let Some(ref search) = self.search
            && !name.contains(search)
        {
            return false;
        }

        true
    }

    /// Convert a lockfile entry to a `ListItem`
    fn lockentry_to_listitem(
        &self,
        entry: &crate::lockfile::LockedResource,
        resource_type: &str,
    ) -> ListItem {
        ListItem {
            name: entry.name.clone(),
            source: entry.source.clone(),
            version: entry.version.clone(),
            path: Some(entry.path.clone()),
            resource_type: resource_type.to_string(),
            installed_at: Some(entry.installed_at.clone()),
            checksum: Some(entry.checksum.clone()),
            resolved_commit: entry.resolved_commit.clone(),
            tool: Some(entry.tool.clone().unwrap_or_else(|| "claude-code".to_string())),
            applied_patches: entry.applied_patches.clone(),
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
    use crate::lockfile::{LockedResource, LockedSource};
    use crate::manifest::{DetailedDependency, ResourceDependency};

    use tempfile::TempDir;

    fn create_default_command() -> ListCommand {
        ListCommand {
            agents: false,
            snippets: false,
            commands: false,
            skills: false,
            format: "table".to_string(),
            manifest: false,
            r#type: None,
            source: None,
            search: None,
            detailed: false,
            files: false,
            verbose: false,
            sort: None,
        }
    }

    fn create_test_manifest() -> crate::manifest::Manifest {
        let mut manifest = crate::manifest::Manifest::new();

        // Add sources
        manifest
            .sources
            .insert("official".to_string(), "https://github.com/example/official.git".to_string());
        manifest.sources.insert(
            "community".to_string(),
            "https://github.com/example/community.git".to_string(),
        );

        // Add agents
        manifest.agents.insert(
            "code-reviewer".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/reviewer.md".to_string(),
                version: Some("v1.0.0".to_string()),
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            })),
        );

        manifest.agents.insert(
            "local-helper".to_string(),
            ResourceDependency::Simple("../local/helper.md".to_string()),
        );

        // Add snippets
        manifest.snippets.insert(
            "utils".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("community".to_string()),
                path: "snippets/utils.md".to_string(),
                version: Some("v1.2.0".to_string()),
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            })),
        );

        manifest.snippets.insert(
            "local-snippet".to_string(),
            ResourceDependency::Simple("./snippets/local.md".to_string()),
        );

        manifest
    }

    fn create_test_lockfile() -> crate::lockfile::LockFile {
        let mut lockfile = crate::lockfile::LockFile::new();

        // Add sources
        lockfile.sources.push(LockedSource {
            name: "official".to_string(),
            url: "https://github.com/example/official.git".to_string(),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        });

        lockfile.sources.push(LockedSource {
            name: "community".to_string(),
            url: "https://github.com/example/community.git".to_string(),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Add agents
        lockfile.agents.push(LockedResource {
            name: "code-reviewer".to_string(),
            source: Some("official".to_string()),
            url: Some("https://github.com/example/official.git".to_string()),
            path: "agents/reviewer.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:abc123".to_string(),
            installed_at: "agents/code-reviewer.md".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });

        lockfile.agents.push(LockedResource {
            name: "local-helper".to_string(),
            source: None,
            url: None,
            path: "../local/helper.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:def456".to_string(),
            installed_at: "agents/local-helper.md".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });

        // Add snippets
        lockfile.snippets.push(LockedResource {
            name: "utils".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "snippets/utils.md".to_string(),
            version: Some("v1.2.0".to_string()),
            resolved_commit: Some("def456ghi789".to_string()),
            checksum: "sha256:ghi789".to_string(),
            installed_at: "snippets/utils.md".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Snippet,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });

        lockfile
    }

    #[tokio::test]
    async fn test_list_no_manifest() {
        let temp = TempDir::new().unwrap();
        // Don't create agpm.toml
        let manifest_path = temp.path().join("agpm.toml");

        let cmd = create_default_command();

        // This should fail because there's no manifest
        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_empty_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create empty manifest
        let manifest = crate::manifest::Manifest::new();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_from_manifest_with_resources() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_from_lockfile_no_lockfile() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest but no lockfile
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = create_default_command(); // manifest = false by default

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_from_lockfile_with_resources() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");

        // Create both manifest and lockfile
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let lockfile = create_test_lockfile();
        lockfile.save(&lockfile_path).unwrap();

        let cmd = create_default_command(); // manifest = false by default

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_arguments_valid_format() {
        let valid_formats = ["table", "json", "yaml", "compact", "simple"];

        for format in valid_formats {
            let cmd = ListCommand {
                format: format.to_string(),
                ..create_default_command()
            };
            assert!(cmd.validate_arguments().is_ok());
        }
    }

    #[test]
    fn test_validate_arguments_invalid_format() {
        let cmd = ListCommand {
            format: "invalid".to_string(),
            ..create_default_command()
        };

        let result = cmd.validate_arguments();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid format"));
    }

    #[test]
    fn test_validate_arguments_valid_type() {
        let valid_types = ["agents", "snippets"];

        for type_name in valid_types {
            let cmd = ListCommand {
                r#type: Some(type_name.to_string()),
                ..create_default_command()
            };
            assert!(cmd.validate_arguments().is_ok());
        }
    }

    #[test]
    fn test_validate_arguments_invalid_type() {
        let cmd = ListCommand {
            r#type: Some("invalid".to_string()),
            ..create_default_command()
        };

        let result = cmd.validate_arguments();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid type"));
    }

    #[test]
    fn test_validate_arguments_valid_sort() {
        let valid_sorts = ["name", "version", "source", "type"];

        for sort in valid_sorts {
            let cmd = ListCommand {
                sort: Some(sort.to_string()),
                ..create_default_command()
            };
            assert!(cmd.validate_arguments().is_ok());
        }
    }

    #[test]
    fn test_validate_arguments_invalid_sort() {
        let cmd = ListCommand {
            sort: Some("invalid".to_string()),
            ..create_default_command()
        };

        let result = cmd.validate_arguments();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid sort field"));
    }

    #[test]
    fn test_should_show_agents() {
        // Show agents when no specific type filter
        let cmd = create_default_command();
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

        // Show only agents when agents flag is set
        let cmd = ListCommand {
            agents: true,
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

        // Don't show agents when snippets flag is set
        let cmd = ListCommand {
            snippets: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));

        // Show agents when type is "agents"
        let cmd = ListCommand {
            r#type: Some("agents".to_string()),
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

        // Don't show agents when type is "snippets"
        let cmd = ListCommand {
            r#type: Some("snippets".to_string()),
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    }

    #[test]
    fn test_should_show_snippets() {
        // Show snippets when no specific type filter
        let cmd = create_default_command();
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

        // Don't show snippets when agents flag is set
        let cmd = ListCommand {
            agents: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

        // Show only snippets when snippets flag is set
        let cmd = ListCommand {
            snippets: true,
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

        // Don't show snippets when type is "agents"
        let cmd = ListCommand {
            r#type: Some("agents".to_string()),
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

        // Show snippets when type is "snippets"
        let cmd = ListCommand {
            r#type: Some("snippets".to_string()),
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
    }

    #[test]
    fn test_should_show_commands() {
        // Show commands when no specific type filter
        let cmd = create_default_command();
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

        // Don't show commands when agents flag is set
        let cmd = ListCommand {
            agents: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

        // Don't show commands when snippets flag is set
        let cmd = ListCommand {
            snippets: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

        // Show only commands when commands flag is set
        let cmd = ListCommand {
            commands: true,
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

        // Don't show other types when commands flag is set
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

        // Show commands when type is "commands" or "command"
        let cmd = ListCommand {
            r#type: Some("commands".to_string()),
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

        let cmd = ListCommand {
            r#type: Some("command".to_string()),
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));
    }

    #[test]
    fn test_mutually_exclusive_type_filters() {
        // Test that only one type shows when flags are set individually
        let cmd = ListCommand {
            agents: true,
            ..create_default_command()
        };
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

        let cmd = ListCommand {
            snippets: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

        let cmd = ListCommand {
            commands: true,
            ..create_default_command()
        };
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
        assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
        assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));
    }

    #[test]
    fn test_matches_filters_source() {
        let cmd = ListCommand {
            source: Some("official".to_string()),
            ..create_default_command()
        };

        let dep_with_source = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            command: None,
            branch: None,
            rev: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,

            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        let dep_with_different_source =
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));

        let dep_without_source = ResourceDependency::Simple("local/file.md".to_string());

        assert!(cmd.matches_filters("test", Some(&dep_with_source), "agent"));
        assert!(!cmd.matches_filters("test", Some(&dep_with_different_source), "agent"));
        assert!(!cmd.matches_filters("test", Some(&dep_without_source), "agent"));
    }

    #[test]
    fn test_matches_filters_search() {
        let cmd = ListCommand {
            search: Some("code".to_string()),
            ..create_default_command()
        };

        assert!(cmd.matches_filters("code-reviewer", None, "agent"));
        assert!(cmd.matches_filters("my-code-helper", None, "agent"));
        assert!(!cmd.matches_filters("utils", None, "agent"));
    }

    #[test]
    fn test_matches_lockfile_filters_source() {
        let cmd = ListCommand {
            source: Some("official".to_string()),
            ..create_default_command()
        };

        let entry_with_source = LockedResource {
            name: "test".to_string(),
            source: Some("official".to_string()),
            url: None,
            path: "test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "abc123".to_string(),
            installed_at: "test.md".to_string(),
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

        let entry_with_different_source = LockedResource {
            name: "test".to_string(),
            source: Some("community".to_string()),
            url: None,
            path: "test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "abc123".to_string(),
            installed_at: "test.md".to_string(),
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

        let entry_without_source = LockedResource {
            name: "test".to_string(),
            source: None,
            url: None,
            path: "test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "abc123".to_string(),
            installed_at: "test.md".to_string(),
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

        assert!(cmd.matches_lockfile_filters("test", &entry_with_source, "agent"));
        assert!(!cmd.matches_lockfile_filters("test", &entry_with_different_source, "agent"));
        assert!(!cmd.matches_lockfile_filters("test", &entry_without_source, "agent"));
    }

    #[test]
    fn test_matches_lockfile_filters_search() {
        let cmd = ListCommand {
            search: Some("code".to_string()),
            ..create_default_command()
        };

        let entry = LockedResource {
            name: "test".to_string(),
            source: None,
            url: None,
            path: "test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "abc123".to_string(),
            installed_at: "test.md".to_string(),
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

        assert!(cmd.matches_lockfile_filters("code-reviewer", &entry, "agent"));
        assert!(cmd.matches_lockfile_filters("my-code-helper", &entry, "agent"));
        assert!(!cmd.matches_lockfile_filters("utils", &entry, "agent"));
    }

    #[test]
    fn test_sort_items() {
        let cmd = ListCommand {
            sort: Some("name".to_string()),
            ..create_default_command()
        };

        let mut items = vec![
            ListItem {
                name: "zebra".to_string(),
                source: None,
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
            ListItem {
                name: "alpha".to_string(),
                source: None,
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
        ];

        cmd.sort_items(&mut items);
        assert_eq!(items[0].name, "alpha");
        assert_eq!(items[1].name, "zebra");
    }

    #[test]
    fn test_sort_items_by_version() {
        let cmd = ListCommand {
            sort: Some("version".to_string()),
            ..create_default_command()
        };

        let mut items = vec![
            ListItem {
                name: "test1".to_string(),
                source: None,
                version: Some("v2.0.0".to_string()),
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
            ListItem {
                name: "test2".to_string(),
                source: None,
                version: Some("v1.0.0".to_string()),
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
        ];

        cmd.sort_items(&mut items);
        assert_eq!(items[0].version, Some("v1.0.0".to_string()));
        assert_eq!(items[1].version, Some("v2.0.0".to_string()));
    }

    #[test]
    fn test_sort_items_by_source() {
        let cmd = ListCommand {
            sort: Some("source".to_string()),
            ..create_default_command()
        };

        let mut items = vec![
            ListItem {
                name: "test1".to_string(),
                source: Some("zebra".to_string()),
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
            ListItem {
                name: "test2".to_string(),
                source: Some("alpha".to_string()),
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
            ListItem {
                name: "test3".to_string(),
                source: None, // Should be treated as "local"
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
        ];

        cmd.sort_items(&mut items);
        assert_eq!(items[0].source, Some("alpha".to_string()));
        assert_eq!(items[1].source, None); // "local" comes before "zebra"
        assert_eq!(items[2].source, Some("zebra".to_string()));
    }

    #[test]
    fn test_sort_items_by_type() {
        let cmd = ListCommand {
            sort: Some("type".to_string()),
            ..create_default_command()
        };

        let mut items = vec![
            ListItem {
                name: "test1".to_string(),
                source: None,
                version: None,
                path: None,
                resource_type: "snippet".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("agpm".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
            ListItem {
                name: "test2".to_string(),
                source: None,
                version: None,
                path: None,
                resource_type: "agent".to_string(),
                installed_at: None,
                checksum: None,
                resolved_commit: None,
                tool: Some("claude-code".to_string()),
                applied_patches: std::collections::BTreeMap::new(),
            },
        ];

        cmd.sort_items(&mut items);
        assert_eq!(items[0].resource_type, "agent");
        assert_eq!(items[1].resource_type, "snippet");
    }

    #[test]
    fn test_lockentry_to_listitem() {
        let cmd = create_default_command();

        let lock_entry = LockedResource {
            name: "test-agent".to_string(),
            source: Some("official".to_string()),
            url: Some("https://example.com/repo.git".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:def456".to_string(),
            installed_at: "agents/test-agent.md".to_string(),
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

        let list_item = cmd.lockentry_to_listitem(&lock_entry, "agent");

        assert_eq!(list_item.name, "test-agent");
        assert_eq!(list_item.source, Some("official".to_string()));
        assert_eq!(list_item.version, Some("v1.0.0".to_string()));
        assert_eq!(list_item.path, Some("agents/test.md".to_string()));
        assert_eq!(list_item.resource_type, "agent");
        assert_eq!(list_item.installed_at, Some("agents/test-agent.md".to_string()));
        assert_eq!(list_item.checksum, Some("sha256:def456".to_string()));
        assert_eq!(list_item.resolved_commit, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_list_with_json_format() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            format: "json".to_string(),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_yaml_format() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            format: "yaml".to_string(),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_compact_format() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            format: "compact".to_string(),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_simple_format() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            format: "simple".to_string(),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_filter_by_agents_only() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with both agents and snippets
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            agents: true,
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_filter_by_snippets_only() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with both agents and snippets
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            snippets: true,
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_filter_by_type() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with both agents and snippets
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        // Test filtering by agents
        let cmd = ListCommand {
            r#type: Some("agents".to_string()),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path.clone()).await;
        assert!(result.is_ok());

        // Test filtering by snippets
        let cmd = ListCommand {
            r#type: Some("snippets".to_string()),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_filter_by_source() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources from different sources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            source: Some("official".to_string()),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_search_by_pattern() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            search: Some("code".to_string()),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_detailed_flag() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            detailed: true,
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_files_flag() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            files: true,
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_verbose_flag() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            verbose: true,
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_with_sort() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with resources
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            sort: Some("name".to_string()),
            manifest: true,
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_empty_lockfile_json_output() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest but no lockfile
        let manifest = create_test_manifest();
        manifest.save(&manifest_path).unwrap();

        let cmd = ListCommand {
            format: "json".to_string(),
            manifest: false, // Use lockfile mode
            ..create_default_command()
        };

        let result = cmd.execute_from_path(manifest_path).await;
        assert!(result.is_ok());
    }
}
