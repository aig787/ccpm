//! Fluent builder for creating agpm.toml manifests in tests
//!
//! This module provides a type-safe, fluent API for constructing test manifests,
//! eliminating manual TOML string formatting and reducing test boilerplate.
//!
//! # Quick Examples
//!
//! ## Simple manifest with one agent
//! ```rust
//! use crate::common::ManifestBuilder;
//!
//! let manifest = ManifestBuilder::new()
//!     .add_source("official", "file:///path/to/repo.git")
//!     .add_agent("my-agent", |d| d
//!         .source("official")
//!         .path("agents/my-agent.md")
//!         .version("v1.0.0")
//!     )
//!     .build();
//! ```
//!
//! ## Manifest with multiple resources
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_sources(&[
//!         ("official", &official_url),
//!         ("community", &community_url),
//!     ])
//!     .add_standard_agent("my-agent", "official", "agents/my-agent.md")
//!     .add_standard_agent("helper", "community", "agents/helper.md")
//!     .add_snippet("utils", |d| d
//!         .source("official")
//!         .path("snippets/utils.md")
//!         .version("v2.0.0")
//!         .tool("agpm")
//!     )
//!     .build();
//! ```
//!
//! ## Local dependencies (no source/version)
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_local_agent("local-helper", "../local/agents/helper.md")
//!     .build();
//! ```
//!
//! ## Pattern-based dependencies
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_source("community", &url)
//!     .add_agent_pattern("ai-agents", "community", "agents/ai/*.md", "v1.0.0")
//!     .build();
//! ```

use std::collections::HashMap;

/// Builder for creating test manifests with type safety
///
/// This builder uses a fluent API pattern to construct agpm.toml manifests
/// programmatically, ensuring type safety and eliminating string formatting errors.
#[derive(Default, Debug)]
pub struct ManifestBuilder {
    sources: HashMap<String, String>,
    target_config: Option<TargetConfig>,
    tools_config: Option<ToolsConfig>,
    agents: Vec<DependencyEntry>,
    snippets: Vec<DependencyEntry>,
    commands: Vec<DependencyEntry>,
    scripts: Vec<DependencyEntry>,
    hooks: Vec<DependencyEntry>,
    mcp_servers: Vec<DependencyEntry>,
    skills: Vec<DependencyEntry>, // NEW: Support for skills
}

/// Configuration for the [target] section
#[derive(Debug, Clone)]
struct TargetConfig {
    agents: Option<String>,
    snippets: Option<String>,
    commands: Option<String>,
    scripts: Option<String>,
    hooks: Option<String>,
    mcp_servers: Option<String>,
    skills: Option<String>, // NEW: Support for skills
    gitignore: Option<bool>,
}

/// Configuration for the [tools] section
#[derive(Debug, Clone)]
struct ToolsConfig {
    tools: HashMap<String, ToolConfig>,
}

/// Configuration for a single tool
#[derive(Debug, Clone)]
struct ToolConfig {
    path: Option<String>,
    enabled: Option<bool>,
    resources: HashMap<String, ResourceConfig>,
}

/// Configuration for a resource type within a tool
#[derive(Debug, Clone)]
struct ResourceConfig {
    path: Option<String>,
    merge_target: Option<String>,
    flatten: Option<bool>,
}

/// Builder for configuring the [tools] section
#[derive(Default, Debug)]
pub struct ToolsConfigBuilder {
    tools: HashMap<String, ToolConfig>,
}

/// Builder for configuring a single tool
#[derive(Debug)]
pub struct ToolConfigBuilder {
    name: String,
    path: Option<String>,
    enabled: Option<bool>,
    resources: HashMap<String, ResourceConfig>,
}

impl ToolConfigBuilder {
    /// Set the tool path
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Enable or disable the tool
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Configure agents resource
    pub fn agents(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("agents".to_string(), config.build());
        self
    }

    /// Configure snippets resource
    pub fn snippets(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("snippets".to_string(), config.build());
        self
    }

    /// Configure commands resource
    pub fn commands(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("commands".to_string(), config.build());
        self
    }

    /// Configure scripts resource
    pub fn scripts(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("scripts".to_string(), config.build());
        self
    }

    /// Configure hooks resource
    pub fn hooks(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("hooks".to_string(), config.build());
        self
    }

    /// Configure mcp-servers resource
    pub fn mcp_servers(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("mcp-servers".to_string(), config.build());
        self
    }

    /// Configure skills resource
    pub fn skills(mut self, config: ResourceConfigBuilder) -> Self {
        self.resources.insert("skills".to_string(), config.build());
        self
    }

    fn build(self) -> ToolConfig {
        ToolConfig {
            path: self.path,
            enabled: self.enabled,
            resources: self.resources,
        }
    }
}

/// Builder for configuring a resource type
#[derive(Default, Debug)]
pub struct ResourceConfigBuilder {
    path: Option<String>,
    merge_target: Option<String>,
    flatten: Option<bool>,
}

impl ResourceConfigBuilder {
    /// Set the resource path
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Set the merge target
    pub fn merge_target(mut self, target: &str) -> Self {
        self.merge_target = Some(target.to_string());
        self
    }

    /// Set flatten option
    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = Some(flatten);
        self
    }

    fn build(self) -> ResourceConfig {
        ResourceConfig {
            path: self.path,
            merge_target: self.merge_target,
            flatten: self.flatten,
        }
    }
}

impl ToolsConfigBuilder {
    /// Add a tool configuration
    pub fn tool<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(ToolConfigBuilder) -> ToolConfigBuilder,
    {
        let builder = ToolConfigBuilder {
            name: name.to_string(),
            path: None,
            enabled: None,
            resources: HashMap::new(),
        };
        let tool_config = config(builder).build();
        self.tools.insert(name.to_string(), tool_config);
        self
    }

    fn build(self) -> ToolsConfig {
        ToolsConfig {
            tools: self.tools,
        }
    }
}

/// Builder for configuring the [target] section
#[derive(Default, Debug)]
pub struct TargetConfigBuilder {
    agents: Option<String>,
    snippets: Option<String>,
    commands: Option<String>,
    scripts: Option<String>,
    hooks: Option<String>,
    mcp_servers: Option<String>,
    skills: Option<String>, // NEW: Support for skills
    gitignore: Option<bool>,
}

impl TargetConfigBuilder {
    /// Set the agents target path
    pub fn agents(mut self, path: &str) -> Self {
        self.agents = Some(path.to_string());
        self
    }

    /// Set the snippets target path
    pub fn snippets(mut self, path: &str) -> Self {
        self.snippets = Some(path.to_string());
        self
    }

    /// Set the commands target path
    pub fn commands(mut self, path: &str) -> Self {
        self.commands = Some(path.to_string());
        self
    }

    /// Set the scripts target path
    pub fn scripts(mut self, path: &str) -> Self {
        self.scripts = Some(path.to_string());
        self
    }

    /// Set the hooks target path
    pub fn hooks(mut self, path: &str) -> Self {
        self.hooks = Some(path.to_string());
        self
    }

    /// Set the mcp-servers target path
    pub fn mcp_servers(mut self, path: &str) -> Self {
        self.mcp_servers = Some(path.to_string());
        self
    }

    /// Set the skills target path
    pub fn skills(mut self, path: &str) -> Self {
        self.skills = Some(path.to_string());
        self
    }

    /// Enable or disable gitignore management
    pub fn gitignore(mut self, enabled: bool) -> Self {
        self.gitignore = Some(enabled);
        self
    }

    fn build(self) -> TargetConfig {
        TargetConfig {
            agents: self.agents,
            snippets: self.snippets,
            commands: self.commands,
            scripts: self.scripts,
            hooks: self.hooks,
            mcp_servers: self.mcp_servers,
            skills: self.skills,
            gitignore: self.gitignore,
        }
    }
}

/// A single dependency entry with all possible fields
#[derive(Debug, Clone)]
struct DependencyEntry {
    name: String,
    source: Option<String>,
    path: String,
    version: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    tool: Option<String>,
    target: Option<String>,
    flatten: Option<bool>,
}

/// Builder for configuring a single dependency
///
/// This builder is used within the resource-specific methods (add_agent, add_snippet, etc.)
/// to configure individual dependencies with a fluent API.
#[derive(Debug)]
pub struct DependencyBuilder {
    name: String,
    source: Option<String>,
    path: Option<String>,
    version: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    tool: Option<String>,
    target: Option<String>,
    flatten: Option<bool>,
}

impl DependencyBuilder {
    /// Set the source repository name
    pub fn source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Set the path to the resource in the repository
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Set the version constraint (e.g., "v1.0.0", "^v2.0", "main")
    pub fn version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    /// Set the branch reference (e.g., "main", "develop")
    pub fn branch(mut self, branch: &str) -> Self {
        self.branch = Some(branch.to_string());
        self
    }

    /// Set the commit reference (e.g., "abc123def")
    pub fn rev(mut self, rev: &str) -> Self {
        self.rev = Some(rev.to_string());
        self
    }

    /// Set the target tool (e.g., "claude-code", "opencode", "agpm")
    pub fn tool(mut self, tool: &str) -> Self {
        self.tool = Some(tool.to_string());
        self
    }

    /// Set a custom installation target path
    pub fn target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// Control directory structure flattening
    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = Some(flatten);
        self
    }

    /// Build the dependency entry (internal use)
    fn build(self) -> DependencyEntry {
        DependencyEntry {
            name: self.name,
            source: self.source,
            path: self.path.expect("path is required for dependency"),
            version: self.version,
            branch: self.branch,
            rev: self.rev,
            tool: self.tool,
            target: self.target,
            flatten: self.flatten,
        }
    }
}

