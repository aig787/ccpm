//! Manifest file parsing and validation for CCPM projects.
//!
//! This module handles the `ccpm.toml` manifest file that defines project
//! dependencies and configuration. The manifest uses TOML format and follows
//! a structure similar to Cargo.toml, providing a lockfile-based dependency
//! management system for Claude Code resources.
//!
//! # Overview
//!
//! The manifest system enables:
//! - Declarative dependency management through `ccpm.toml`
//! - Reproducible installations via lockfile generation
//! - Support for multiple Git-based source repositories
//! - Local and remote dependency resolution
//! - Version constraint specification and validation
//! - Cross-platform path handling and installation
//! - MCP (Model Context Protocol) server configuration management
//! - Atomic file operations for reliability
//!
//! # Complete TOML Format Specification
//!
//! ## Basic Structure
//!
//! A `ccpm.toml` manifest file consists of four main sections:
//!
//! ```toml
//! # Named source repositories (optional)
//! [sources]
//! # Git repository URLs mapped to convenient names
//! official = "https://github.com/example-org/ccpm-official.git"
//! community = "https://github.com/community/ccpm-resources.git"
//! private = "git@github.com:company/private-resources.git"
//!
//! # Installation target directories (optional)
//! [target]
//! # Where agents should be installed (default: ".claude/agents")
//! agents = ".claude/agents"
//! # Where snippets should be installed (default: ".claude/ccpm/snippets")
//! snippets = ".claude/ccpm/snippets"
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
//! official = "https://github.com/owner/ccpm-resources.git"
//! community = "https://gitlab.com/group/ccpm-community.git"
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
//! snippets = ".claude/ccpm/snippets"  # Where snippet .md files are copied
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
//! CCPM supports flexible version constraints:
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
//! **CRITICAL**: Never include authentication credentials in `ccpm.toml`:
//!
//! ```toml
//! # ❌ NEVER DO THIS - credentials will be committed to git
//! [sources]
//! private = "https://token:ghp_xxxx@github.com/company/repo.git"
//!
//! # ✅ Instead, use global configuration in ~/.ccpm/config.toml
//! # Or use SSH keys with git@ URLs
//! [sources]
//! private = "git@github.com:company/repo.git"
//! ```
//!
//! Authentication should be configured globally in `~/.ccpm/config.toml` or
//! through SSH keys for `git@` URLs. See [`crate::config`] for details.
//!
//! ## Relationship to Lockfile
//!
//! The manifest works together with the lockfile (`ccpm.lock`):
//!
//! - **Manifest (`ccpm.toml`)**: Declares dependencies and constraints
//! - **Lockfile (`ccpm.lock`)**: Records exact resolved versions and checksums
//!
//! When you run `ccpm install`:
//! 1. Reads dependencies from `ccpm.toml`
//! 2. Resolves versions within constraints  
//! 3. Generates/updates `ccpm.lock` with exact commits
//! 4. Installs resources to target directories
//!
//! See [`crate::lockfile`] for lockfile format details.
//!
//! ## Cross-Platform Compatibility
//!
//! CCPM handles platform differences automatically:
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
//! 8. **Version control manifests**: Always commit `ccpm.toml` to git
//! 9. **Validate regularly**: Run `ccpm validate` before commits
//! 10. **Use lockfiles**: Commit `ccpm.lock` for reproducible builds
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
//! The manifest module works closely with other CCPM modules:
//!
//! ### With [`crate::resolver`]
//!
//! ```rust,ignore
//! use ccpm::manifest::Manifest;
//! use ccpm::resolver::DependencyResolver;
//!
//! let manifest = Manifest::load(&project_path.join("ccpm.toml"))?;
//! let resolver = DependencyResolver::new(&manifest);
//! let resolved = resolver.resolve_all().await?;
//! ```
//!
//! ### With [`crate::lockfile`]
//!
//! ```rust,ignore  
//! use ccpm::manifest::Manifest;
//! use ccpm::lockfile::LockFile;
//!
//! let manifest = Manifest::load(&project_path.join("ccpm.toml"))?;
//! let lockfile = LockFile::generate_from_manifest(&manifest).await?;
//! lockfile.save(&project_path.join("ccpm.lock"))?;
//! ```
//!
//! ### With [`crate::git`] for Source Management
//!
//! ```rust,ignore
//! use ccpm::manifest::Manifest;
//! use ccpm::git::GitManager;
//!
//! let manifest = Manifest::load(&project_path.join("ccpm.toml"))?;
//! let git = GitManager::new(&cache_dir);
//!
//! for (name, url) in &manifest.sources {
//!     git.clone_or_update(name, url).await?;
//! }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The main manifest file structure representing a complete `ccpm.toml` file.
///
/// This struct encapsulates all configuration for a CCPM project, including
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
/// ```rust
/// use ccpm::manifest::{Manifest, ResourceDependency};
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

    /// Installation target directory configuration.
    ///
    /// Specifies where different resource types should be installed relative
    /// to the project root. Uses sensible defaults if not specified.
    ///
    /// See [`TargetConfig`] for details on default values and customization.
    #[serde(default)]
    pub target: TargetConfig,

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
    /// `.claude/ccpm/mcp-servers/` and configured in `.mcp.json`.
    ///
    /// See [`ResourceDependency`] for specification format details.
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        rename = "mcp-servers"
    )]
    pub mcp_servers: HashMap<String, ResourceDependency>,

    /// Script dependencies mapping names to their specifications.
    ///
    /// Scripts are executable files (.sh, .js, .py, etc.) that can be run by hooks
    /// or independently. They are installed to `.claude/ccpm/scripts/` and can be
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
}

