//! Manifest file parsing and validation for AGPM projects.
//!
//! This module handles the `agpm.toml` manifest file that defines project
//! dependencies and configuration. The manifest uses TOML format and follows
//! a structure similar to Cargo.toml, providing a lockfile-based dependency
//! management system for Claude Code resources.
//!
//! # Overview
//!
//! The manifest system enables:
//! - Declarative dependency management through `agpm.toml`
//! - Reproducible installations via lockfile generation
//! - Support for multiple Git-based source repositories
//! - Local and remote dependency resolution
//! - Version constraint specification and validation
//! - Transitive dependency resolution from resource metadata
//! - Cross-platform path handling and installation
//! - MCP (Model Context Protocol) server configuration management
//! - Atomic file operations for reliability
//!
//! # Complete TOML Format Specification
//!
//! ## Basic Structure
//!
//! A `agpm.toml` manifest file consists of four main sections:
//!
//! ```toml
//! # Named source repositories (optional)
//! [sources]
//! # Git repository URLs mapped to convenient names
//! official = "https://github.com/example-org/agpm-official.git"
//! community = "https://github.com/community/agpm-resources.git"
//! private = "git@github.com:company/private-resources.git"
//!
//! # Installation target directories (optional)
//! [target]
//! # Where agents should be installed (default: ".claude/agents")
//! agents = ".claude/agents"
//! # Where snippets should be installed (default: ".agpm/snippets")
//! snippets = ".agpm/snippets"
//! # Where commands should be installed (default: ".claude/commands")
//! commands = ".claude/commands"
//!
//! # Agent dependencies (optional)
//! [agents]
//! # Various dependency specification formats
//! simple-agent = "../local/agents/helper.md"                    # Local path
//! remote-agent = { source = "official", path = "agents/reviewer.md", version = "v1.0.0" }
//! latest-agent = { source = "community", path = "agents/utils.md", version = "latest" }
//! branch-agent = { source = "private", path = "agents/internal.md", git = "develop" }
//! commit-agent = { source = "official", path = "agents/stable.md", git = "abc123..." }
//! # Custom target installation directory (relative to .claude)
//! custom-agent = { source = "official", path = "agents/special.md", version = "v1.0.0", target = "integrations/ai" }
//!
//! # Snippet dependencies (optional)
//! [snippets]
//! # Same formats as agents
//! local-snippet = "../shared/snippets/common.md"
//! remote-snippet = { source = "community", path = "snippets/utils.md", version = "v2.1.0" }
//! # Custom target for special snippets
//! integration-snippet = { source = "community", path = "snippets/api.md", version = "v1.0.0", target = "tools/snippets" }
//!
//! # Command dependencies (optional)
//! [commands]
//! # Same formats as agents and snippets
//! local-command = "../shared/commands/helper.md"
//! remote-command = { source = "community", path = "commands/build.md", version = "v1.0.0" }
//! ```
//!
//! ## Sources Section
//!
//! The `[sources]` section maps convenient names to Git repository URLs:
//!
//! ```toml
//! [sources]
//! # HTTPS URLs (recommended for public repositories)
//! official = "https://github.com/owner/agpm-resources.git"
//! community = "https://gitlab.com/group/agpm-community.git"
//!
//! # SSH URLs (for private repositories with key authentication)
//! private = "git@github.com:company/private-resources.git"
//! internal = "git@gitlab.company.com:team/internal-resources.git"
//!
//! # Local Git repository URLs
//! local-repo = "file:///absolute/path/to/local/repo"
//!
//! # Environment variable expansion (useful for CI/CD)
//! dynamic = "https://github.com/${GITHUB_ORG}/resources.git"
//! home-repo = "file://${HOME}/git/resources"
//! ```
//!
//! ## Target Section
//!
//! The `[target]` section configures where resources are installed:
//!
//! ```toml
//! [target]
//! # Default values shown - these can be customized
//! agents = ".claude/agents"      # Where agent .md files are copied
//! snippets = ".agpm/snippets"  # Where snippet .md files are copied
//! commands = ".claude/commands"  # Where command .md files are copied
//!
//! # Alternative configurations
//! agents = "resources/agents"
//! snippets = "resources/snippets"
//! commands = "resources/commands"
//!
//! # Absolute paths are supported
//! agents = "/opt/claude/agents"
//! snippets = "/opt/claude/snippets"
//! commands = "/opt/claude/commands"
//! ```
//!
//! ## Dependency Sections
//!
//! Both `[agents]` and `[snippets]` sections support multiple dependency formats:
//!
//! ### 1. Local Path Dependencies
//!
//! For resources in your local filesystem:
//!
//! ```toml
//! [agents]
//! # Relative paths from manifest directory
//! local-helper = "../shared/agents/helper.md"
//! nearby-agent = "./local-agents/custom.md"
//!
//! # Absolute paths (not recommended for portability)
//! system-agent = "/usr/local/share/claude/agents/system.md"
//! ```
//!
//! Local dependencies:
//! - Do not support version constraints
//! - Are copied directly from the filesystem
//! - Are not cached or managed through Git
//! - Must exist at install time
//!
//! ### 2. Remote Source Dependencies
//!
//! For resources from Git repositories:
//!
//! ```toml
//! [agents]
//! # Basic remote dependency with semantic version
//! code-reviewer = { source = "official", path = "agents/reviewer.md", version = "v1.0.0" }
//!
//! # Using latest version (not recommended for production)
//! utils = { source = "community", path = "agents/utils.md", version = "latest" }
//!
//! # Specific Git branch
//! bleeding-edge = { source = "official", path = "agents/experimental.md", git = "develop" }
//!
//! # Specific Git commit (maximum reproducibility)
//! stable = { source = "official", path = "agents/stable.md", git = "a1b2c3d4e5f6..." }
//!
//! # Git tag (alternative to version field)
//! tagged = { source = "community", path = "agents/tagged.md", git = "release-2.0" }
//! ```
//!
//! ### 3. Custom Target Installation
//!
//! Dependencies can specify a custom installation directory using the `target` field:
//!
//! ```toml
//! [agents]
//! # Install to .claude/integrations/ai/ instead of .claude/agents/
//! integration-agent = {
//!     source = "official",
//!     path = "agents/integration.md",
//!     version = "v1.0.0",
//!     target = "integrations/ai"
//! }
//!
//! # Organize tools in a custom structure
//! debug-tool = {
//!     source = "community",
//!     path = "agents/debugger.md",
//!     version = "v2.0.0",
//!     target = "development/tools"
//! }
//!
//! [snippets]
//! # Custom location for API snippets
//! api-helper = {
//!     source = "community",
//!     path = "snippets/api.md",
//!     version = "v1.0.0",
//!     target = "api/snippets"
//! }
//! ```
//!
//! Custom targets:
//! - Are always relative to the `.claude` directory
//! - Leading `.claude/` or `/` are automatically stripped
//! - Directories are created if they don't exist
//! - Help organize resources in complex projects
//!
//! ### 4. Custom Filenames
//!
//! Dependencies can specify a custom filename using the `filename` field:
//!
//! ```toml
//! [agents]
//! # Install as "ai-assistant.md" instead of "my-ai.md"
//! my-ai = {
//!     source = "official",
//!     path = "agents/complex-long-name-v2.md",
//!     version = "v1.0.0",
//!     filename = "ai-assistant.md"
//! }
//!
//! # Change both filename and extension
//! doc-helper = {
//!     source = "community",
//!     path = "agents/documentation.md",
//!     version = "v2.0.0",
//!     filename = "docs.txt"
//! }
//!
//! # Combine custom target and filename
//! special-tool = {
//!     source = "official",
//!     path = "agents/debug-analyzer-enhanced.md",
//!     version = "v1.0.0",
//!     target = "tools/debugging",
//!     filename = "analyzer.markdown"
//! }
//!
//! [scripts]
//! # Rename script during installation
//! data-processor = {
//!     source = "community",
//!     path = "scripts/data-processor-v3.py",
//!     version = "v1.0.0",
//!     filename = "process.py"
//! }
//! ```
//!
//! Custom filenames:
//! - Include the full filename with extension
//! - Override the default name (based on dependency key)
//! - Work with any resource type
//! - Can be combined with custom targets
//!
//! ## Version Constraint Syntax
//!
//! AGPM supports flexible version constraints:
//!
//! - `"v1.0.0"` - Exact semantic version
//! - `"1.0.0"` - Exact version (v prefix optional)
//! - `"latest"` - Always use the latest available version
//! - `"main"` - Use the main/master branch HEAD
//! - `"develop"` - Use a specific branch
//! - `"a1b2c3d4..."` - Use a specific commit SHA
//! - `"release-1.0"` - Use a specific Git tag
//!
//! ## Complete Examples
//!
//! ### Minimal Manifest
//!
//! ```toml
//! [agents]
//! helper = "../agents/helper.md"
//! ```
//!
//! ### Production Manifest
//!
//! ```toml
//! [sources]
//! official = "https://github.com/claude-org/official-resources.git"
//! community = "https://github.com/claude-community/resources.git"
//! company = "git@github.com:mycompany/claude-resources.git"
//!
//! [target]
//! agents = "resources/agents"
//! snippets = "resources/snippets"
//!
//! [agents]
//! # Production agents with pinned versions
//! code-reviewer = { source = "official", path = "agents/code-reviewer.md", version = "v2.1.0" }
//! documentation = { source = "community", path = "agents/doc-writer.md", version = "v1.5.2" }
//! internal-helper = { source = "company", path = "agents/helper.md", version = "v1.0.0" }
//!
//! # Local customizations
//! custom-agent = "./local/agents/custom.md"
//!
//! [snippets]
//! # Utility snippets
//! common-patterns = { source = "community", path = "snippets/patterns.md", version = "v1.2.0" }
//! company-templates = { source = "company", path = "snippets/templates.md", version = "latest" }
//! ```
//!
//! ## Security Considerations
//!
//! **CRITICAL**: Never include authentication credentials in `agpm.toml`:
//!
//! ```toml
//! # ❌ NEVER DO THIS - credentials will be committed to git
//! [sources]
//! private = "https://token:ghp_xxxx@github.com/company/repo.git"
//!
//! # ✅ Instead, use global configuration in ~/.agpm/config.toml
//! # Or use SSH keys with git@ URLs
//! [sources]
//! private = "git@github.com:company/repo.git"
//! ```
//!
//! Authentication should be configured globally in `~/.agpm/config.toml` or
//! through SSH keys for `git@` URLs. See [`crate::config`] for details.
//!
//! ## Relationship to Lockfile
//!
//! The manifest works together with the lockfile (`agpm.lock`):
//!
//! - **Manifest (`agpm.toml`)**: Declares dependencies and constraints
//! - **Lockfile (`agpm.lock`)**: Records exact resolved versions and checksums
//!
//! When you run `agpm install`:
//! 1. Reads dependencies from `agpm.toml`
//! 2. Resolves versions within constraints  
//! 3. Generates/updates `agpm.lock` with exact commits
//! 4. Installs resources to target directories
//!
//! See [`crate::lockfile`] for lockfile format details.
//!
//! ## Cross-Platform Compatibility
//!
//! AGPM handles platform differences automatically:
//! - Path separators (/ vs \\) are normalized
//! - Home directory expansion (~) is supported
//! - Environment variable expansion is available
//! - Git commands work on Windows, macOS, and Linux
//! - Long path support on Windows (>260 characters)
//! - Unicode filenames and paths are fully supported
//!
//! ## Best Practices
//!
//! 1. **Use semantic versions**: Prefer `v1.0.0` over `latest`
//! 2. **Pin production dependencies**: Use exact versions in production
//! 3. **Organize sources logically**: Group by organization or purpose
//! 4. **Document dependencies**: Add comments explaining why each is needed
//! 5. **Keep manifests simple**: Avoid overly complex dependency trees
//! 6. **Use SSH for private repos**: More secure than HTTPS tokens
//! 7. **Test across platforms**: Verify paths work on all target systems
//! 8. **Version control manifests**: Always commit `agpm.toml` to git
//! 9. **Validate regularly**: Run `agpm validate` before commits
//! 10. **Use lockfiles**: Commit `agpm.lock` for reproducible builds
//!
//! ## Transitive Dependencies
//!
//! Resources can declare their own dependencies within their files using structured
//! metadata. This enables automatic dependency resolution without manual manifest updates.
//!
//! ### Supported Formats
//!
//! #### Markdown Files (YAML Frontmatter)
//!
//! ```markdown
//! ---
//! dependencies:
//!   agents:
//!     - path: agents/helper.md
//!       version: v1.0.0
//!     - path: agents/reviewer.md
//!   snippets:
//!     - path: snippets/utils.md
//! ---
//!
//! # My Command Documentation
//! ...
//! ```
//!
//! #### JSON Files (Top-Level Field)
//!
//! ```json
//! {
//!   "events": ["UserPromptSubmit"],
//!   "type": "command",
//!   "command": ".claude/scripts/test.js",
//!   "dependencies": {
//!     "scripts": [
//!       { "path": "scripts/test-runner.sh", "version": "v1.0.0" },
//!       { "path": "scripts/validator.py" }
//!     ],
//!     "agents": [
//!       { "path": "agents/code-analyzer.md", "version": "~1.2.0" }
//!     ]
//!   }
//! }
//! ```
//!
//! ### Key Features
//!
//! - **Automatic Discovery**: Dependencies extracted during resolution
//! - **Version Inheritance**: If no version specified, parent's version is used
//! - **Same-Source Model**: Transitive deps inherit parent's source repository
//! - **Cycle Detection**: Circular dependency loops are detected and prevented
//! - **Topological Ordering**: Dependencies installed in correct order
//! - **Optional Resolution**: Can be disabled with `--no-transitive` flag
//!
//! ### Data Structures
//!
//! Transitive dependencies are represented by:
//! - [`DependencySpec`]: Individual dependency specification (path + optional version)
//! - [`DependencyMetadata`]: Collection of dependencies by resource type
//! - [`DetailedDependency::dependencies`]: Field storing extracted transitive deps
//!
//! ### Processing Flow
//!
//! 1. Manifest dependencies are resolved first
//! 2. Resource files are checked for metadata (YAML frontmatter or JSON fields)
//! 3. Discovered dependencies are added to dependency graph
//! 4. Graph is validated for cycles
//! 5. Dependencies are resolved in topological order
//! 6. All resources (direct + transitive) are installed
//!
//! See [`dependency_spec`] module for detailed specification formats.
//!
//! ## Error Handling
//!
//! The manifest module provides comprehensive error handling with:
//! - **Context-rich errors**: Detailed messages with actionable suggestions
//! - **Validation errors**: Clear explanations of manifest problems
//! - **I/O errors**: Helpful context for file system issues
//! - **TOML parsing errors**: Specific syntax error locations
//! - **Security validation**: Detection of potential security issues
//!
//! All errors implement [`std::error::Error`] and provide both user-friendly
//! messages and programmatic access to error details.
//!
//! ## Performance Characteristics
//!
//! - **Parsing**: O(n) where n is the manifest file size
//! - **Validation**: O(d) where d is the number of dependencies
//! - **Serialization**: O(n) where n is the total data size
//! - **Memory usage**: Proportional to manifest complexity
//! - **Thread safety**: All operations are thread-safe
//!
//! ## Integration with Other Modules
//!
//! The manifest module works closely with other AGPM modules:
//!
//! ### With [`crate::resolver`]
//!
//! ```rust,ignore
//! use agpm_cli::manifest::Manifest;
//! use agpm_cli::resolver::DependencyResolver;
//!
//! let manifest = Manifest::load(&project_path.join("agpm.toml"))?;
//! let resolver = DependencyResolver::new(&manifest);
//! let resolved = resolver.resolve_all().await?;
//! ```
//!
//! ### With [`crate::lockfile`]
//!
//! ```rust,ignore  
//! use agpm_cli::manifest::Manifest;
//! use agpm_cli::lockfile::LockFile;
//!
//! let manifest = Manifest::load(&project_path.join("agpm.toml"))?;
//! let lockfile = LockFile::generate_from_manifest(&manifest).await?;
//! lockfile.save(&project_path.join("agpm.lock"))?;
//! ```
//!
//! ### With [`crate::git`] for Source Management
//!
//! ```rust,ignore
//! use agpm_cli::manifest::Manifest;
//! use agpm_cli::git::GitManager;
//!
//! let manifest = Manifest::load(&project_path.join("agpm.toml"))?;
//! let git = GitManager::new(&cache_dir);
//!
//! for (name, url) in &manifest.sources {
//!     git.clone_or_update(name, url).await?;
//! }
//! ```