impl ManifestBuilder {
    /// Create a new empty manifest builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source repository
    ///
    /// # Example
    /// ```rust
    /// builder.add_source("official", "file:///path/to/repo.git")
    /// ```
    pub fn add_source(mut self, name: &str, url: &str) -> Self {
        self.sources.insert(name.to_string(), url.to_string());
        self
    }

    /// Add multiple sources at once
    ///
    /// # Example
    /// ```rust
    /// builder.add_sources(&[
    ///     ("official", &official_url),
    ///     ("community", &community_url),
    /// ])
    /// ```
    pub fn add_sources(mut self, sources: &[(&str, &str)]) -> Self {
        for (name, url) in sources {
            self.sources.insert(name.to_string(), url.to_string());
        }
        self
    }

    /// Configure the [target] section with custom paths and gitignore setting
    ///
    /// # Example
    /// ```rust
    /// builder.with_target_config(|t| t
    ///     .agents(".claude/agents")
    ///     .snippets(".agpm/snippets")
    ///     .gitignore(true)
    /// )
    /// ```
    pub fn with_target_config<F>(mut self, config: F) -> Self
    where
        F: FnOnce(TargetConfigBuilder) -> TargetConfigBuilder,
    {
        let builder = TargetConfigBuilder::default();
        self.target_config = Some(config(builder).build());
        self
    }