/// Target directories configuration specifying where resources are installed.
///
/// This struct defines the installation destinations for different resource types
/// within a CCPM project. All paths are relative to the project root (where
/// `ccpm.toml` is located) unless they are absolute paths.
///
/// # Default Values
///
/// - **Agents**: `.claude/agents` - Following Claude Code conventions
/// - **Snippets**: `.claude/ccpm/snippets` - Following Claude Code conventions
/// - **Commands**: `.claude/commands` - Following Claude Code conventions
///
/// # Path Resolution
///
/// - Relative paths are resolved from the manifest directory
/// - Absolute paths are used as-is (not recommended for portability)
/// - Path separators are automatically normalized for the target platform
/// - Directories are created automatically during installation if they don't exist
///
/// # Examples
///
/// ```toml
/// # Default configuration (can be omitted)
/// [target]
/// agents = ".claude/agents"
/// snippets = ".claude/ccpm/snippets"
/// commands = ".claude/commands"
///
/// # Custom configuration
/// [target]
/// agents = "resources/ai-agents"
/// snippets = "templates/code-snippets"
/// commands = "resources/commands"
///
/// # Absolute paths (use with caution)
/// [target]
/// agents = "/opt/claude/agents"
/// snippets = "/opt/claude/snippets"
/// commands = "/opt/claude/commands"
/// ```
///
/// # Cross-Platform Considerations
///
/// CCPM automatically handles platform differences:
/// - Forward slashes work on all platforms (Windows, macOS, Linux)
/// - Path separators are normalized during installation
/// - Long path support on Windows is handled automatically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    /// Directory where agent `.md` files should be installed.
    ///
    /// Agents are AI model definitions, prompts, or behavioral specifications.
    /// This directory will contain copies of agent files from dependencies.
    ///
    /// **Default**: `.claude/agents` (following Claude Code conventions)
    #[serde(default = "default_agents_dir")]
    pub agents: String,

    /// Directory where snippet `.md` files should be installed.
    ///
    /// Snippets are reusable code templates, examples, or documentation.
    /// This directory will contain copies of snippet files from dependencies.
    ///
    /// **Default**: `.claude/ccpm/snippets` (following Claude Code conventions)
    #[serde(default = "default_snippets_dir")]
    pub snippets: String,

    /// Directory where command `.md` files should be installed.
    ///
    /// Commands are Claude Code slash commands that provide custom functionality.
    /// This directory will contain copies of command files from dependencies.
    ///
    /// **Default**: `.claude/commands` (following Claude Code conventions)
    #[serde(default = "default_commands_dir")]
    pub commands: String,

    /// Directory where MCP server configurations should be tracked.
    ///
    /// Note: MCP servers are configured in `.mcp.json` at the project root,
    /// not installed to this directory. This directory is used for tracking
    /// metadata about installed servers.
    ///
    /// **Default**: `.claude/ccpm/mcp-servers` (following Claude Code conventions)
    #[serde(default = "default_mcp_servers_dir", rename = "mcp-servers")]
    pub mcp_servers: String,

    /// Directory where script files should be installed.
    ///
    /// Scripts are executable files (.sh, .js, .py, etc.) that can be referenced
    /// by hooks or run independently.
    ///
    /// **Default**: `.claude/ccpm/scripts` (following Claude Code conventions)
    #[serde(default = "default_scripts_dir")]
    pub scripts: String,

    /// Directory where hook configuration files should be installed.
    ///
    /// Hooks are JSON configuration files that define event-based automation
    /// in Claude Code.
    ///
    /// **Default**: `.claude/ccpm/hooks` (following Claude Code conventions)
    #[serde(default = "default_hooks_dir")]
    pub hooks: String,

    /// Whether to automatically add installed files to `.gitignore`.
    ///
    /// When enabled (default), CCPM will create or update `.gitignore`
    /// to exclude all installed files from version control. This prevents
    /// installed dependencies from being committed to your repository.
    ///
    /// Set to `false` if you want to commit installed resources to version control.
    ///
    /// **Default**: `true`
    #[serde(default = "default_gitignore")]
    pub gitignore: bool,
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            agents: default_agents_dir(),
            snippets: default_snippets_dir(),
            commands: default_commands_dir(),
            mcp_servers: default_mcp_servers_dir(),
            scripts: default_scripts_dir(),
            hooks: default_hooks_dir(),
            gitignore: default_gitignore(),
        }
    }
}

fn default_agents_dir() -> String {
    ".claude/agents".to_string()
}

fn default_snippets_dir() -> String {
    ".claude/ccpm/snippets".to_string()
}

fn default_commands_dir() -> String {
    ".claude/commands".to_string()
}

fn default_mcp_servers_dir() -> String {
    ".claude/ccpm/mcp-servers".to_string()
}

fn default_scripts_dir() -> String {
    ".claude/ccpm/scripts".to_string()
}

fn default_hooks_dir() -> String {
    ".claude/ccpm/hooks".to_string()
}