pub mod dependency_spec;
pub mod helpers;
pub mod patches;
pub mod resource_dependency;
pub mod tool_config;

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod tool_config_tests;

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub use dependency_spec::{DependencyMetadata, DependencySpec};
pub use helpers::{expand_url, find_manifest, find_manifest_from, find_manifest_with_optional};
pub use patches::{ManifestPatches, PatchConflict, PatchData, PatchOrigin};
pub use resource_dependency::{DetailedDependency, ResourceDependency};
pub use tool_config::{ArtifactTypeConfig, ResourceConfig, ToolsConfig, WellKnownTool};

/// The main manifest file structure representing a complete `agpm.toml` file.
///
/// This struct encapsulates all configuration for a AGPM project, including
/// source repositories, installation targets, and resource dependencies.
/// It provides the foundation for declarative dependency management similar
/// to Cargo's `Cargo.toml`.
///
/// # Structure
///
/// - **Sources**: Named Git repositories that can be referenced by dependencies
/// - **Target**: Installation directories for different resource types
/// - **Agents**: AI agent dependencies (`.md` files with agent definitions)
/// - **Snippets**: Code snippet dependencies (`.md` files with reusable code)
/// - **Commands**: Claude Code command dependencies (`.md` files with slash commands)
///
/// # Serialization
///
/// The struct uses Serde for TOML serialization/deserialization with these behaviors:
/// - Empty collections are omitted from serialized output for cleaner files
/// - Default values are automatically applied for missing fields
/// - Field names match TOML section names exactly
///
/// # Thread Safety
///
/// This struct is thread-safe and can be shared across async tasks safely.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::manifest::{Manifest, ResourceDependency};
///
/// // Create a new empty manifest
/// let mut manifest = Manifest::new();
///
/// // Add a source repository
/// manifest.add_source(
///     "community".to_string(),
///     "https://github.com/claude-community/resources.git".to_string()
/// );
///
/// // Add a dependency
/// manifest.add_dependency(
///     "helper".to_string(),
///     ResourceDependency::Simple("../local/helper.md".to_string()),
///     true  // is_agent = true
/// );
/// ```
/// Project-specific template variables for AI coding assistants.
///
/// An arbitrary map of user-defined variables that can be referenced in resource templates.
/// This provides maximum flexibility for teams to organize project context however they want,
/// without imposing any predefined structure.
///
/// # Use Case: AI Agent Context
///
/// When AI agents work on your codebase, they need context about:
/// - Where to find coding standards and style guides
/// - What conventions to follow (formatting, naming, patterns)
/// - Where architecture and design docs are located
/// - Project-specific requirements (testing, security, performance)
///
/// # Template Access
///
/// All variables are accessible in templates under the `agpm.project` namespace.
/// The structure is completely user-defined.
///
/// # Examples
///
/// ## Flexible Structure - Organize However You Want
/// ```toml
/// [project]
/// # Top-level variables
/// style_guide = "docs/STYLE_GUIDE.md"
/// max_line_length = 100
/// test_framework = "pytest"
///
/// # Nested sections (optional, just for organization)
/// [project.paths]
/// architecture = "docs/ARCHITECTURE.md"
/// conventions = "docs/CONVENTIONS.md"
///
/// [project.standards]
/// indent_style = "spaces"
/// indent_size = 4
/// ```
///
/// ## Template Usage
/// ```markdown
/// # Code Reviewer
/// Follow guidelines at: {{ agpm.project.style_guide }}
/// Max line length: {{ agpm.project.max_line_length }}
/// Architecture: {{ agpm.project.paths.architecture }}
/// ```
///
/// ## Any Structure Works
/// ```toml
/// [project]
/// whatever = "you want"
/// numbers = 42
/// arrays = ["work", "too"]
///
/// [project.deeply.nested.structure]
/// is_allowed = true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig(toml::map::Map<String, toml::Value>);