    /// Configure the [tools] section with custom tool configurations
    ///
    /// # Example
    /// ```rust
    /// builder.with_tools_config(|t| t
    ///     .tool("claude-code", |c| c
    ///         .path(".claude")
    ///         .agents(ResourceConfigBuilder::default().path("agents"))
    ///         .hooks(ResourceConfigBuilder::default().merge_target(".claude/settings.local.json"))
    ///     )
    /// )
    /// ```
    pub fn with_tools_config<F>(mut self, config: F) -> Self
    where
        F: FnOnce(ToolsConfigBuilder) -> ToolsConfigBuilder,
    {
        let builder = ToolsConfigBuilder::default();
        self.tools_config = Some(config(builder).build());
        self
    }

    /// Set gitignore field in [target] section (convenience method)
    ///
    /// # Example
    /// ```rust
    /// builder.with_gitignore(true)
    /// ```
    pub fn with_gitignore(mut self, enabled: bool) -> Self {
        if let Some(ref mut config) = self.target_config {
            config.gitignore = Some(enabled);
        } else {
            self.target_config = Some(TargetConfig {
                agents: None,
                snippets: None,
                commands: None,
                scripts: None,
                hooks: None,
                mcp_servers: None,
                skills: None,
                gitignore: Some(enabled),
            });
        }
        self
    }