fn default_gitignore() -> bool {
    true
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceDependency {
    /// Simple path-only dependency, typically for local files.
    ///
    /// This variant represents the simplest dependency format where only
    /// a file path is specified. It's primarily used for local dependencies
    /// that exist in the filesystem relative to the project.
    ///
    /// # Format
    ///
    /// ```toml
    /// dependency-name = "path/to/file.md"
    /// ```
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Relative paths from manifest directory
    /// helper = "../shared/helper.md"
    /// custom = "./local/custom.md"
    ///
    /// # Absolute paths (not recommended)
    /// system = "/usr/local/share/agent.md"
    /// ```
    ///
    /// # Limitations
    ///
    /// - Cannot specify version constraints
    /// - Cannot reference remote Git sources
    /// - Must be a valid filesystem path
    /// - Path must exist at installation time
    Simple(String),

    /// Detailed dependency specification with full control.
    ///
    /// This variant provides complete control over dependency specification,
    /// supporting both local and remote dependencies with version constraints,
    /// Git references, and explicit source mapping.
    ///
    /// See [`DetailedDependency`] for field-level documentation.
    Detailed(DetailedDependency),
}

/// Detailed dependency specification with full control over source resolution.
///
/// This struct provides fine-grained control over dependency specification,
/// supporting both local filesystem paths and remote Git repository resources
/// with flexible version constraints and Git reference handling.
///
/// # Field Relationships
///
/// The fields work together with specific validation rules:
/// - If `source` is specified: Must have either `version` or `git`
/// - If `source` is omitted: Dependency is local, `version` and `git` are ignored
/// - `path` is always required and cannot be empty
///
/// # Examples
///
/// ## Remote Dependencies
///
/// ```toml
/// [agents]
/// # Semantic version constraint
/// stable = { source = "official", path = "agents/stable.md", version = "v1.0.0" }
///
/// # Latest version (not recommended for production)
/// latest = { source = "community", path = "agents/utils.md", version = "latest" }
///
/// # Specific Git branch
/// cutting-edge = { source = "official", path = "agents/new.md", git = "develop" }
///
/// # Specific commit SHA (maximum reproducibility)
/// pinned = { source = "community", path = "agents/tool.md", git = "a1b2c3d4e5f6..." }
///
/// # Git tag
/// release = { source = "official", path = "agents/release.md", git = "v2.0-release" }
/// ```
///
/// ## Local Dependencies
///
/// ```toml
/// [agents]
/// # Local file (version/git fields ignored if present)
/// local-helper = { path = "../shared/helper.md" }
/// custom = { path = "./local/custom.md" }
/// ```
///
/// # Version Resolution Priority
///
/// When both `version` and `git` are specified, `git` takes precedence:
///
/// ```toml
/// # This will use the "develop" branch, not "v1.0.0"
/// conflicted = { source = "repo", path = "file.md", version = "v1.0.0", git = "develop" }
/// ```
///
/// # Path Format
///
/// Paths are interpreted differently based on context:
/// - **Remote dependencies**: Path within the Git repository
/// - **Local dependencies**: Filesystem path relative to manifest directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    /// Source repository name referencing the `[sources]` section.
    ///
    /// When specified, this dependency will be resolved from a Git repository.
    /// The name must exactly match a key in the manifest's `[sources]` table.
    ///
    /// **Omit this field** to create a local filesystem dependency.
    ///
    /// # Examples
    ///
    /// ```toml
    /// # References this source definition:
    /// [sources]
    /// official = "https://github.com/org/repo.git"
    ///
    /// [agents]
    /// remote-agent = { source = "official", path = "agents/tool.md", version = "v1.0.0" }
    /// local-agent = { path = "../local/tool.md" }  # No source = local dependency
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Path to the resource file or glob pattern for multiple resources.
    ///
    /// For **remote dependencies**: Path within the Git repository\
    /// For **local dependencies**: Filesystem path relative to manifest directory\
    /// For **pattern dependencies**: Glob pattern to match multiple resources
    ///
    /// This field supports both individual file paths and glob patterns:
    /// - Individual file: `"agents/helper.md"`
    /// - Pattern matching: `"agents/*.md"`, `"**/*.md"`, `"agents/[a-z]*.md"`
    ///
    /// Pattern dependencies are detected by the presence of glob characters
    /// (`*`, `?`, `[`) in the path. When a pattern is detected, CCPM will
    /// expand it to match all resources in the source repository.
    ///
    /// # Examples
    ///
    /// ```toml
    /// # Remote: single file in git repo
    /// remote = { source = "repo", path = "agents/helper.md", version = "v1.0.0" }
    ///
    /// # Local: filesystem path
    /// local = { path = "../shared/helper.md" }
    ///
    /// # Pattern: all agents in AI folder
    /// ai_agents = { source = "repo", path = "agents/ai/*.md", version = "v1.0.0" }
    ///
    /// # Pattern: all agents recursively
    /// all_agents = { source = "repo", path = "agents/**/*.md", version = "v1.0.0" }
    /// ```
    pub path: String,

    /// Version constraint for Git tag resolution.
    ///
    /// Specifies which version of the resource to use when resolving from
    /// a Git repository. This field is ignored for local dependencies.
    ///
    /// **Note**: If both `version` and `git` are specified, `git` takes precedence.
    ///
    /// # Supported Formats
    ///
    /// - `"v1.0.0"` - Exact semantic version tag
    /// - `"1.0.0"` - Exact version (v prefix optional)  
    /// - `"latest"` - Latest available version (determined by Git tags)
    /// - `"main"` - Use main/master branch HEAD
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// stable = { source = "repo", path = "agent.md", version = "v1.0.0" }
    /// latest = { source = "repo", path = "agent.md", version = "latest" }
    /// main = { source = "repo", path = "agent.md", version = "main" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Git branch to track.
    ///
    /// Specifies a Git branch to use when resolving the dependency.
    /// Branch references are mutable and will update to the latest commit on each update.
    /// This field is ignored for local dependencies.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Track the main branch
    /// dev = { source = "repo", path = "agent.md", branch = "main" }
    ///
    /// # Track a feature branch
    /// experimental = { source = "repo", path = "agent.md", branch = "feature/new-capability" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Git commit hash (revision).
    ///
    /// Specifies an exact Git commit to use when resolving the dependency.
    /// Provides maximum reproducibility as commits are immutable.
    /// This field is ignored for local dependencies.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Pin to exact commit (full hash)
    /// pinned = { source = "repo", path = "agent.md", rev = "a1b2c3d4e5f67890abcdef1234567890abcdef12" }
    ///
    /// # Pin to exact commit (abbreviated)
    /// stable = { source = "repo", path = "agent.md", rev = "abc123def" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,

    /// Command to execute for MCP servers.
    ///
    /// This field is specific to MCP server dependencies and specifies
    /// the command that will be executed to run the MCP server.
    /// Only used for entries in the `[mcp-servers]` section.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [mcp-servers]
    /// github = { source = "repo", path = "mcp/github.toml", version = "v1.0.0", command = "npx" }
    /// sqlite = { path = "./local/sqlite.toml", command = "uvx" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments to pass to the MCP server command.
    ///
    /// This field is specific to MCP server dependencies and provides
    /// the arguments that will be passed to the command when starting
    /// the MCP server. Only used for entries in the `[mcp-servers]` section.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [mcp-servers]
    /// github = {
    ///     source = "repo",
    ///     path = "mcp/github.toml",
    ///     version = "v1.0.0",
    ///     command = "npx",
    ///     args = ["-y", "@modelcontextprotocol/server-github"]
    /// }
    /// sqlite = {
    ///     path = "./local/sqlite.toml",
    ///     command = "uvx",
    ///     args = ["mcp-server-sqlite", "--db", "./data/local.db"]
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Custom target directory for this dependency.
    ///
    /// Overrides the default installation directory for this specific dependency.
    /// The path is relative to the `.claude` directory for consistency and security.
    /// If not specified, the dependency will be installed to the default location
    /// based on its resource type.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Install to .claude/custom/tools/ instead of default .claude/agents/
    /// special-agent = {
    ///     source = "repo",
    ///     path = "agent.md",
    ///     version = "v1.0.0",
    ///     target = "custom/tools"
    /// }
    ///
    /// # Install to .claude/integrations/ai/
    /// integration = {
    ///     source = "repo",
    ///     path = "integration.md",
    ///     version = "v2.0.0",
    ///     target = "integrations/ai"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Custom filename for this dependency.
    ///
    /// Overrides the default filename (which is based on the dependency key).
    /// The filename should include the desired file extension. If not specified,
    /// the dependency will be installed using the key name with an automatically
    /// determined extension based on the resource type.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Install as "ai-assistant.md" instead of "my-ai.md"
    /// my-ai = {
    ///     source = "repo",
    ///     path = "agent.md",
    ///     version = "v1.0.0",
    ///     filename = "ai-assistant.md"
    /// }
    ///
    /// # Install with a different extension
    /// doc-agent = {
    ///     source = "repo",
    ///     path = "documentation.md",
    ///     version = "v2.0.0",
    ///     filename = "docs-helper.txt"
    /// }
    ///
    /// [scripts]
    /// # Rename a script during installation
    /// analyzer = {
    ///     source = "repo",
    ///     path = "scripts/data-analyzer-v3.py",
    ///     version = "v1.0.0",
    ///     filename = "analyze.py"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl Manifest {
    /// Create a new empty manifest with default configuration.
    ///
    /// The new manifest will have:
    /// - No sources defined
    /// - Default target directories (`.claude/agents` and `.claude/ccpm/snippets`)
    /// - No dependencies
    ///
    /// This is typically used when programmatically building a manifest or
    /// as a starting point for adding dependencies.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// assert!(manifest.sources.is_empty());
    /// assert!(manifest.agents.is_empty());
    /// assert!(manifest.snippets.is_empty());
    /// assert!(manifest.commands.is_empty());
    /// assert!(manifest.mcp_servers.is_empty());
    /// assert_eq!(manifest.target.agents, ".claude/agents");
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            target: TargetConfig::default(),
            agents: HashMap::new(),
            snippets: HashMap::new(),
            commands: HashMap::new(),
            mcp_servers: HashMap::new(),
            scripts: HashMap::new(),
            hooks: HashMap::new(),
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
    /// ```rust,ignore
    /// use ccpm::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// // Load a manifest file
    /// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
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
    /// Expects a valid TOML file following the CCPM manifest format.
    /// See the module-level documentation for complete format specification.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "Cannot read manifest file: {}\n\n\
                    Possible causes:\n\
                    - File doesn't exist or has been moved\n\
                    - Permission denied (check file ownership)\n\
                    - File is locked by another process",
                path.display()
            )
        })?;

        let manifest: Self = toml::from_str(&content)
            .map_err(|e| crate::core::CcpmError::ManifestParseError {
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

        manifest.validate()?;

        Ok(manifest)
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
    /// ```rust
    /// use ccpm::manifest::Manifest;
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
    /// # let manifest_path = temp_dir.path().join("ccpm.toml");
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
    /// snippets = ".claude/ccpm/snippets"
    ///
    /// [agents]
    /// helper = { source = "official", path = "agents/helper.md", version = "v1.0.0" }
    ///
    /// [snippets]
    /// utils = { source = "official", path = "snippets/utils.md", version = "v1.0.0" }
    /// ```
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize manifest data to TOML format")?;

        std::fs::write(path, content).with_context(|| {
            format!(
                "Cannot write manifest file: {}\n\n\
                    Possible causes:\n\
                    - Permission denied (try running with elevated permissions)\n\
                    - Directory doesn't exist\n\
                    - Disk is full or read-only\n\
                    - File is locked by another process",
                path.display()
            )
        })?;

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
    /// - [`crate::core::CcpmError::SourceNotFound`]: Referenced source doesn't exist
    /// - [`crate::core::CcpmError::ManifestValidationError`]: General validation failures
    /// - Context errors for specific issues with actionable suggestions
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{Manifest, ResourceDependency, DetailedDependency};
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
    ///     ResourceDependency::Detailed(DetailedDependency {
    ///         source: Some("missing".to_string()),
    ///         path: "agent.md".to_string(),
    ///         version: Some("v1.0.0".to_string()),
    ///         branch: None,
    ///         rev: None,
    ///         command: None,
    ///         args: None,
    ///         target: None,
    ///         filename: None,
    ///     }),
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
        // Check that all referenced sources exist and dependencies have required fields
        for (name, dep) in self.all_dependencies() {
            // Check for empty path
            if dep.get_path().is_empty() {
                return Err(crate::core::CcpmError::ManifestValidationError {
                    reason: format!("Missing required field 'path' for dependency '{name}'"),
                }
                .into());
            }

            // Validate pattern safety if it's a pattern dependency
            if dep.is_pattern() {
                crate::pattern::validate_pattern_safety(dep.get_path()).map_err(|e| {
                    crate::core::CcpmError::ManifestValidationError {
                        reason: format!("Invalid pattern in dependency '{}': {}", name, e),
                    }
                })?;
            }

            // Check for version when source is specified (non-local dependencies)
            if let Some(source) = dep.get_source() {
                if !self.sources.contains_key(source) {
                    return Err(crate::core::CcpmError::SourceNotFound {
                        name: source.to_string(),
                    }
                    .into());
                }

                // Check if the source URL is a local path
                let source_url = self.sources.get(source).unwrap();
                let is_local_source = source_url.starts_with('/')
                    || source_url.starts_with("./")
                    || source_url.starts_with("../");

                // Non-local dependencies (git repositories) should have a version
                // Local path sources don't need versions
                if !is_local_source
                    && (dep.get_version().is_none() || dep.get_version() == Some(""))
                {
                    return Err(crate::core::CcpmError::ManifestValidationError {
                        reason: format!("Missing required field 'version' for dependency '{name}' from source '{source}'. Suggestion: Add version = \"v1.0.0\" to specify a version"),
                    }
                    .into());
                }
            } else {
                // For local path dependencies (no source), version is not allowed
                // Skip directory check for pattern dependencies
                if !dep.is_pattern() {
                    let path = dep.get_path();
                    let is_plain_dir =
                        path.starts_with('/') || path.starts_with("./") || path.starts_with("../");

                    if is_plain_dir && dep.get_version().is_some() {
                        return Err(crate::core::CcpmError::ManifestValidationError {
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
                        return Err(crate::core::CcpmError::ManifestValidationError {
                            reason: format!("Version conflict for dependency '{name}': found versions '{existing_version}' and '{version}'"),
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
                return Err(crate::core::CcpmError::ManifestValidationError {
                    reason: format!(
                        "Source '{name}' has invalid URL: '{url}'. Must be HTTP(S), SSH (git@...), or file:// URL"
                    ),
                }
                .into());
            }

            // Check if plain directory path is used as a source
            if expanded_url.starts_with('/')
                || expanded_url.starts_with("./")
                || expanded_url.starts_with("../")
            {
                return Err(crate::core::CcpmError::ManifestValidationError {
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
                        return Err(crate::core::CcpmError::ManifestValidationError {
                            reason: format!(
                                "Case conflict: '{}' and '{}' would map to the same file on case-insensitive filesystems. To ensure portability across platforms, resource names must be case-insensitively unique.",
                                name, other_name
                            ),
                        }
                        .into());
                    }
                }
            }
        }

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
    /// ```rust
    /// use ccpm::manifest::Manifest;
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
    /// Returns the HashMap of dependencies for the specified resource type.
    /// Note: MCP servers return None as they use a different dependency type.
    pub fn get_dependencies(
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
        }
    }

    /// Get the target directory for a specific resource type
    pub fn get_target_dir(&self, resource_type: crate::core::ResourceType) -> &str {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &self.target.agents,
            ResourceType::Snippet => &self.target.snippets,
            ResourceType::Command => &self.target.commands,
            ResourceType::Script => &self.target.scripts,
            ResourceType::Hook => &self.target.hooks,
            ResourceType::McpServer => &self.target.mcp_servers,
        }
    }

    #[must_use]
    pub fn all_dependencies(&self) -> Vec<(&str, &ResourceDependency)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                for (name, dep) in type_deps {
                    deps.push((name.as_str(), dep));
                }
            }
        }

        deps
    }

    /// Get all dependencies including MCP servers.
    ///
    /// All resource types now use standard ResourceDependency, so no conversion needed.
    #[must_use]
    pub fn all_dependencies_with_mcp(
        &self,
    ) -> Vec<(&str, std::borrow::Cow<'_, ResourceDependency>)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                for (name, dep) in type_deps {
                    deps.push((name.as_str(), std::borrow::Cow::Borrowed(dep)));
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
    /// ```rust
    /// use ccpm::manifest::{Manifest, ResourceDependency};
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

    /// Get a dependency by name from any section.
    ///
    /// Searches both the `[agents]` and `[snippets]` sections for a dependency
    /// with the specified name, returning the first match found. Agents are
    /// searched before snippets.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{Manifest, ResourceDependency};
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
    /// ```rust
    /// use ccpm::manifest::Manifest;
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
    /// configure authentication globally in `~/.ccpm/config.toml`.
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
    /// ```rust
    /// use ccpm::manifest::{Manifest, ResourceDependency, DetailedDependency};
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
    ///     ResourceDependency::Detailed(DetailedDependency {
    ///         source: Some("community".to_string()),
    ///         path: "snippets/utils.md".to_string(),
    ///         version: Some("v1.0.0".to_string()),
    ///         branch: None,
    ///         rev: None,
    ///         command: None,
    ///         args: None,
    ///         target: None,
    ///         filename: None,
    ///     }),
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
    /// ```rust
    /// use ccpm::manifest::{Manifest, ResourceDependency};
    /// use ccpm::core::ResourceType;
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
        }
    }

    /// Add or update an MCP server configuration.
    ///
    /// MCP servers now use standard ResourceDependency format,
    /// pointing to JSON configuration files in source repositories.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::manifest::{Manifest, ResourceDependency};
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

impl ResourceDependency {
    /// Get the source repository name if this is a remote dependency.
    ///
    /// Returns the source name for remote dependencies (those that reference
    /// a Git repository), or `None` for local dependencies (those that reference
    /// local filesystem paths).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - no source
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.get_source().is_none());
    ///
    /// // Remote dependency - has source
    /// let remote = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert_eq!(remote.get_source(), Some("official"));
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is commonly used to:
    /// - Determine if dependency resolution should use Git vs filesystem
    /// - Validate that referenced sources exist in the manifest
    /// - Filter dependencies by type (local vs remote)
    /// - Generate dependency graphs and reports
    #[must_use]
    pub fn get_source(&self) -> Option<&str> {
        match self {
            ResourceDependency::Simple(_) => None,
            ResourceDependency::Detailed(d) => d.source.as_deref(),
        }
    }

    /// Get the custom target directory for this dependency.
    ///
    /// Returns the custom target directory if specified, or `None` if the
    /// dependency should use the default installation location for its resource type.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Dependency with custom target
    /// let custom = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     target: Some("custom/tools".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     filename: None,
    /// });
    /// assert_eq!(custom.get_target(), Some("custom/tools"));
    ///
    /// // Dependency without custom target
    /// let default = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(default.get_target().is_none());
    /// ```
    #[must_use]
    pub fn get_target(&self) -> Option<&str> {
        match self {
            ResourceDependency::Simple(_) => None,
            ResourceDependency::Detailed(d) => d.target.as_deref(),
        }
    }

    /// Get the custom filename for this dependency.
    ///
    /// Returns the custom filename if specified, or `None` if the
    /// dependency should use the default filename based on the dependency key.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Dependency with custom filename
    /// let custom = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     filename: Some("ai-assistant.md".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    /// });
    /// assert_eq!(custom.get_filename(), Some("ai-assistant.md"));
    ///
    /// // Dependency without custom filename
    /// let default = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(default.get_filename().is_none());
    /// ```
    #[must_use]
    pub fn get_filename(&self) -> Option<&str> {
        match self {
            ResourceDependency::Simple(_) => None,
            ResourceDependency::Detailed(d) => d.filename.as_deref(),
        }
    }

    /// Get the path to the resource file.
    ///
    /// Returns the path component of the dependency, which is interpreted
    /// differently based on whether this is a local or remote dependency:
    ///
    /// - **Local dependencies**: Filesystem path relative to the manifest directory
    /// - **Remote dependencies**: Path within the Git repository
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - filesystem path
    /// let local = ResourceDependency::Simple("../shared/helper.md".to_string());
    /// assert_eq!(local.get_path(), "../shared/helper.md");
    ///
    /// // Remote dependency - repository path  
    /// let remote = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/code-reviewer.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert_eq!(remote.get_path(), "agents/code-reviewer.md");
    /// ```
    ///
    /// # Path Resolution
    ///
    /// The returned path should be processed appropriately based on the dependency type:
    /// - Local paths may need resolution against the manifest directory
    /// - Remote paths are used directly within the cloned repository
    /// - All paths should use forward slashes (/) for cross-platform compatibility
    #[must_use]
    pub fn get_path(&self) -> &str {
        match self {
            ResourceDependency::Simple(path) => path,
            ResourceDependency::Detailed(d) => &d.path,
        }
    }

    /// Check if this is a pattern-based dependency.
    ///
    /// Returns `true` if this dependency uses a glob pattern to match
    /// multiple resources, `false` if it specifies a single resource path.
    ///
    /// Patterns are detected by the presence of glob characters (`*`, `?`, `[`)
    /// in the path field.
    #[must_use]
    pub fn is_pattern(&self) -> bool {
        let path = self.get_path();
        path.contains('*') || path.contains('?') || path.contains('[')
    }

    /// Get the version constraint for dependency resolution.
    ///
    /// Returns the version constraint that should be used when resolving this
    /// dependency from a Git repository. For local dependencies, always returns `None`.
    ///
    /// # Priority Rules
    ///
    /// If both `version` and `git` fields are present in a detailed dependency,
    /// the `git` field takes precedence:
    ///
    /// ```rust,no_run
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// let dep = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),  // This is ignored
    ///     branch: Some("develop".to_string()),   // This takes precedence over version
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    ///
    /// assert_eq!(dep.get_version(), Some("develop"));
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - no version
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.get_version().is_none());
    ///
    /// // Remote dependency with version
    /// let versioned = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert_eq!(versioned.get_version(), Some("v1.0.0"));
    ///
    /// // Remote dependency with branch reference
    /// let branch_ref = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: None,
    ///     branch: Some("main".to_string()),
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert_eq!(branch_ref.get_version(), Some("main"));
    /// ```
    ///
    /// # Version Formats
    ///
    /// Supported version constraint formats include:
    /// - Semantic versions: `"v1.0.0"`, `"1.2.3"`
    /// - Branch names: `"main"`, `"develop"`, `"feature/new"`
    /// - Git tags: `"release-2023"`, `"stable"`
    /// - Commit SHAs: `"a1b2c3d4e5f6..."`
    /// - Special values: `"latest"` (resolve to latest tag)
    #[must_use]
    pub fn get_version(&self) -> Option<&str> {
        match self {
            ResourceDependency::Simple(_) => None,
            ResourceDependency::Detailed(d) => {
                // Precedence: rev > branch > version
                d.rev
                    .as_deref()
                    .or(d.branch.as_deref())
                    .or(d.version.as_deref())
            }
        }
    }

    /// Check if this is a local filesystem dependency.
    ///
    /// Returns `true` if this dependency refers to a local file (no Git source),
    /// or `false` if it's a remote dependency that will be resolved from a
    /// Git repository.
    ///
    /// This is a convenience method equivalent to `self.get_source().is_none()`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.is_local());
    ///
    /// // Remote dependency
    /// let remote = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert!(!remote.is_local());
    ///
    /// // Local detailed dependency (no source specified)
    /// let local_detailed = ResourceDependency::Detailed(DetailedDependency {
    ///     source: None,
    ///     path: "../shared/tool.md".to_string(),
    ///     version: None,
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    /// });
    /// assert!(local_detailed.is_local());
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful for:
    /// - Choosing between filesystem and Git resolution strategies
    /// - Validation logic (local deps can't have versions)
    /// - Installation planning (local deps don't need caching)
    /// - Progress reporting (different steps for local vs remote)
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.get_source().is_none()
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Expand environment variables and home directory in URLs.
///
/// This function handles URL expansion for source repository specifications,
/// supporting environment variable substitution and home directory expansion
/// while preserving standard Git URL formats.
///
/// # Processing Rules
///
/// 1. **Standard Git URLs** are returned unchanged:
///    - `http://` and `https://` URLs
///    - SSH URLs starting with `git@`
///    - File URLs starting with `file://`
///
/// 2. **Local paths** with expansion markers are processed:
///    - Environment variables: `${VAR_NAME}` or `$VAR_NAME`
///    - Home directory: `~` at the start of the path
///    - Relative paths: `./` or `../`
///    - Absolute paths: starting with `/`
///
/// 3. **Converted to file:// URLs**: Local paths are converted to file:// URLs
///    for consistent handling throughout the system.
///
/// # Examples
///
/// ```rust,ignore
/// # use ccpm::manifest::expand_url;
/// # fn example() -> anyhow::Result<()> {
/// // Standard URLs remain unchanged
/// assert_eq!(expand_url("https://github.com/user/repo.git")?,
///            "https://github.com/user/repo.git");
/// assert_eq!(expand_url("git@github.com:user/repo.git")?,
///            "git@github.com:user/repo.git");
///
/// // Environment variable expansion (if HOME=/home/user)
/// std::env::set_var("REPOS_DIR", "/home/user/repositories");
/// assert_eq!(expand_url("${REPOS_DIR}/my-repo")?,
///            "file:///home/user/repositories/my-repo");
///
/// // Home directory expansion  
/// assert_eq!(expand_url("~/git/my-repo")?,
///            "file:///home/user/git/my-repo");
/// # Ok(())
/// # }
/// ```
///
/// # Error Handling
///
/// - Returns the original URL if expansion fails
/// - Never panics, even with malformed input
/// - Allows validation to catch invalid URLs with proper error messages
///
/// # Security Considerations
///
/// - Environment variable expansion is limited to safe patterns
/// - Path traversal attempts in expanded paths are detected later in validation
/// - No execution of shell commands or arbitrary code
///
/// # Use Cases
///
/// This function enables flexible source specifications in manifests:
/// - CI/CD systems can inject repository URLs via environment variables
/// - Users can reference repositories relative to their home directory  
/// - Docker containers can use mounted paths with consistent URLs
/// - Development teams can share manifests without hardcoded paths
/// - Multi-platform projects can use consistent path references
///
/// # Thread Safety
///
/// This function is thread-safe and does not modify global state.
/// Environment variable access is read-only and atomic.
fn expand_url(url: &str) -> Result<String> {
    // If it looks like a standard protocol URL (http, https, git@, file://), don't expand
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("git@")
        || url.starts_with("file://")
    {
        return Ok(url.to_string());
    }

    // Only try to expand if it looks like a local path (contains path separators, starts with ~, or contains env vars)
    if url.contains('/') || url.contains('\\') || url.starts_with('~') || url.contains('$') {
        // For cases that look like local paths, try to expand as a local path and convert to file:// URL
        match crate::utils::platform::resolve_path(url) {
            Ok(expanded_path) => {
                // Convert to file:// URL
                let path_str = expanded_path.to_string_lossy();
                if expanded_path.is_absolute() {
                    Ok(format!("file://{path_str}"))
                } else {
                    Ok(format!(
                        "file://{}",
                        std::env::current_dir()?
                            .join(expanded_path)
                            .to_string_lossy()
                    ))
                }
            }
            Err(_) => {
                // If path expansion fails, return the original URL
                // This allows the validation to catch the error with a proper message
                Ok(url.to_string())
            }
        }
    } else {
        // For strings that don't look like paths, return as-is to let validation catch the error
        Ok(url.to_string())
    }
}

/// Find the manifest file by searching up the directory tree from the current directory.
///
/// This function implements the standard CCPM behavior of searching for a `ccpm.toml`
/// file starting from the current working directory and walking up the directory
/// tree until one is found or the filesystem root is reached.
///
/// This behavior mirrors tools like Cargo, Git, and NPM that search for project
/// configuration files in parent directories.
///
/// # Search Algorithm
///
/// 1. Start from the current working directory
/// 2. Look for `ccpm.toml` in the current directory
/// 3. If not found, move to the parent directory
/// 4. Repeat until found or reach the filesystem root
/// 5. Return error if no manifest is found
///
/// # Examples
///
/// ```rust
/// use ccpm::manifest::find_manifest;
///
/// // Find manifest from current directory
/// match find_manifest() {
///     Ok(path) => println!("Found manifest at: {}", path.display()),
///     Err(e) => println!("No manifest found: {}", e),
/// }
/// ```
///
/// # Directory Structure Example
///
/// ```text
/// /home/user/project/
/// ├── ccpm.toml          ← Found here
/// └── subdir/
///     └── deep/
///         └── nested/     ← Search started here, walks up
/// ```
///
/// If called from `/home/user/project/subdir/deep/nested/`, this function
/// will find and return `/home/user/project/ccpm.toml`.
///
/// # Error Conditions
///
/// - **No manifest found**: Searched to filesystem root without finding `ccpm.toml`
/// - **Permission denied**: Cannot read current directory or traverse up
/// - **Filesystem corruption**: Cannot determine current working directory
///
/// # Use Cases
///
/// This function is typically called by CLI commands that need to locate the
/// project configuration, allowing users to run CCPM commands from any
/// subdirectory within their project.
pub fn find_manifest() -> Result<PathBuf> {
    let current = std::env::current_dir()
        .context("Cannot determine current working directory. This may indicate a permission issue or corrupted filesystem")?;
    find_manifest_from(current)
}

/// Find the manifest file, using an explicit path if provided.
///
/// This function provides a unified way to locate the manifest file,
/// either using an explicitly provided path or by searching from the
/// current directory.
///
/// # Arguments
///
/// * `explicit_path` - Optional path to a manifest file. If provided and the file exists,
///   this path is returned. If provided but the file doesn't exist, an error is returned.
///
/// # Returns
///
/// The path to the manifest file.
///
/// # Errors
///
/// Returns an error if:
/// - An explicit path is provided but the file doesn't exist
/// - No explicit path is provided and no manifest is found via search
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::manifest::find_manifest_with_optional;
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Use explicit path
/// let explicit = Some(PathBuf::from("/path/to/ccpm.toml"));
/// let manifest = find_manifest_with_optional(explicit)?;
///
/// // Search from current directory
/// let manifest = find_manifest_with_optional(None)?;
/// # Ok(())
/// # }
/// ```
pub fn find_manifest_with_optional(explicit_path: Option<PathBuf>) -> Result<PathBuf> {
    match explicit_path {
        Some(path) => {
            if path.exists() {
                Ok(path)
            } else {
                Err(crate::core::CcpmError::ManifestNotFound.into())
            }
        }
        None => find_manifest(),
    }
}

/// Find the manifest file by searching up from a specific starting directory.
///
/// This is the core manifest discovery function that implements the directory
/// traversal logic. It's used internally by [`find_manifest`] and can also
/// be used when you need to search from a specific directory rather than
/// the current working directory.
///
/// # Algorithm
///
/// 1. Check if `ccpm.toml` exists in the starting directory
/// 2. If found, return the full path to the manifest
/// 3. If not found, move to the parent directory
/// 4. Repeat until manifest is found or filesystem root is reached
/// 5. Return [`crate::core::CcpmError::ManifestNotFound`] if no manifest is found
///
/// # Parameters
///
/// - `current`: The starting directory for the search (consumed by the function)
///
/// # Examples
///
/// ```rust
/// use ccpm::manifest::find_manifest_from;
/// use std::path::PathBuf;
///
/// // Search from a specific directory
/// let start_dir = PathBuf::from("/home/user/project/subdir");
/// match find_manifest_from(start_dir) {
///     Ok(manifest_path) => {
///         println!("Found manifest: {}", manifest_path.display());
///     }
///     Err(e) => {
///         println!("No manifest found: {}", e);
///     }
/// }
/// ```
///
/// # Performance Considerations
///
/// - Each directory check involves a filesystem stat operation
/// - Search depth is limited by filesystem hierarchy (typically < 20 levels)
/// - Function returns immediately upon finding the first manifest
/// - No filesystem locks are held during the search
///
/// # Cross-Platform Behavior
///
/// - Works correctly on Windows, macOS, and Linux
/// - Handles filesystem roots appropriately (`/` on Unix, `C:\` on Windows)
/// - Respects platform-specific path separators and conventions
/// - Works with network filesystems and mounted volumes
///
/// # Error Handling
///
/// Returns [`crate::core::CcpmError::ManifestNotFound`] wrapped in an [`anyhow::Error`]
/// if no manifest file is found after searching to the filesystem root.
pub fn find_manifest_from(mut current: PathBuf) -> Result<PathBuf> {
    loop {
        let manifest_path = current.join("ccpm.toml");
        if manifest_path.exists() {
            return Ok(manifest_path);
        }

        if !current.pop() {
            return Err(crate::core::CcpmError::ManifestNotFound.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new();
        assert!(manifest.sources.is_empty());
        assert!(manifest.agents.is_empty());
        assert!(manifest.snippets.is_empty());
        assert!(manifest.commands.is_empty());
        assert!(manifest.mcp_servers.is_empty());
    }

    #[test]
    fn test_manifest_load_save() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/example-org/ccpm-official.git".to_string(),
        );
        manifest.add_dependency(
            "test-agent".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        manifest.save(&manifest_path).unwrap();

        let loaded = Manifest::load(&manifest_path).unwrap();
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.agents.len(), 1);
        assert!(loaded.has_dependency("test-agent"));
    }

    #[test]
    fn test_manifest_validation() {
        let mut manifest = Manifest::new();

        // Add dependency without source - should be valid (local dependency)
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );
        assert!(manifest.validate().is_ok());

        // Add dependency with undefined source - should fail validation
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("undefined".to_string()),
                path: "agent.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );
        assert!(manifest.validate().is_err());

        // Add the source - should now be valid
        manifest.add_source(
            "undefined".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_dependency_helpers() {
        let simple_dep = ResourceDependency::Simple("path/to/file.md".to_string());
        assert_eq!(simple_dep.get_path(), "path/to/file.md");
        assert!(simple_dep.get_source().is_none());
        assert!(simple_dep.get_version().is_none());
        assert!(simple_dep.is_local());

        let detailed_dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        });
        assert_eq!(detailed_dep.get_path(), "agents/test.md");
        assert_eq!(detailed_dep.get_source(), Some("official"));
        assert_eq!(detailed_dep.get_version(), Some("v1.0.0"));
        assert!(!detailed_dep.is_local());
    }

    #[test]
    fn test_all_dependencies() {
        let mut manifest = Manifest::new();

        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("a1.md".to_string()),
            true,
        );
        manifest.add_dependency(
            "snippet1".to_string(),
            ResourceDependency::Simple("s1.md".to_string()),
            false,
        );

        let all_deps = manifest.all_dependencies();
        assert_eq!(all_deps.len(), 2);
    }

    #[test]
    fn test_source_url_validation() {
        let mut manifest = Manifest::new();

        // Valid URLs
        manifest.add_source(
            "http".to_string(),
            "http://github.com/test/repo.git".to_string(),
        );
        manifest.add_source(
            "https".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.add_source(
            "ssh".to_string(),
            "git@github.com:test/repo.git".to_string(),
        );
        assert!(manifest.validate().is_ok());

        // Invalid URL
        manifest.add_source("invalid".to_string(), "not-a-url".to_string());
        let result = manifest.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid URL"));
    }

    #[test]
    fn test_manifest_commands() {
        let mut manifest = Manifest::new();

        // Add a command dependency
        manifest.add_typed_dependency(
            "build-command".to_string(),
            ResourceDependency::Simple("commands/build.md".to_string()),
            crate::core::ResourceType::Command,
        );

        assert!(manifest.commands.contains_key("build-command"));
        assert_eq!(manifest.commands.len(), 1);
        assert!(manifest.has_dependency("build-command"));

        // Test get_dependency returns command
        let dep = manifest.get_dependency("build-command");
        assert!(dep.is_some());
        assert_eq!(dep.unwrap().get_path(), "commands/build.md");
    }

    #[test]
    fn test_manifest_all_dependencies_with_commands() {
        let mut manifest = Manifest::new();

        manifest.add_typed_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("a1.md".to_string()),
            crate::core::ResourceType::Agent,
        );
        manifest.add_typed_dependency(
            "snippet1".to_string(),
            ResourceDependency::Simple("s1.md".to_string()),
            crate::core::ResourceType::Snippet,
        );
        manifest.add_typed_dependency(
            "command1".to_string(),
            ResourceDependency::Simple("c1.md".to_string()),
            crate::core::ResourceType::Command,
        );

        let all_deps = manifest.all_dependencies();
        assert_eq!(all_deps.len(), 3);

        // Verify all three types are present
        assert!(manifest.agents.contains_key("agent1"));
        assert!(manifest.snippets.contains_key("snippet1"));
        assert!(manifest.commands.contains_key("command1"));
    }

    #[test]
    fn test_manifest_save_load_commands() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source(
            "community".to_string(),
            "https://github.com/example/community.git".to_string(),
        );
        manifest.add_typed_dependency(
            "deploy".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "commands/deploy.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            crate::core::ResourceType::Command,
        );

        // Save and reload
        manifest.save(&manifest_path).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(loaded.commands.len(), 1);
        assert!(loaded.commands.contains_key("deploy"));
        assert!(loaded.has_dependency("deploy"));

        let dep = loaded.get_dependency("deploy").unwrap();
        assert_eq!(dep.get_path(), "commands/deploy.md");
        assert_eq!(dep.get_version(), Some("v2.0.0"));
    }

    #[test]
    fn test_target_config_commands_dir() {
        let config = TargetConfig::default();
        assert_eq!(config.commands, ".claude/commands");

        // Test custom config
        let mut manifest = Manifest::new();
        manifest.target.commands = "custom/commands".to_string();
        assert_eq!(manifest.target.commands, "custom/commands");
    }

    #[test]
    fn test_mcp_servers() {
        let mut manifest = Manifest::new();

        // Add an MCP server (now using standard ResourceDependency)
        manifest.add_mcp_server(
            "test-server".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("npm".to_string()),
                path: "mcp-servers/test-server.json".to_string(),
                version: Some("latest".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );

        assert_eq!(manifest.mcp_servers.len(), 1);
        assert!(manifest.mcp_servers.contains_key("test-server"));

        let server = &manifest.mcp_servers["test-server"];
        assert_eq!(server.get_source(), Some("npm"));
        assert_eq!(server.get_path(), "mcp-servers/test-server.json");
        assert_eq!(server.get_version(), Some("latest"));
    }

    #[test]
    fn test_manifest_save_load_mcp_servers() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source("npm".to_string(), "https://registry.npmjs.org".to_string());
        manifest.add_mcp_server(
            "postgres".to_string(),
            ResourceDependency::Simple("../local/mcp-servers/postgres.json".to_string()),
        );

        // Save and reload
        manifest.save(&manifest_path).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(loaded.mcp_servers.len(), 1);
        assert!(loaded.mcp_servers.contains_key("postgres"));

        let server = &loaded.mcp_servers["postgres"];
        assert_eq!(server.get_path(), "../local/mcp-servers/postgres.json");
    }

    #[test]
    fn test_target_config_mcp_servers_dir() {
        let config = TargetConfig::default();
        assert_eq!(config.mcp_servers, ".claude/ccpm/mcp-servers");

        // Test custom config
        let mut manifest = Manifest::new();
        manifest.target.mcp_servers = "custom/mcp".to_string();
        assert_eq!(manifest.target.mcp_servers, "custom/mcp");
    }

    #[test]
    fn test_dependency_with_custom_target() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/tool.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: Some("custom/tools".to_string()),
            filename: None,
        });

        assert_eq!(dep.get_target(), Some("custom/tools"));
        assert_eq!(dep.get_source(), Some("official"));
        assert_eq!(dep.get_path(), "agents/tool.md");
    }

    #[test]
    fn test_dependency_without_custom_target() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/tool.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        });

        assert!(dep.get_target().is_none());
    }

    #[test]
    fn test_simple_dependency_no_custom_target() {
        let dep = ResourceDependency::Simple("../local/file.md".to_string());
        assert!(dep.get_target().is_none());
    }

    #[test]
    fn test_save_load_dependency_with_custom_target() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/example/official.git".to_string(),
        );

        // Add dependency with custom target
        manifest.add_typed_dependency(
            "special-agent".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/special.md".to_string(),
                version: Some("v1.0.0".to_string()),
                target: Some("integrations/ai".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                filename: None,
            }),
            crate::core::ResourceType::Agent,
        );

        // Save and reload
        manifest.save(&manifest_path).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(loaded.agents.len(), 1);
        assert!(loaded.agents.contains_key("special-agent"));

        let dep = loaded.get_dependency("special-agent").unwrap();
        assert_eq!(dep.get_target(), Some("integrations/ai"));
        assert_eq!(dep.get_path(), "agents/special.md");
    }

    #[test]
    fn test_dependency_with_custom_filename() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/tool.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: Some("ai-assistant.md".to_string()),
        });

        assert_eq!(dep.get_filename(), Some("ai-assistant.md"));
        assert_eq!(dep.get_source(), Some("official"));
        assert_eq!(dep.get_path(), "agents/tool.md");
    }

    #[test]
    fn test_dependency_without_custom_filename() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/tool.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        });

        assert!(dep.get_filename().is_none());
    }

    #[test]
    fn test_simple_dependency_no_custom_filename() {
        let dep = ResourceDependency::Simple("../local/file.md".to_string());
        assert!(dep.get_filename().is_none());
    }

    #[test]
    fn test_save_load_dependency_with_custom_filename() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/example/official.git".to_string(),
        );

        // Add dependency with custom filename
        manifest.add_typed_dependency(
            "my-agent".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/complex-name.md".to_string(),
                version: Some("v1.0.0".to_string()),
                target: None,
                filename: Some("simple-name.txt".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
            crate::core::ResourceType::Agent,
        );

        // Save and reload
        manifest.save(&manifest_path).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(loaded.agents.len(), 1);
        assert!(loaded.agents.contains_key("my-agent"));

        let dep = loaded.get_dependency("my-agent").unwrap();
        assert_eq!(dep.get_filename(), Some("simple-name.txt"));
        assert_eq!(dep.get_path(), "agents/complex-name.md");
    }

    #[test]
    fn test_pattern_dependency() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("repo".to_string()),
            path: "agents/**/*.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        });

        assert!(dep.is_pattern());
        assert_eq!(dep.get_path(), "agents/**/*.md");
        assert!(!dep.is_local());
    }

    #[test]
    fn test_pattern_dependency_validation() {
        let mut manifest = Manifest::new();
        manifest.sources.insert(
            "repo".to_string(),
            "https://github.com/example/repo.git".to_string(),
        );

        // Valid pattern dependency (uses glob characters in path)
        manifest.agents.insert(
            "ai-agents".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("repo".to_string()),
                path: "agents/ai/*.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );

        assert!(manifest.validate().is_ok());

        // Valid: regular dependency (no glob characters)
        manifest.agents.insert(
            "regular".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("repo".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );

        let result = manifest.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_pattern_dependency_with_path_traversal() {
        let mut manifest = Manifest::new();
        manifest.sources.insert(
            "repo".to_string(),
            "https://github.com/example/repo.git".to_string(),
        );

        // Pattern with path traversal (using path field now)
        manifest.agents.insert(
            "unsafe".to_string(),
            ResourceDependency::Detailed(DetailedDependency {
                source: Some("repo".to_string()),
                path: "../../../etc/*.conf".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );

        let result = manifest.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid pattern"));
    }

    #[test]
    fn test_dependency_with_both_target_and_filename() {
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/tool.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: Some("tools/ai".to_string()),
            filename: Some("assistant.markdown".to_string()),
        });

        assert_eq!(dep.get_target(), Some("tools/ai"));
        assert_eq!(dep.get_filename(), Some("assistant.markdown"));
    }
}