impl ProjectConfig {
    /// Convert this ProjectConfig to a serde_json::Value for template rendering.
    ///
    /// This method handles conversion of TOML values to JSON values, which is necessary
    /// for proper Tera template rendering.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::ProjectConfig;
    ///
    /// let mut config_map = toml::map::Map::new();
    /// config_map.insert("style_guide".to_string(), toml::Value::String("docs/STYLE.md".into()));
    /// let config = ProjectConfig::from(config_map);
    ///
    /// let json = config.to_json_value();
    /// // Use json in Tera template context
    /// ```
    pub fn to_json_value(&self) -> serde_json::Value {
        toml_value_to_json(&toml::Value::Table(self.0.clone()))
    }
}

impl From<toml::map::Map<String, toml::Value>> for ProjectConfig {
    fn from(map: toml::map::Map<String, toml::Value>) -> Self {
        Self(map)
    }
}

/// Convert a toml::Value to serde_json::Value.
pub(crate) fn toml_value_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(table) => {
            // Sort keys to ensure deterministic JSON serialization
            let mut keys: Vec<_> = table.keys().collect();
            keys.sort();
            let map: serde_json::Map<String, serde_json::Value> =
                keys.into_iter().map(|k| (k.clone(), toml_value_to_json(&table[k]))).collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Convert JSON value to TOML value for template variable merging.
///
/// Handles JSON null as empty string since TOML lacks a null type.
/// Used when merging template_vars (JSON) with project config (TOML).
#[cfg(test)]
pub(crate) fn json_value_to_toml(value: &serde_json::Value) -> toml::Value {
    match value {
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                // Fallback for numbers that don't fit i64 or f64
                toml::Value::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Null => {
            // TOML doesn't have a null type - represent as empty string
            toml::Value::String(String::new())
        }
        serde_json::Value::Array(arr) => {
            toml::Value::Array(arr.iter().map(json_value_to_toml).collect())
        }
        serde_json::Value::Object(obj) => {
            let table: toml::map::Map<String, toml::Value> =
                obj.iter().map(|(k, v)| (k.clone(), json_value_to_toml(v))).collect();
            toml::Value::Table(table)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Named source repositories mapped to their Git URLs.
    ///
    /// Keys are short, convenient names used in dependency specifications.
    /// Values are Git repository URLs (HTTPS, SSH, or local file:// URLs).
    ///
    /// **Security Note**: Never include authentication tokens in these URLs.
    /// Use SSH keys or configure authentication in the global config file.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [sources]
    /// official = "https://github.com/claude-org/official.git"
    /// private = "git@github.com:company/private.git"
    /// local = "file:///home/user/local-repo"
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sources: HashMap<String, String>,

    /// Tool type configurations for multi-tool support.
    ///
    /// Maps tool type names (claude-code, opencode, agpm, custom) to their
    /// installation configurations. This replaces the old `target` field and
    /// enables support for multiple tools and custom tool types.
    ///
    /// See [`ToolsConfig`] for details on configuration format.
    #[serde(rename = "tools", skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsConfig>,

    /// Agent dependencies mapping names to their specifications.
    ///
    /// Agents are typically AI model definitions, prompts, or behavioral
    /// specifications stored as Markdown files. Each dependency can be
    /// either local (filesystem path) or remote (from a Git source).
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agents: HashMap<String, ResourceDependency>,

    /// Snippet dependencies mapping names to their specifications.
    ///
    /// Snippets are typically reusable code templates, examples, or
    /// documentation stored as Markdown files. They follow the same
    /// dependency format as agents.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub snippets: HashMap<String, ResourceDependency>,

    /// Command dependencies mapping names to their specifications.
    ///
    /// Commands are Claude Code slash commands that provide custom functionality
    /// and automation within the Claude Code interface. They follow the same
    /// dependency format as agents and snippets.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub commands: HashMap<String, ResourceDependency>,

    /// MCP server configurations mapping names to their specifications.
    ///
    /// MCP servers provide integrations with external systems and services,
    /// allowing Claude Code to connect to databases, APIs, and other tools.
    /// MCP servers are JSON configuration files that get installed to
    /// `.mcp.json` (no separate directory - configurations are merged into the JSON file).
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "mcp-servers")]
    pub mcp_servers: HashMap<String, ResourceDependency>,

    /// Script dependencies mapping names to their specifications.
    ///
    /// Scripts are executable files (.sh, .js, .py, etc.) that can be run by hooks
    /// or independently. They are installed to `.claude/scripts/` and can be
    /// referenced by hook configurations.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scripts: HashMap<String, ResourceDependency>,

    /// Hook dependencies mapping names to their specifications.
    ///
    /// Hooks are JSON configuration files that define event-based automation
    /// in Claude Code. They specify when to run scripts based on tool usage,
    /// prompts, and other events. Hook configurations are merged into
    /// `settings.local.json`.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hooks: HashMap<String, ResourceDependency>,
    /// Skill dependencies mapping names to their specifications.
    ///
    /// Skills are directory-based resources containing a SKILL.md file with
    /// frontmatter and optional supporting files. They enable packaging expertise,
    /// procedures, and resources that Claude can automatically invoke based on context.
    /// Skills are installed to `.claude/skills/<name>/` as directories.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skills: HashMap<String, ResourceDependency>,

    /// Patches for overriding resource metadata.
    ///
    /// Patches allow overriding YAML frontmatter fields (like `model`) in
    /// resources without forking upstream repositories. They are keyed by
    /// resource type and manifest alias.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [patch.agents.my-agent]
    /// model = "claude-3-haiku"
    /// temperature = "0.7"
    /// ```
    #[serde(default, skip_serializing_if = "ManifestPatches::is_empty", rename = "patch")]
    pub patches: ManifestPatches,

    /// Project-level patches (from agpm.toml).
    ///
    /// This field is not serialized - it's populated during loading to track
    /// which patches came from the project manifest vs private config.
    #[serde(skip)]
    pub project_patches: ManifestPatches,

    /// Private patches (from agpm.private.toml).
    ///
    /// This field is not serialized - it's populated during loading to track
    /// which patches came from private config. These are kept separate from
    /// project patches to maintain deterministic lockfiles.
    #[serde(skip)]
    pub private_patches: ManifestPatches,

    /// Default tool overrides for resource types.
    ///
    /// Allows users to override which tool is used by default when a dependency
    /// doesn't explicitly specify a tool. Keys are resource type names (agents,
    /// snippets, commands, scripts, hooks, mcp-servers), values are tool names
    /// (claude-code, opencode, agpm, or custom tool names).
    ///
    /// # Examples
    ///
    /// ```toml
    /// [default-tools]
    /// snippets = "claude-code"  # Override default for Claude-only users
    /// agents = "claude-code"    # Explicit (already the default)
    /// commands = "opencode"     # Use OpenCode by default for commands
    /// ```
    ///
    /// # Built-in Defaults (when not configured)
    ///
    /// - `snippets` → `"agpm"` (shared infrastructure)
    /// - All other resource types → `"claude-code"`
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "default-tools")]
    pub default_tools: HashMap<String, String>,

    /// Project-specific template variables.
    ///
    /// Custom project configuration that can be referenced in resource templates
    /// via Tera template syntax. This allows teams to define project-specific
    /// values like paths, standards, and conventions that are then available
    /// throughout all installed resources.
    ///
    /// Template access: `{{ agpm.project.name }}`, `{{ agpm.project.paths.style_guide }}`
    ///
    /// # Examples
    ///
    /// ```toml
    /// [project]
    /// name = "My Project"
    /// version = "2.0.0"
    ///
    /// [project.paths]
    /// style_guide = "docs/STYLE_GUIDE.md"
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,

    /// Directory containing the manifest file (for resolving relative paths).
    ///
    /// This field is populated when loading the manifest and is used to resolve
    /// relative paths in dependencies, particularly for path-only dependencies
    /// and their transitive dependencies.
    ///
    /// This field is not serialized and only exists at runtime.
    #[serde(skip)]
    pub manifest_dir: Option<std::path::PathBuf>,
}