    /// Add an agent dependency with full configuration
    ///
    /// # Example
    /// ```rust
    /// builder.add_agent("my-agent", |d| d
    ///     .source("official")
    ///     .path("agents/my-agent.md")
    ///     .version("v1.0.0")
    /// )
    /// ```
    pub fn add_agent<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.agents.push(entry);
        self
    }

    /// Add a snippet dependency with full configuration
    pub fn add_snippet<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.snippets.push(entry);
        self
    }

    /// Add a command dependency with full configuration
    pub fn add_command<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.commands.push(entry);
        self
    }

    /// Add a script dependency with full configuration
    pub fn add_script<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.scripts.push(entry);
        self
    }

    /// Add a hook dependency with full configuration
    pub fn add_hook<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.hooks.push(entry);
        self
    }

    /// Add an MCP server dependency with full configuration
    pub fn add_mcp_server<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.mcp_servers.push(entry);
        self
    }

    /// Add a skill dependency with full configuration
    ///
    /// # Example
    /// ```rust
    /// builder.add_skill("my-skill", |d| d
    ///     .source("official")
    ///     .path("skills/my-skill")
    ///     .version("v1.0.0")
    /// )
    /// ```
    pub fn add_skill<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
            flatten: None,
        };
        let entry = config(builder).build();
        self.skills.push(entry);
        self
    }

    /// Build the final TOML string
    ///
    /// Constructs a valid agpm.toml manifest from the builder state.
    pub fn build(self) -> String {
        // Helper to escape string values for TOML (backslashes need to be doubled)
        fn escape_toml_string(s: &str) -> String {
            s.replace('\\', "\\\\")
        }

        let mut toml = String::new();

        // Sources section
        if !self.sources.is_empty() {
            toml.push_str("[sources]\n");
            for (name, url) in &self.sources {
                toml.push_str(&format!("{} = \"{}\"\n", name, escape_toml_string(url)));
            }
            toml.push('\n');
        }

        // Helper to format dependency sections
        fn format_dependencies(toml: &mut String, section: &str, deps: &[DependencyEntry]) {
            if !deps.is_empty() {
                toml.push_str(&format!("[{}]\n", section));
                for dep in deps {
                    toml.push_str(&format!("{} = {{ ", dep.name));

                    if let Some(source) = &dep.source {
                        toml.push_str(&format!("source = \"{}\", ", escape_toml_string(source)));
                    }

                    toml.push_str(&format!("path = \"{}\"", escape_toml_string(&dep.path)));

                    if let Some(version) = &dep.version {
                        toml.push_str(&format!(", version = \"{}\"", escape_toml_string(version)));
                    }

                    if let Some(branch) = &dep.branch {
                        toml.push_str(&format!(", branch = \"{}\"", escape_toml_string(branch)));
                    }

                    if let Some(rev) = &dep.rev {
                        toml.push_str(&format!(", rev = \"{}\"", escape_toml_string(rev)));
                    }

                    if let Some(tool) = &dep.tool {
                        toml.push_str(&format!(", tool = \"{}\"", escape_toml_string(tool)));
                    }

                    if let Some(target) = &dep.target {
                        toml.push_str(&format!(", target = \"{}\"", escape_toml_string(target)));
                    }

                    if let Some(flatten) = dep.flatten {
                        toml.push_str(&format!(", flatten = {}", flatten));
                    }

                    toml.push_str(" }\n");
                }
                toml.push('\n');
            }
        }

        // Format all resource sections
        format_dependencies(&mut toml, "agents", &self.agents);
        format_dependencies(&mut toml, "snippets", &self.snippets);
        format_dependencies(&mut toml, "commands", &self.commands);
        format_dependencies(&mut toml, "scripts", &self.scripts);
        format_dependencies(&mut toml, "hooks", &self.hooks);
        format_dependencies(&mut toml, "mcp-servers", &self.mcp_servers);
        format_dependencies(&mut toml, "skills", &self.skills);

        // Tools configuration section
        if let Some(config) = self.tools_config {
            if !config.tools.is_empty() {
                toml.push_str("[tools]\n");
                for (tool_name, tool_config) in &config.tools {
                    toml.push_str(&format!("[tools.{}]\n", tool_name));

                    if let Some(path) = &tool_config.path {
                        toml.push_str(&format!("path = \"{}\"\n", escape_toml_string(path)));
                    }

                    if let Some(enabled) = tool_config.enabled {
                        toml.push_str(&format!("enabled = {}\n", enabled));
                    }

                    if !tool_config.resources.is_empty() {
                        toml.push_str("[tools.");
                        toml.push_str(tool_name);
                        toml.push_str(".resources]\n");

                        for (resource_name, resource_config) in &tool_config.resources {
                            toml.push_str(resource_name);
                            toml.push_str(" = { ");

                            let mut has_fields = false;

                            if let Some(path) = &resource_config.path {
                                toml.push_str(&format!("path = \"{}\"", escape_toml_string(path)));
                                has_fields = true;
                            }

                            if let Some(merge_target) = &resource_config.merge_target {
                                if has_fields {
                                    toml.push_str(", ");
                                }
                                toml.push_str(&format!(
                                    "merge-target = \"{}\"",
                                    escape_toml_string(merge_target)
                                ));
                                has_fields = true;
                            }

                            if let Some(flatten) = resource_config.flatten {
                                if has_fields {
                                    toml.push_str(", ");
                                }
                                toml.push_str(&format!("flatten = {}", flatten));
                            }

                            toml.push_str(" }\n");
                        }
                    }

                    toml.push('\n');
                }
            }
        }

        // Target configuration section
        if let Some(config) = self.target_config {
            let mut has_fields = false;
            let mut target_section = String::from("[target]\n");

            if let Some(path) = config.agents {
                target_section.push_str(&format!("agents = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.snippets {
                target_section.push_str(&format!("snippets = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.commands {
                target_section.push_str(&format!("commands = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.scripts {
                target_section.push_str(&format!("scripts = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.hooks {
                target_section.push_str(&format!("hooks = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.mcp_servers {
                target_section
                    .push_str(&format!("mcp-servers = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.skills {
                target_section.push_str(&format!("skills = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(enabled) = config.gitignore {
                target_section.push_str(&format!("gitignore = {}\n", enabled));
                has_fields = true;
            }

            if has_fields {
                toml.push_str(&target_section);
                toml.push('\n');
            }
        }

        toml
    }
}

// Convenience methods for common patterns
impl ManifestBuilder {
    /// Quick add: agent with standard v1.0.0 version from source
    ///
    /// # Example
    /// ```rust
    /// builder.add_standard_agent("my-agent", "official", "agents/my-agent.md")
    /// ```
    pub fn add_standard_agent(self, name: &str, source: &str, path: &str) -> Self {
        self.add_agent(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: snippet with standard v1.0.0 version from source
    pub fn add_standard_snippet(self, name: &str, source: &str, path: &str) -> Self {
        self.add_snippet(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: command with standard v1.0.0 version from source
    pub fn add_standard_command(self, name: &str, source: &str, path: &str) -> Self {
        self.add_command(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: local agent dependency (no source/version)
    ///
    /// # Example
    /// ```rust
    /// builder.add_local_agent("local-helper", "../local/agents/helper.md")
    /// ```
    pub fn add_local_agent(self, name: &str, path: &str) -> Self {
        self.add_agent(name, |d| d.path(path))
    }

    /// Quick add: local snippet dependency (no source/version)
    pub fn add_local_snippet(self, name: &str, path: &str) -> Self {
        self.add_snippet(name, |d| d.path(path))
    }

    /// Quick add: local command dependency (no source/version)
    pub fn add_local_command(self, name: &str, path: &str) -> Self {
        self.add_command(name, |d| d.path(path))
    }

    /// Quick add: pattern-based agent dependency
    ///
    /// # Example
    /// ```rust
    /// builder.add_agent_pattern("ai-agents", "community", "agents/ai/*.md", "v1.0.0")
    /// ```
    pub fn add_agent_pattern(self, name: &str, source: &str, pattern: &str, version: &str) -> Self {
        self.add_agent(name, |d| d.source(source).path(pattern).version(version))
    }

    /// Quick add: pattern-based snippet dependency
    pub fn add_snippet_pattern(
        self,
        name: &str,
        source: &str,
        pattern: &str,
        version: &str,
    ) -> Self {
        self.add_snippet(name, |d| d.source(source).path(pattern).version(version))
    }

    /// Add standard claude-code tool configuration
    ///
    /// # Example
    /// ```rust
    /// builder.with_claude_code_tool()
    /// ```
    pub fn with_claude_code_tool(self) -> Self {
        self.with_tools_config(|t| {
            t.tool("claude-code", |c| {
                c.path(".claude")
                    .agents(ResourceConfigBuilder::default().path("agents"))
                    .snippets(ResourceConfigBuilder::default().path("snippets"))
                    .commands(ResourceConfigBuilder::default().path("commands"))
                    .scripts(ResourceConfigBuilder::default().path("scripts"))
                    .hooks(
                        ResourceConfigBuilder::default()
                            .merge_target(".claude/settings.local.json"),
                    )
                    .mcp_servers(ResourceConfigBuilder::default().merge_target(".mcp.json"))
                    .skills(ResourceConfigBuilder::default().path("skills"))
            })
        })
    }

    /// Add standard opencode tool configuration
    ///
    /// # Example
    /// ```rust
    /// builder.with_opencode_tool()
    /// ```
    pub fn with_opencode_tool(self) -> Self {
        self.with_tools_config(|t| {
            t.tool("opencode", |c| {
                c.path(".opencode")
                    .enabled(false)
                    .agents(ResourceConfigBuilder::default().path("agent"))
                    .commands(ResourceConfigBuilder::default().path("command"))
                    .mcp_servers(
                        ResourceConfigBuilder::default().merge_target(".opencode/opencode.json"),
                    )
            })
        })
    }

    /// Add malformed tool configuration for testing (hooks with path instead of merge_target)
    ///
    /// # Example
    /// ```rust
    /// builder.with_malformed_hooks_tool()
    /// ```
    pub fn with_malformed_hooks_tool(self) -> Self {
        self.with_tools_config(|t| {
            t.tool("claude-code", |c| {
                c.path(".claude")
                    .agents(ResourceConfigBuilder::default().path("agents"))
                    .snippets(ResourceConfigBuilder::default().path("snippets"))
                    .commands(ResourceConfigBuilder::default().path("commands"))
                    .scripts(ResourceConfigBuilder::default().path("scripts"))
                    .hooks(ResourceConfigBuilder::default()) // Empty - no path or merge_target
                    .mcp_servers(ResourceConfigBuilder::default().merge_target(".mcp.json"))
            })
        })
    }

    /// Add tool configuration with missing hooks resource
    ///
    /// # Example
    /// ```rust
    /// builder.with_missing_hooks_tool()
    /// ```
    pub fn with_missing_hooks_tool(self) -> Self {
        self.with_tools_config(|t| {
            t.tool("claude-code", |c| {
                c.path(".claude")
                    .agents(ResourceConfigBuilder::default().path("agents"))
                    .snippets(ResourceConfigBuilder::default().path("snippets"))
                    .commands(ResourceConfigBuilder::default().path("commands"))
                    .scripts(ResourceConfigBuilder::default().path("scripts"))
                    // hooks completely missing
                    .mcp_servers(ResourceConfigBuilder::default().merge_target(".mcp.json"))
            })
        })
    }

    /// Add tool configuration with empty hooks resource
    ///
    /// # Example
    /// ```rust
    /// builder.with_empty_hooks_tool()
    /// ```
    pub fn with_empty_hooks_tool(self) -> Self {
        self.with_tools_config(|t| {
            t.tool("claude-code", |c| {
                c.path(".claude")
                    .agents(ResourceConfigBuilder::default().path("agents"))
                    .snippets(ResourceConfigBuilder::default().path("snippets"))
                    .commands(ResourceConfigBuilder::default().path("commands"))
                    .scripts(ResourceConfigBuilder::default().path("scripts"))
                    .hooks(ResourceConfigBuilder::default()) // Empty - no path or merge_target
                    .mcp_servers(ResourceConfigBuilder::default().merge_target(".mcp.json"))
            })
        })
    }
}