/// A resource dependency specification supporting multiple formats.
///
/// Dependencies can be specified in two main formats to balance simplicity
/// with flexibility. The enum uses Serde's `untagged` attribute to automatically
/// deserialize the correct variant based on the TOML structure.
///
/// # Variants
///
/// ## Simple Dependencies
///
/// For local file dependencies, just specify the path directly:
///
/// ```toml
/// [agents]
/// local-helper = "../shared/agents/helper.md"
/// nearby-agent = "./local/custom-agent.md"
/// ```
///
/// ## Detailed Dependencies
///
/// For remote dependencies or when you need more control:
///
/// ```toml
/// [agents]
/// # Remote dependency with version
/// code-reviewer = { source = "official", path = "agents/reviewer.md", version = "v1.0.0" }
///
/// # Remote dependency with git reference
/// experimental = { source = "community", path = "agents/new.md", git = "develop" }
///
/// # Local dependency with explicit path (equivalent to simple form)
/// local-tool = { path = "../tools/agent.md" }
/// ```
///
/// # Validation Rules
///
/// - **Local dependencies** (no source): Cannot have version constraints
/// - **Remote dependencies** (with source): Must have either `version` or `git` field
/// - **Path field**: Required and cannot be empty
/// - **Source field**: Must reference an existing source in the `[sources]` section
///
/// # Type Safety
///
/// The enum ensures type safety at compile time while providing runtime
/// validation through the [`Manifest::validate`] method.
///
/// # Serialization Behavior
///
/// - Simple paths serialize directly as strings
/// - Detailed specs serialize as TOML inline tables
/// - Empty optional fields are omitted for cleaner output
/// - Deserialization is automatic based on TOML structure
///
/// # Memory Layout
///
/// This enum uses `#[serde(untagged)]` for automatic variant detection,
/// which means deserialization tries the `Detailed` variant first, then
/// falls back to `Simple`. This is efficient for the expected usage patterns
/// where detailed dependencies are more common in larger projects.
impl Manifest {
    /// Create a new empty manifest with default configuration.
    ///
    /// The new manifest will have:
    /// - No sources defined
    /// - Default target directories (`.claude/agents` and `.agpm/snippets`)
    /// - No dependencies
    ///
    /// This is typically used when programmatically building a manifest or
    /// as a starting point for adding dependencies.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// assert!(manifest.sources.is_empty());
    /// assert!(manifest.agents.is_empty());
    /// assert!(manifest.snippets.is_empty());
    /// assert!(manifest.commands.is_empty());
    /// assert!(manifest.mcp_servers.is_empty());
    /// ```
    #[must_use]
    #[allow(deprecated)]
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            tools: None,
            agents: HashMap::new(),
            snippets: HashMap::new(),
            commands: HashMap::new(),
            mcp_servers: HashMap::new(),
            scripts: HashMap::new(),
            hooks: HashMap::new(),
            skills: HashMap::new(),
            patches: ManifestPatches::new(),
            project_patches: ManifestPatches::new(),
            private_patches: ManifestPatches::new(),
            default_tools: HashMap::new(),
            project: None,
            manifest_dir: None,
        }
    }

    /// Load and parse a manifest from a TOML file.
    ///
    /// This method reads the specified file, parses it as TOML, deserializes
    /// it into a [`Manifest`] struct, and validates the result. The entire
    /// operation is atomic - either the manifest loads successfully or an
    /// error is returned.
    ///
    /// # Validation
    ///
    /// After parsing, the manifest is automatically validated to ensure:
    /// - All dependency sources reference valid entries in the `[sources]` section
    /// - Required fields are present and non-empty
    /// - Version constraints are properly specified for remote dependencies
    /// - Source URLs use supported protocols
    /// - No version conflicts exist between dependencies
    ///
    /// # Error Handling
    ///
    /// Returns detailed errors for common problems:
    /// - **File I/O errors**: File not found, permission denied, etc.
    /// - **TOML syntax errors**: Invalid TOML format with helpful suggestions
    /// - **Validation errors**: Logical inconsistencies in the manifest
    /// - **Security errors**: Unsafe URL patterns or credential leakage
    ///
    /// All errors include contextual information and actionable suggestions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// // Load a manifest file
    /// let manifest = Manifest::load(Path::new("agpm.toml"))?;
    ///
    /// // Access parsed data
    /// println!("Found {} sources", manifest.sources.len());
    /// println!("Found {} agents", manifest.agents.len());
    /// println!("Found {} snippets", manifest.snippets.len());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # File Format
    ///
    /// Expects a valid TOML file following the AGPM manifest format.
    /// See the module-level documentation for complete format specification.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_file_context(
            FileOperation::Read,
            path,
            "reading manifest file",
            "manifest_module",
        )?;

        let mut manifest: Self = toml::from_str(&content)
            .map_err(|e| crate::core::AgpmError::ManifestParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in manifest file: {}\n\n\
                    Common TOML syntax errors:\n\
                    - Missing quotes around strings\n\
                    - Unmatched brackets [ ] or braces {{ }}\n\
                    - Invalid characters in keys or values\n\
                    - Incorrect indentation or structure",
                    path.display()
                )
            })?;

        // Apply resource-type-specific defaults for tool
        // Snippets default to "agpm" (shared infrastructure) instead of "claude-code"
        manifest.apply_tool_defaults();

        // Store the manifest directory for resolving relative paths
        manifest.manifest_dir = Some(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Manifest path has no parent directory"))?
                .to_path_buf(),
        );

        manifest.validate()?;

        Ok(manifest)
    }

    /// Load manifest with private config merged.
    ///
    /// Loads the project manifest from `agpm.toml` and then attempts to load
    /// `agpm.private.toml` from the same directory. If a private config exists,
    /// its patches are merged with the project patches (private silently takes precedence).
    ///
    /// Any conflicts (same field defined in both files with different values) are
    /// returned for informational purposes only. Private patches always override
    /// project patches without raising an error.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the project manifest file (`agpm.toml`)
    ///
    /// # Returns
    ///
    /// A manifest with merged patches and a list of any conflicts detected (for
    /// informational/debugging purposes).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// let (manifest, conflicts) = Manifest::load_with_private(Path::new("agpm.toml"))?;
    /// // Conflicts are informational only - private patches already won
    /// if !conflicts.is_empty() {
    ///     eprintln!("Note: {} private patch(es) override project settings", conflicts.len());
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load_with_private(path: &Path) -> Result<(Self, Vec<PatchConflict>)> {
        // Load the main project manifest
        let mut manifest = Self::load(path)?;

        // Store project patches before merging
        manifest.project_patches = manifest.patches.clone();

        // Try to load private config
        let private_path = if let Some(parent) = path.parent() {
            parent.join("agpm.private.toml")
        } else {
            PathBuf::from("agpm.private.toml")
        };

        if private_path.exists() {
            let private_manifest = Self::load_private(&private_path)?;

            // Store private patches
            manifest.private_patches = private_manifest.patches.clone();

            // Merge patches (private takes precedence)
            let (merged_patches, conflicts) =
                manifest.patches.merge_with(&private_manifest.patches);
            manifest.patches = merged_patches;

            Ok((manifest, conflicts))
        } else {
            // No private config, keep private_patches empty
            manifest.private_patches = ManifestPatches::new();
            Ok((manifest, Vec::new()))
        }
    }

    /// Load a private manifest file.
    ///
    /// Private manifests can only contain patches - they cannot define sources,
    /// tools, or dependencies. This method loads and validates that the private
    /// config follows these rules.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the private manifest file (`agpm.private.toml`)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The TOML syntax is invalid
    /// - The private config contains non-patch fields
    fn load_private(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_file_context(
            FileOperation::Read,
            path,
            "reading private manifest file",
            "manifest_module",
        )?;

        let manifest: Self = toml::from_str(&content)
            .map_err(|e| crate::core::AgpmError::ManifestParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in private manifest file: {}\n\n\
                    Common TOML syntax errors:\n\
                    - Missing quotes around strings\n\
                    - Unmatched brackets [ ] or braces {{ }}\n\
                    - Invalid characters in keys or values\n\
                    - Incorrect indentation or structure",
                    path.display()
                )
            })?;

        // Validate that private config only contains patches
        if !manifest.sources.is_empty()
            || manifest.tools.is_some()
            || !manifest.agents.is_empty()
            || !manifest.snippets.is_empty()
            || !manifest.commands.is_empty()
            || !manifest.mcp_servers.is_empty()
            || !manifest.scripts.is_empty()
            || !manifest.hooks.is_empty()
        {
            anyhow::bail!(
                "Private manifest file ({}) can only contain [patch] sections, not sources, tools, or dependencies",
                path.display()
            );
        }

        Ok(manifest)
    }

    /// Get the default tool for a resource type.
    ///
    /// Checks the `[default-tools]` configuration first, then falls back to
    /// the built-in defaults:
    /// - `snippets` → `"agpm"` (shared infrastructure)
    /// - All other resource types → `"claude-code"`
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type to get the default tool for
    ///
    /// # Returns
    ///
    /// The default tool name as a string.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    /// assert_eq!(manifest.get_default_tool(ResourceType::Snippet), "agpm");
    /// assert_eq!(manifest.get_default_tool(ResourceType::Agent), "claude-code");
    /// ```
    #[must_use]
    pub fn get_default_tool(&self, resource_type: crate::core::ResourceType) -> String {
        // Get the resource name in plural form for consistency with TOML section names
        // (agents, snippets, commands, etc.)
        let resource_name = match resource_type {
            crate::core::ResourceType::Agent => "agents",
            crate::core::ResourceType::Snippet => "snippets",
            crate::core::ResourceType::Command => "commands",
            crate::core::ResourceType::Script => "scripts",
            crate::core::ResourceType::Hook => "hooks",
            crate::core::ResourceType::McpServer => "mcp-servers",
            crate::core::ResourceType::Skill => "skills",
        };

        // Check if there's a configured override
        if let Some(tool) = self.default_tools.get(resource_name) {
            return tool.clone();
        }

        // Fall back to built-in defaults
        resource_type.default_tool().to_string()
    }

    fn apply_tool_defaults(&mut self) {
        // Apply resource-type-specific defaults only when tool is not explicitly specified
        for resource_type in [
            crate::core::ResourceType::Snippet,
            crate::core::ResourceType::Agent,
            crate::core::ResourceType::Command,
            crate::core::ResourceType::Script,
            crate::core::ResourceType::Hook,
            crate::core::ResourceType::McpServer,
        ] {
            // Get the default tool before the mutable borrow to avoid borrow conflicts
            let default_tool = self.get_default_tool(resource_type);

            if let Some(deps) = self.get_dependencies_mut(resource_type) {
                for dependency in deps.values_mut() {
                    if let ResourceDependency::Detailed(details) = dependency {
                        if details.tool.is_none() {
                            details.tool = Some(default_tool.clone());
                        }
                    }
                }
            }
        }
    }

    /// Save the manifest to a TOML file with pretty formatting.
    ///
    /// This method serializes the manifest to TOML format and writes it to the
    /// specified file path. The output is pretty-printed for human readability
    /// and follows TOML best practices.
    ///
    /// # Formatting
    ///
    /// The generated TOML file will:
    /// - Use consistent indentation and spacing
    /// - Omit empty sections for cleaner output
    /// - Order sections logically (sources, target, agents, snippets)
    /// - Include inline tables for detailed dependencies
    ///
    /// # Atomic Operation
    ///
    /// The save operation is atomic - the file is either completely written
    /// or left unchanged. This prevents corruption if the operation fails
    /// partway through.
    ///
    /// # Error Handling
    ///
    /// Returns detailed errors for common problems:
    /// - **Permission denied**: Insufficient write permissions
    /// - **Directory doesn't exist**: Parent directory missing  
    /// - **Disk full**: Insufficient storage space
    /// - **File locked**: Another process has the file open
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_source(
    ///     "official".to_string(),
    ///     "https://github.com/claude-org/resources.git".to_string()
    /// );
    ///
    /// // Save to file
    /// # use tempfile::tempdir;
    /// # let temp_dir = tempdir()?;
    /// # let manifest_path = temp_dir.path().join("agpm.toml");
    /// manifest.save(&manifest_path)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Output Format
    ///
    /// The generated file will follow this structure:
    ///
    /// ```toml
    /// [sources]
    /// official = "https://github.com/claude-org/resources.git"
    ///
    /// [target]
    /// agents = ".claude/agents"
    /// snippets = ".agpm/snippets"
    ///
    /// [agents]
    /// helper = { source = "official", path = "agents/helper.md", version = "v1.0.0" }
    ///
    /// [snippets]
    /// utils = { source = "official", path = "snippets/utils.md", version = "v1.0.0" }
    /// ```
    pub fn save(&self, path: &Path) -> Result<()> {
        // Serialize to a document first so we can control formatting
        let mut doc = toml_edit::ser::to_document(self)
            .with_context(|| "Failed to serialize manifest data to TOML format")?;

        // Convert top-level inline tables to regular tables (section headers)
        // This keeps [sources], [agents], etc. as sections but nested values stay inline
        for (_key, value) in doc.iter_mut() {
            if let Some(inline_table) = value.as_inline_table() {
                // Convert inline table to regular table
                let table = inline_table.clone().into_table();
                *value = toml_edit::Item::Table(table);
            }
        }

        let content = doc.to_string();

        std::fs::write(path, content).with_file_context(
            FileOperation::Write,
            path,
            "writing manifest file",
            "manifest_module",
        )?;

        Ok(())
    }

    /// Validate the manifest structure and enforce business rules.
    ///
    /// This method performs comprehensive validation of the manifest to ensure
    /// logical consistency, security best practices, and correct dependency
    /// relationships. It's automatically called during [`Self::load`] but can
    /// also be used independently to validate programmatically constructed manifests.
    ///
    /// # Validation Rules
    ///
    /// ## Source Validation
    /// - All source URLs must use supported protocols (HTTPS, SSH, git://, file://)
    /// - No plain directory paths allowed as sources (must use file:// URLs)
    /// - No authentication tokens embedded in URLs (security check)
    /// - Environment variable expansion is validated for syntax
    ///
    /// ## Dependency Validation  
    /// - All dependency paths must be non-empty
    /// - Remote dependencies must reference existing sources
    /// - Remote dependencies must specify version constraints
    /// - Local dependencies cannot have version constraints
    /// - No version conflicts between dependencies with the same name
    ///
    /// ## Path Validation
    /// - Local dependency paths are checked for proper format
    /// - Remote dependency paths are validated as repository-relative
    /// - Path traversal attempts are detected and rejected
    ///
    /// # Error Types
    ///
    /// Returns specific error types for different validation failures:
    /// - [`crate::core::AgpmError::SourceNotFound`]: Referenced source doesn't exist
    /// - [`crate::core::AgpmError::ManifestValidationError`]: General validation failures
    /// - Context errors for specific issues with actionable suggestions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency, DetailedDependency};
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // This will pass validation (local dependency)
    /// manifest.add_dependency(
    ///     "local".to_string(),
    ///     ResourceDependency::Simple("../local/helper.md".to_string()),
    ///     true
    /// );
    /// assert!(manifest.validate().is_ok());
    ///
    /// // This will fail validation (missing source)
    /// manifest.add_dependency(
    ///     "remote".to_string(),
    ///     ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///         source: Some("missing".to_string()),
    ///         path: "agent.md".to_string(),
    ///         version: Some("v1.0.0".to_string()),
    ///         branch: None,
    ///         rev: None,
    ///         command: None,
    ///         args: None,
    ///         target: None,
    ///         filename: None,
    ///         dependencies: None,
    ///         tool: Some("claude-code".to_string()),
    ///         flatten: None,
    ///         install: None,
    ///         template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    ///     })),
    ///     true
    /// );
    /// assert!(manifest.validate().is_err());
    /// ```
    ///
    /// # Security Considerations
    ///
    /// This method enforces critical security rules:
    /// - Prevents credential leakage in version-controlled files
    /// - Blocks path traversal attacks in local dependencies
    /// - Validates URL schemes to prevent protocol confusion
    /// - Checks for malicious patterns in dependency specifications
    ///
    /// # Performance
    ///
    /// Validation is designed to be fast and is safe to call frequently.
    /// Complex validations (like network connectivity) are not performed
    /// here - those are handled during dependency resolution.
    pub fn validate(&self) -> Result<()> {
        // Validate artifact type names
        for artifact_type in self.get_tools_config().types.keys() {
            if artifact_type.contains('/') || artifact_type.contains('\\') {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Artifact type name '{artifact_type}' cannot contain path separators ('/' or '\\\\'). \n\
                        Artifact type names must be simple identifiers without special characters."
                    ),
                }
                .into());
            }

            // Also check for other potentially problematic characters
            if artifact_type.contains("..") {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Artifact type name '{artifact_type}' cannot contain '..' (path traversal). \n\
                        Artifact type names must be simple identifiers."
                    ),
                }
                .into());
            }
        }

        // Check that all referenced sources exist and dependencies have required fields
        for (name, dep) in self.all_dependencies() {
            // Check for empty path
            if dep.get_path().is_empty() {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!("Missing required field 'path' for dependency '{name}'"),
                }
                .into());
            }

            // Validate pattern safety if it's a pattern dependency
            if dep.is_pattern() {
                crate::pattern::validate_pattern_safety(dep.get_path()).map_err(|e| {
                    crate::core::AgpmError::ManifestValidationError {
                        reason: format!("Invalid pattern in dependency '{name}': {e}"),
                    }
                })?;
            }

            // Check for version when source is specified (non-local dependencies)
            if let Some(source) = dep.get_source() {
                if !self.sources.contains_key(source) {
                    return Err(crate::core::AgpmError::SourceNotFound {
                        name: source.to_string(),
                    }
                    .into());
                }

                // Check if the source URL is a local path
                let source_url = self.sources.get(source).unwrap();
                let _is_local_source = source_url.starts_with('/')
                    || source_url.starts_with("./")
                    || source_url.starts_with("../");

                // Git dependencies can optionally have a version (defaults to 'main' if not specified)
                // Local path sources don't need versions
                // We no longer require versions for Git dependencies - they'll default to 'main'
            } else {
                // For local path dependencies (no source), version is not allowed
                // Skip directory check for pattern dependencies
                if !dep.is_pattern() {
                    let path = dep.get_path();
                    let is_plain_dir =
                        path.starts_with('/') || path.starts_with("./") || path.starts_with("../");

                    if is_plain_dir && dep.get_version().is_some() {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Version specified for plain directory dependency '{name}' with path '{path}'. \n\
                                Plain directory dependencies do not support versions. \n\
                            Remove the 'version' field or use a git source instead."
                            ),
                        }
                        .into());
                    }
                }
            }
        }

        // Check for version conflicts (same dependency name with different versions)
        let mut seen_deps: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, dep) in self.all_dependencies() {
            if let Some(version) = dep.get_version() {
                if let Some(existing_version) = seen_deps.get(name) {
                    if existing_version != version {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Version conflict for dependency '{name}': found versions '{existing_version}' and '{version}'"
                            ),
                        }
                        .into());
                    }
                } else {
                    seen_deps.insert(name.to_string(), version.to_string());
                }
            }
        }

        // Validate URLs in sources
        for (name, url) in &self.sources {
            // Expand environment variables and home directory in URL
            let expanded_url = expand_url(url)?;

            if !expanded_url.starts_with("http://")
                && !expanded_url.starts_with("https://")
                && !expanded_url.starts_with("git@")
                && !expanded_url.starts_with("file://")
            // Plain directory paths not allowed as sources
            && !expanded_url.starts_with('/')
            && !expanded_url.starts_with("./")
            && !expanded_url.starts_with("../")
            {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!("Source '{name}' has invalid URL: '{url}'. Must be HTTP(S), SSH (git@...), or file:// URL"),
                }
                .into());
            }

            // Check if plain directory path is used as a source
            if expanded_url.starts_with('/')
                || expanded_url.starts_with("./")
                || expanded_url.starts_with("../")
            {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Plain directory path '{url}' cannot be used as source '{name}'. \n\
                        Sources must be git repositories. Use one of:\n\
                        - Remote URL: https://github.com/owner/repo.git\n\
                        - Local git repo: file:///absolute/path/to/repo\n\
                        - Or use direct path dependencies without a source"
                    ),
                }
                .into());
            }
        }

        // Check for case-insensitive conflicts on all platforms
        // This ensures manifests are portable across different filesystems
        // Even though Linux supports case-sensitive files, we reject conflicts
        // to ensure the manifest works on Windows and macOS too
        let mut normalized_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (name, _) in self.all_dependencies() {
            let normalized = name.to_lowercase();
            if !normalized_names.insert(normalized.clone()) {
                // Find the original conflicting name
                for (other_name, _) in self.all_dependencies() {
                    if other_name != name && other_name.to_lowercase() == normalized {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Case conflict: '{name}' and '{other_name}' would map to the same file on case-insensitive filesystems. To ensure portability across platforms, resource names must be case-insensitively unique."
                            ),
                        }
                        .into());
                    }
                }
            }
        }

        // Validate artifact types and resource type support
        for resource_type in crate::core::ResourceType::all() {
            if let Some(deps) = self.get_dependencies(*resource_type) {
                for (name, dep) in deps {
                    // Get tool from dependency (defaults based on resource type)
                    let tool_string = dep
                        .get_tool()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| self.get_default_tool(*resource_type));
                    let tool = tool_string.as_str();

                    // Check if tool is configured
                    if self.get_tool_config(tool).is_none() {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Unknown tool '{tool}' for dependency '{name}'.\n\
                                Available types: {}\n\
                                Configure custom types in [tools] section or use a standard type.",
                                self.get_tools_config()
                                    .types
                                    .keys()
                                    .map(|s| format!("'{s}'"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        }
                        .into());
                    }

                    // Check if resource type is supported by this tool
                    if !self.is_resource_supported(tool, *resource_type) {
                        let artifact_config = self.get_tool_config(tool).unwrap();
                        let resource_plural = resource_type.to_plural();

                        // Check if this is a malformed configuration (resource exists but not properly configured)
                        let is_malformed = artifact_config.resources.contains_key(resource_plural);

                        let supported_types: Vec<String> = artifact_config
                            .resources
                            .iter()
                            .filter(|(_, res_config)| {
                                res_config.path.is_some() || res_config.merge_target.is_some()
                            })
                            .map(|(s, _)| s.to_string())
                            .collect();

                        // Build resource-type-specific suggestions
                        let mut suggestions = Vec::new();

                        if is_malformed {
                            // Resource type exists but is malformed
                            suggestions.push(format!(
                                "Resource type '{}' is configured for tool '{}' but missing required 'path' or 'merge_target' field",
                                resource_plural, tool
                            ));

                            // Provide specific fix suggestions based on resource type
                            match resource_type {
                                crate::core::ResourceType::Hook => {
                                    suggestions.push("For hooks, add: merge_target = '.claude/settings.local.json'".to_string());
                                }
                                crate::core::ResourceType::McpServer => {
                                    suggestions.push(
                                        "For MCP servers, add: merge_target = '.mcp.json'"
                                            .to_string(),
                                    );
                                }
                                _ => {
                                    suggestions.push(format!(
                                        "For {}, add: path = '{}'",
                                        resource_plural, resource_plural
                                    ));
                                }
                            }
                        } else {
                            // Resource type not supported at all
                            match resource_type {
                                crate::core::ResourceType::Snippet => {
                                    suggestions.push("Snippets work best with the 'agpm' tool (shared infrastructure)".to_string());
                                    suggestions.push(
                                        "Add tool='agpm' to this dependency to use shared snippets"
                                            .to_string(),
                                    );
                                }
                                _ => {
                                    // Find which tool types DO support this resource type
                                    let default_config = ToolsConfig::default();
                                    let tools_config =
                                        self.tools.as_ref().unwrap_or(&default_config);
                                    let supporting_types: Vec<String> = tools_config
                                        .types
                                        .iter()
                                        .filter(|(_, config)| {
                                            config.resources.contains_key(resource_plural)
                                                && config
                                                    .resources
                                                    .get(resource_plural)
                                                    .map(|res| {
                                                        res.path.is_some()
                                                            || res.merge_target.is_some()
                                                    })
                                                    .unwrap_or(false)
                                        })
                                        .map(|(type_name, _)| format!("'{}'", type_name))
                                        .collect();

                                    if !supporting_types.is_empty() {
                                        suggestions.push(format!(
                                            "This resource type is supported by tools: {}",
                                            supporting_types.join(", ")
                                        ));
                                    }
                                }
                            }
                        }

                        let mut reason = if is_malformed {
                            format!(
                                "Resource type '{}' is improperly configured for tool '{}' for dependency '{}'.\n\n",
                                resource_plural, tool, name
                            )
                        } else {
                            format!(
                                "Resource type '{}' is not supported by tool '{}' for dependency '{}'.\n\n",
                                resource_plural, tool, name
                            )
                        };

                        reason.push_str(&format!(
                            "Tool '{}' properly supports: {}\n\n",
                            tool,
                            supported_types.join(", ")
                        ));

                        if !suggestions.is_empty() {
                            reason.push_str("💡 Suggestions:\n");
                            for suggestion in &suggestions {
                                reason.push_str(&format!("  • {}\n", suggestion));
                            }
                            reason.push('\n');
                        }

                        reason.push_str(
                            "You can fix this by:\n\
                            1. Changing the 'tool' field to a supported tool\n\
                            2. Using a different resource type\n\
                            3. Removing this dependency from your manifest",
                        );

                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason,
                        }
                        .into());
                    }
                }
            }
        }

        // Validate patches reference valid aliases
        self.validate_patches()?;

        Ok(())
    }

    /// Validate that patches reference valid manifest aliases.
    ///
    /// This method checks that all patch aliases correspond to actual dependencies
    /// defined in the manifest. Patches for non-existent aliases are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error if a patch references an alias that doesn't exist in the manifest.
    fn validate_patches(&self) -> Result<()> {
        use crate::core::ResourceType;

        // Helper to check if an alias exists for a resource type
        let check_patch_aliases = |resource_type: ResourceType,
                                   patches: &BTreeMap<String, PatchData>|
         -> Result<()> {
            let deps = self.get_dependencies(resource_type);

            for alias in patches.keys() {
                // Check if this alias exists in the manifest
                let exists = if let Some(deps) = deps {
                    deps.contains_key(alias)
                } else {
                    false
                };

                if !exists {
                    return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Patch references unknown alias '{alias}' in [patch.{}] section.\n\
                                The alias must be defined in [{}] section of agpm.toml.\n\
                                To patch a transitive dependency, first add it explicitly to your manifest.",
                                resource_type.to_plural(),
                                resource_type.to_plural()
                            ),
                        }
                        .into());
                }
            }
            Ok(())
        };

        // Validate patches for each resource type
        check_patch_aliases(ResourceType::Agent, &self.patches.agents)?;
        check_patch_aliases(ResourceType::Snippet, &self.patches.snippets)?;
        check_patch_aliases(ResourceType::Command, &self.patches.commands)?;
        check_patch_aliases(ResourceType::Script, &self.patches.scripts)?;
        check_patch_aliases(ResourceType::McpServer, &self.patches.mcp_servers)?;
        check_patch_aliases(ResourceType::Hook, &self.patches.hooks)?;

        Ok(())
    }

    /// Get all dependencies from both agents and snippets sections.
    ///
    /// Returns a vector of tuples containing dependency names and their
    /// specifications. This is useful for iteration over all dependencies
    /// without needing to handle agents and snippets separately.
    ///
    /// # Return Value
    ///
    /// Each tuple contains:
    /// - `&str`: The dependency name (key from TOML)
    /// - `&ResourceDependency`: The dependency specification
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// // ... add some dependencies
    ///
    /// for (name, dep) in manifest.all_dependencies() {
    ///     println!("Dependency: {} -> {}", name, dep.get_path());
    ///     if let Some(source) = dep.get_source() {
    ///         println!("  Source: {}", source);
    ///     }
    /// }
    /// ```
    ///
    /// # Order
    ///
    /// Dependencies are returned in the order they appear in the underlying
    /// `HashMaps` (agents first, then snippets, then commands), which means the order is not
    /// guaranteed to be stable across runs.
    /// Get dependencies for a specific resource type
    ///
    /// Returns the `HashMap` of dependencies for the specified resource type.
    /// Note: MCP servers return None as they use a different dependency type.
    pub const fn get_dependencies(
        &self,
        resource_type: crate::core::ResourceType,
    ) -> Option<&HashMap<String, ResourceDependency>> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => Some(&self.agents),
            ResourceType::Snippet => Some(&self.snippets),
            ResourceType::Command => Some(&self.commands),
            ResourceType::Script => Some(&self.scripts),
            ResourceType::Hook => Some(&self.hooks),
            ResourceType::McpServer => Some(&self.mcp_servers),
            ResourceType::Skill => Some(&self.skills),
        }
    }

    /// Get mutable dependencies for a specific resource type
    ///
    /// Returns a mutable reference to the `HashMap` of dependencies for the specified resource type.
    #[must_use]
    pub fn get_dependencies_mut(
        &mut self,
        resource_type: crate::core::ResourceType,
    ) -> Option<&mut HashMap<String, ResourceDependency>> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => Some(&mut self.agents),
            ResourceType::Snippet => Some(&mut self.snippets),
            ResourceType::Command => Some(&mut self.commands),
            ResourceType::Script => Some(&mut self.scripts),
            ResourceType::Hook => Some(&mut self.hooks),
            ResourceType::McpServer => Some(&mut self.mcp_servers),
            ResourceType::Skill => Some(&mut self.skills),
        }
    }

    /// Get the tools configuration, returning default if not specified.
    ///
    /// This method provides access to the tool configurations which define
    /// where resources are installed for different tools (claude-code, opencode, agpm).
    ///
    /// Returns the configured tools or the default configuration if not specified.
    pub fn get_tools_config(&self) -> &ToolsConfig {
        self.tools.as_ref().unwrap_or_else(|| {
            // Return a static default - this is safe because ToolsConfig::default() is deterministic
            static DEFAULT: std::sync::OnceLock<ToolsConfig> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(ToolsConfig::default)
        })
    }

    /// Get configuration for a specific tool type.
    ///
    /// Returns None if the tool is not configured.
    pub fn get_tool_config(&self, tool: &str) -> Option<&ArtifactTypeConfig> {
        self.get_tools_config().types.get(tool)
    }

    /// Get the installation path for a resource within a tool.
    ///
    /// Returns the full installation directory path by combining:
    /// - Tool's base directory (e.g., ".claude", ".opencode")
    /// - Resource type's subdirectory (e.g., "agents", "command")
    ///
    /// Returns None if:
    /// - The tool is not configured
    /// - The resource type is not supported by this tool
    /// - The resource has no configured path (special handling like MCP merge)
    pub fn get_artifact_resource_path(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> Option<std::path::PathBuf> {
        let artifact_config = self.get_tool_config(tool)?;
        let resource_config = artifact_config.resources.get(resource_type.to_plural())?;

        resource_config.path.as_ref().map(|subdir| artifact_config.path.join(subdir))
    }

    /// Get the merge target configuration file path for a resource type.
    ///
    /// Returns the path to the configuration file where resources of this type
    /// should be merged (e.g., hooks, MCP servers). Returns None if the resource
    /// type doesn't use merge targets or if the tool doesn't support this resource type.
    ///
    /// # Arguments
    ///
    /// * `tool` - The tool name (e.g., "claude-code", "opencode")
    /// * `resource_type` - The resource type to look up
    ///
    /// # Returns
    ///
    /// The merge target path if configured, otherwise None.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    ///
    /// // Hooks merge into .claude/settings.local.json
    /// let hook_target = manifest.get_merge_target("claude-code", ResourceType::Hook);
    /// assert_eq!(hook_target, Some(".claude/settings.local.json".into()));
    ///
    /// // MCP servers merge into .mcp.json for claude-code
    /// let mcp_target = manifest.get_merge_target("claude-code", ResourceType::McpServer);
    /// assert_eq!(mcp_target, Some(".mcp.json".into()));
    ///
    /// // MCP servers merge into .opencode/opencode.json for opencode
    /// let opencode_mcp = manifest.get_merge_target("opencode", ResourceType::McpServer);
    /// assert_eq!(opencode_mcp, Some(".opencode/opencode.json".into()));
    /// ```
    pub fn get_merge_target(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> Option<PathBuf> {
        let artifact_config = self.get_tool_config(tool)?;
        let resource_config = artifact_config.resources.get(resource_type.to_plural())?;

        resource_config.merge_target.as_ref().map(PathBuf::from)
    }

    /// Check if a resource type is supported by a tool.
    ///
    /// A resource type is considered supported if it has either:
    /// - A configured installation path (for file-based resources)
    /// - A configured merge target (for resources that merge into config files)
    ///
    /// Returns true if the tool has valid configuration for the given resource type.
    pub fn is_resource_supported(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> bool {
        self.get_tool_config(tool)
            .and_then(|config| config.resources.get(resource_type.to_plural()))
            .map(|res_config| res_config.path.is_some() || res_config.merge_target.is_some())
            .unwrap_or(false)
    }

    /// Returns all dependencies from all resource types.
    ///
    /// This method collects dependencies from agents, snippets, commands,
    /// scripts, hooks, and MCP servers into a single vector. It's commonly used for:
    /// - Manifest validation across all dependency types
    /// - Dependency resolution operations
    /// - Generating reports of all configured dependencies
    /// - Bulk operations on all dependencies
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the dependency name and its configuration.
    /// Each tuple is `(name, dependency)` where:
    /// - `name`: The dependency name as specified in the manifest
    /// - `dependency`: Reference to the [`ResourceDependency`] configuration
    ///
    /// The order follows the resource type order defined in [`crate::core::ResourceType::all()`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::manifest::Manifest;
    /// # let manifest = Manifest::new();
    /// for (name, dep) in manifest.all_dependencies() {
    ///     println!("Dependency: {} -> {}", name, dep.get_path());
    ///     if let Some(source) = dep.get_source() {
    ///         println!("  Source: {}", source);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn all_dependencies(&self) -> Vec<(&str, &ResourceDependency)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort for deterministic iteration order
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    deps.push((name.as_str(), dep));
                }
            }
        }

        deps
    }

    /// Get all dependencies including MCP servers.
    ///
    /// All resource types now use standard `ResourceDependency`, so no conversion needed.
    #[must_use]
    pub fn all_dependencies_with_mcp(
        &self,
    ) -> Vec<(&str, std::borrow::Cow<'_, ResourceDependency>)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort for deterministic iteration order
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    deps.push((name.as_str(), std::borrow::Cow::Borrowed(dep)));
                }
            }
        }

        deps
    }

    /// Get all dependencies with their resource types.
    ///
    /// Returns a vector of tuples containing the dependency name, dependency details,
    /// and the resource type. This preserves type information that is lost in
    /// `all_dependencies_with_mcp()`.
    ///
    /// This is used by the resolver to correctly type transitive dependencies without
    /// falling back to manifest section order lookups.
    ///
    /// Dependencies for disabled tools are automatically filtered out.
    pub fn all_dependencies_with_types(
        &self,
    ) -> Vec<(&str, std::borrow::Cow<'_, ResourceDependency>, crate::core::ResourceType)> {
        let mut deps = Vec::new();

        tracing::debug!(
            "all_dependencies_with_types: agents={}, snippets={}, commands={}, scripts={}, hooks={}, mcp_servers={}, skills={}",
            self.agents.len(),
            self.snippets.len(),
            self.commands.len(),
            self.scripts.len(),
            self.hooks.len(),
            self.mcp_servers.len(),
            self.skills.len()
        );

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort dependencies for deterministic iteration order!
                // HashMap iteration is non-deterministic, so we must sort by name
                // to ensure consistent lockfile generation across runs.
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    // Determine the tool for this dependency
                    let tool_string = dep
                        .get_tool()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| self.get_default_tool(*resource_type));
                    let tool = tool_string.as_str();

                    // Check if the tool is enabled
                    if let Some(tool_config) = self.get_tools_config().types.get(tool) {
                        if !tool_config.enabled {
                            // Skip dependencies for disabled tools
                            tracing::debug!(
                                "Skipping dependency '{}' for disabled tool '{}'",
                                name,
                                tool
                            );
                            continue;
                        }
                    }

                    // Ensure the tool is set on the dependency (apply default if not explicitly set)
                    let dep_with_tool = if dep.get_tool().is_none() {
                        tracing::debug!(
                            "Setting default tool '{}' for dependency '{}' (type: {:?})",
                            tool,
                            name,
                            resource_type
                        );
                        // Need to set the tool - create a modified copy
                        let mut dep_owned = dep.clone();
                        dep_owned.set_tool(Some(tool_string.clone()));
                        std::borrow::Cow::Owned(dep_owned)
                    } else {
                        std::borrow::Cow::Borrowed(dep)
                    };

                    deps.push((name.as_str(), dep_with_tool, *resource_type));
                }
            }
        }

        deps
    }

    /// Check if a dependency with the given name exists in any section.
    ///
    /// Searches the `[agents]`, `[snippets]`, and `[commands]` sections for a dependency
    /// with the specified name. This is useful for avoiding duplicate names
    /// across different resource types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// assert!(manifest.has_dependency("helper"));
    /// assert!(!manifest.has_dependency("nonexistent"));
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs two `HashMap` lookups, so it's O(1) on average.
    #[must_use]
    pub fn has_dependency(&self, name: &str) -> bool {
        self.agents.contains_key(name)
            || self.snippets.contains_key(name)
            || self.commands.contains_key(name)
    }

    /// Get the total number of dependencies across all resource types.
    ///
    /// Counts all dependencies from agents, snippets, commands, mcp-servers,
    /// scripts, hooks, and skills sections.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// if manifest.total_dependencies() == 0 {
    ///     println!("No dependencies defined");
    /// }
    /// ```
    pub fn total_dependencies(&self) -> usize {
        self.agents.len()
            + self.snippets.len()
            + self.commands.len()
            + self.mcp_servers.len()
            + self.scripts.len()
            + self.hooks.len()
            + self.skills.len()
    }

    /// Get a dependency by name from any section.
    ///
    /// Searches both the `[agents]` and `[snippets]` sections for a dependency
    /// with the specified name, returning the first match found. Agents are
    /// searched before snippets.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// if let Some(dep) = manifest.get_dependency("helper") {
    ///     println!("Found dependency: {}", dep.get_path());
    /// }
    /// ```
    ///
    /// # Search Order
    ///
    /// Dependencies are searched in this order:
    /// 1. `[agents]` section
    /// 2. `[snippets]` section
    /// 3. `[commands]` section
    ///
    /// If the same name exists in multiple sections, the first match is returned.
    #[must_use]
    pub fn get_dependency(&self, name: &str) -> Option<&ResourceDependency> {
        self.agents
            .get(name)
            .or_else(|| self.snippets.get(name))
            .or_else(|| self.commands.get(name))
    }

    /// Find a dependency by name from any section (alias for `get_dependency`).
    ///
    /// Searches the `[agents]`, `[snippets]`, and `[commands]` sections for a dependency
    /// with the specified name, returning the first match found.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// if let Some(dep) = manifest.find_dependency("helper") {
    ///     println!("Found dependency: {}", dep.get_path());
    /// }
    /// ```
    pub fn find_dependency(&self, name: &str) -> Option<&ResourceDependency> {
        self.get_dependency(name)
    }

    /// Add or update a source repository in the `[sources]` section.
    ///
    /// Sources map convenient names to Git repository URLs. These names can
    /// then be referenced in dependency specifications to avoid repeating
    /// long URLs throughout the manifest.
    ///
    /// # Parameters
    ///
    /// - `name`: Short, convenient name for the source (e.g., "official", "community")
    /// - `url`: Git repository URL (HTTPS, SSH, or file:// protocol)
    ///
    /// # URL Validation
    ///
    /// The URL is not validated when added - validation occurs during
    /// [`Self::validate`]. Supported URL formats:
    /// - `https://github.com/owner/repo.git`
    /// - `git@github.com:owner/repo.git`
    /// - `file:///absolute/path/to/repo`
    /// - `file:///path/to/local/repo`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add public repository
    /// manifest.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/claude-community/resources.git".to_string()
    /// );
    ///
    /// // Add private repository (SSH)
    /// manifest.add_source(
    ///     "private".to_string(),
    ///     "git@github.com:company/private-resources.git".to_string()
    /// );
    ///
    /// // Add local repository
    /// manifest.add_source(
    ///     "local".to_string(),
    ///     "file:///home/user/my-resources".to_string()
    /// );
    /// ```
    ///
    /// # Security Note
    ///
    /// Never include authentication tokens in the URL. Use SSH keys or
    /// configure authentication globally in `~/.agpm/config.toml`.
    pub fn add_source(&mut self, name: String, url: String) {
        self.sources.insert(name, url);
    }

    /// Add or update a dependency in the appropriate section.
    ///
    /// Adds the dependency to either the `[agents]`, `[snippets]`, or `[commands]` section
    /// based on the `is_agent` parameter. If a dependency with the same name
    /// already exists in the target section, it will be replaced.
    ///
    /// **Note**: This method is deprecated in favor of [`Self::add_typed_dependency`]
    /// which provides explicit control over resource types.
    ///
    /// # Parameters
    ///
    /// - `name`: Unique name for the dependency within its section
    /// - `dep`: The dependency specification (Simple or Detailed)
    /// - `is_agent`: If true, adds to `[agents]`; if false, adds to `[snippets]`
    ///   (Note: Use [`Self::add_typed_dependency`] for commands and other resource types)
    ///
    /// # Validation
    ///
    /// The dependency is not validated when added - validation occurs during
    /// [`Self::validate`]. This allows for building manifests incrementally
    /// before all sources are defined.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency, DetailedDependency};
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add local agent dependency
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../local/helper.md".to_string()),
    ///     true  // is_agent = true
    /// );
    ///
    /// // Add remote snippet dependency
    /// manifest.add_dependency(
    ///     "utils".to_string(),
    ///     ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///         source: Some("community".to_string()),
    ///         path: "snippets/utils.md".to_string(),
    ///         version: Some("v1.0.0".to_string()),
    ///         branch: None,
    ///         rev: None,
    ///         command: None,
    ///         args: None,
    ///         target: None,
    ///         filename: None,
    ///         dependencies: None,
    ///         tool: Some("claude-code".to_string()),
    ///         flatten: None,
    ///         install: None,
    ///         template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    ///     })),
    ///     false  // is_agent = false (snippet)
    /// );
    /// ```
    ///
    /// # Name Conflicts
    ///
    /// This method allows the same dependency name to exist in both the
    /// `[agents]` and `[snippets]` sections. However, some operations like
    /// [`Self::get_dependency`] will prefer agents over snippets when
    /// searching by name.
    pub fn add_dependency(&mut self, name: String, dep: ResourceDependency, is_agent: bool) {
        if is_agent {
            self.agents.insert(name, dep);
        } else {
            self.snippets.insert(name, dep);
        }
    }

    /// Add or update a dependency with specific resource type.
    ///
    /// This is the preferred method for adding dependencies as it explicitly
    /// specifies the resource type using the `ResourceType` enum.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    /// use agpm_cli::core::ResourceType;
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add command dependency
    /// manifest.add_typed_dependency(
    ///     "build".to_string(),
    ///     ResourceDependency::Simple("../commands/build.md".to_string()),
    ///     ResourceType::Command
    /// );
    /// ```
    pub fn add_typed_dependency(
        &mut self,
        name: String,
        dep: ResourceDependency,
        resource_type: crate::core::ResourceType,
    ) {
        match resource_type {
            crate::core::ResourceType::Agent => {
                self.agents.insert(name, dep);
            }
            crate::core::ResourceType::Snippet => {
                self.snippets.insert(name, dep);
            }
            crate::core::ResourceType::Command => {
                self.commands.insert(name, dep);
            }
            crate::core::ResourceType::McpServer => {
                // MCP servers don't use ResourceDependency, they have their own type
                // This method shouldn't be called for MCP servers
                panic!("Use add_mcp_server() for MCP server dependencies");
            }
            crate::core::ResourceType::Script => {
                self.scripts.insert(name, dep);
            }
            crate::core::ResourceType::Hook => {
                self.hooks.insert(name, dep);
            }
            crate::core::ResourceType::Skill => {
                self.skills.insert(name, dep);
            }
        }
    }

    /// Get resource dependencies by type.
    ///
    /// Returns a reference to the HashMap of dependencies for the specified resource type.
    /// This provides a unified interface for accessing different resource collections,
    /// similar to `LockFile::get_resources()`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    /// let agents = manifest.get_resources(&ResourceType::Agent);
    /// println!("Found {} agent dependencies", agents.len());
    /// ```
    #[must_use]
    pub fn get_resources(
        &self,
        resource_type: &crate::core::ResourceType,
    ) -> &HashMap<String, ResourceDependency> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &self.agents,
            ResourceType::Snippet => &self.snippets,
            ResourceType::Command => &self.commands,
            ResourceType::Script => &self.scripts,
            ResourceType::Hook => &self.hooks,
            ResourceType::McpServer => &self.mcp_servers,
            ResourceType::Skill => &self.skills,
        }
    }

    /// Get all resource dependencies across all types.
    ///
    /// Returns a vector of tuples containing the resource type, manifest key (name),
    /// and the dependency specification. This provides a unified way to iterate over
    /// all resources regardless of type.
    ///
    /// # Returns
    ///
    /// A vector of `(ResourceType, &str, &ResourceDependency)` tuples where:
    /// - The first element is the type of resource (Agent, Snippet, etc.)
    /// - The second element is the manifest key (the name in the TOML file)
    /// - The third element is the resource dependency specification
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// let all = manifest.all_resources();
    ///
    /// for (resource_type, name, dep) in all {
    ///     println!("{:?}: {}", resource_type, name);
    /// }
    /// ```
    #[must_use]
    pub fn all_resources(&self) -> Vec<(crate::core::ResourceType, &str, &ResourceDependency)> {
        use crate::core::ResourceType;

        let mut resources = Vec::new();

        for resource_type in ResourceType::all() {
            let type_resources = self.get_resources(resource_type);
            for (name, dep) in type_resources {
                resources.push((*resource_type, name.as_str(), dep));
            }
        }

        resources
    }

    /// Add or update an MCP server configuration.
    ///
    /// MCP servers now use standard `ResourceDependency` format,
    /// pointing to JSON configuration files in source repositories.
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add MCP server from source repository
    /// manifest.add_mcp_server(
    ///     "filesystem".to_string(),
    ///     ResourceDependency::Simple("../local/mcp-servers/filesystem.json".to_string())
    /// );
    /// ```
    pub fn add_mcp_server(&mut self, name: String, dependency: ResourceDependency) {
        self.mcp_servers.insert(name, dependency);
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}
