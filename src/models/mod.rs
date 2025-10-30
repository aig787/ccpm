//! Shared data models for AGPM operations
//!
//! This module provides reusable data structures that are used across
//! different CLI commands and core operations, ensuring consistency
//! and reducing code duplication.

use clap::Args;
use serde::{Deserialize, Serialize};

/// Common dependency specification used across commands
#[derive(Debug, Clone, Args)]
pub struct DependencySpec {
    /// Dependency specification string
    ///
    /// Format: `source:path[@version]` for Git sources or path for local files
    ///
    /// GIT DEPENDENCIES (from a repository source defined in `[sources]`):
    ///   source:path@version       - Specific version (tag/branch/commit)
    ///   source:path               - Defaults to "main" branch
    ///
    /// Examples:
    ///   official:agents/code-reviewer.md@v1.0.0    - Specific version tag
    ///   community:snippets/python-utils.md@main    - Branch name
    ///   myrepo:commands/deploy.md@abc123f          - Commit SHA
    ///   community:hooks/pre-commit.json            - Defaults to "main"
    ///
    /// LOCAL FILE DEPENDENCIES:
    ///   ./path/file.md            - Relative to current directory
    ///   ../path/file.md           - Parent directory
    ///   /absolute/path/file.md    - Absolute path (Unix/macOS)
    ///   C:\path\file.md           - Absolute path (Windows)
    ///
    /// Examples:
    ///   ./agents/my-agent.md                       - Project agent
    ///   ../shared-resources/common-snippet.md      - Shared resource
    ///   /usr/local/share/agpm/hooks/lint.json      - System-wide hook
    ///
    /// PATTERN DEPENDENCIES (glob patterns for multiple files):
    ///   source:dir/*.md@version   - All .md files in directory
    ///   source:dir/**/*.md        - All .md files recursively
    ///
    /// Examples:
    ///   community:agents/ai/*.md@v2.0.0            - All AI agents
    ///   official:agents/**/review*.md@v1.5.0       - All review agents (recursive)
    ///   ./local-agents/*.md                        - All local agents
    ///
    /// Notes:
    /// - Version is optional for Git sources (defaults to "main")
    /// - Version is not applicable for local file paths
    /// - Use --name to specify a custom dependency name
    /// - Patterns require --name to provide a meaningful dependency name
    #[arg(
        value_name = "SPEC",
        help = "Dependency spec: 'source:path@version' for Git (e.g., community:agents/helper.md@v1.0.0) or './path' for local files. Use --help for more examples"
    )]
    pub spec: String,

    /// Custom name for the dependency
    ///
    /// If not provided, the name will be derived from the file path.
    /// This allows for more descriptive or shorter names in the manifest.
    #[arg(long)]
    pub name: Option<String>,

    /// Target tool for the dependency
    ///
    /// Specifies which AI coding tool this resource is for.
    /// Supported values: claude-code, opencode, agpm
    ///
    /// Examples:
    ///   --tool claude-code  - Install to .claude/ (default for agents, commands, scripts, hooks)
    ///   --tool opencode     - Install to .opencode/
    ///   --tool agpm         - Install to .agpm/ (default for snippets)
    #[arg(long)]
    pub tool: Option<String>,

    /// Custom installation target path (relative to resource directory)
    ///
    /// Override the default installation path. The path is relative to the
    /// resource type's default directory (e.g., .claude/agents/).
    ///
    /// IMPORTANT: Since v0.3.18+, custom targets are relative to the resource
    /// directory, not the project root.
    ///
    /// Examples:
    ///   --target custom/special.md       - Install to .claude/agents/custom/special.md
    ///   --target experimental/test.md    - Install to .claude/commands/experimental/test.md
    #[arg(long)]
    pub target: Option<String>,

    /// Custom filename for the installed resource
    ///
    /// Override the default filename derived from the source path.
    /// Use this to rename resources during installation.
    ///
    /// Examples:
    ///   --filename my-reviewer.md    - Install as my-reviewer.md instead of original name
    ///   --filename helper.json       - Rename JSON file during installation
    #[arg(long)]
    pub filename: Option<String>,

    /// Force overwrite if dependency exists
    ///
    /// By default, adding a duplicate dependency will fail.
    /// Use this flag to replace existing dependencies.
    #[arg(long, short = 'f')]
    pub force: bool,

    /// Skip automatic installation after adding dependency
    ///
    /// By default, the dependency is automatically installed after being added
    /// to the manifest. Use this flag to only update the manifest without
    /// installing the dependency files.
    ///
    /// Examples:
    ///   --no-install    - Add to manifest only, skip installation
    #[arg(long)]
    pub no_install: bool,
}

/// Arguments for adding an agent dependency
#[derive(Debug, Clone, Args)]
pub struct AgentDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a snippet dependency
#[derive(Debug, Clone, Args)]
pub struct SnippetDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a command dependency
#[derive(Debug, Clone, Args)]
pub struct CommandDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding an MCP server dependency
#[derive(Debug, Clone, Args)]
pub struct McpServerDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Enum representing all possible dependency types
#[derive(Debug, Clone)]
pub enum DependencyType {
    /// An agent dependency
    Agent(AgentDependency),
    /// A snippet dependency
    Snippet(SnippetDependency),
    /// A command dependency
    Command(CommandDependency),
    /// A script dependency
    Script(ScriptDependency),
    /// A hook dependency
    Hook(HookDependency),
    /// An MCP server dependency
    McpServer(McpServerDependency),
    /// A skill dependency
    Skill(SkillDependency),
}

impl DependencyType {
    /// Get the common dependency specification
    #[must_use]
    pub const fn common(&self) -> &DependencySpec {
        match self {
            Self::Agent(dep) => &dep.common,
            Self::Snippet(dep) => &dep.common,
            Self::Command(dep) => &dep.common,
            Self::Script(dep) => &dep.common,
            Self::Hook(dep) => &dep.common,
            Self::McpServer(dep) => &dep.common,
            Self::Skill(dep) => &dep.common,
        }
    }

    /// Get the resource type as a string
    #[must_use]
    pub const fn resource_type(&self) -> &'static str {
        match self {
            Self::Agent(_) => "agent",
            Self::Snippet(_) => "snippet",
            Self::Command(_) => "command",
            Self::Script(_) => "script",
            Self::Hook(_) => "hook",
            Self::McpServer(_) => "mcp-server",
            Self::Skill(_) => "skill",
        }
    }
}

/// Source repository specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    /// Name for the source
    pub name: String,

    /// Git repository URL
    pub url: String,
}

/// Resource installation options
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Skip installation, only update lockfile
    pub no_install: bool,

    /// Force reinstallation even if up to date
    pub force: bool,

    /// Suppress progress indicators
    pub quiet: bool,

    /// Use cached data only, don't fetch updates
    pub offline: bool,
}

/// Resource update options
#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Update all dependencies
    pub all: bool,

    /// Specific dependencies to update
    pub dependencies: Vec<String>,

    /// Allow updating to incompatible versions
    pub breaking: bool,

    /// Suppress progress indicators
    pub quiet: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_type_common() {
        let agent = DependencyType::Agent(AgentDependency {
            common: DependencySpec {
                spec: "test:agent.md".to_string(),
                name: None,
                tool: None,
                target: None,
                filename: None,
                force: false,
                no_install: false,
            },
        });

        assert_eq!(agent.common().spec, "test:agent.md");
        assert_eq!(agent.resource_type(), "agent");
    }

    #[test]
    fn test_mcp_server_dependency() {
        let mcp = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "test:mcp.toml".to_string(),
                name: Some("test-server".to_string()),
                tool: None,
                target: None,
                filename: None,
                force: true,
                no_install: false,
            },
        });

        assert_eq!(mcp.common().spec, "test:mcp.toml");
        assert_eq!(mcp.common().name, Some("test-server".to_string()));
        assert!(mcp.common().force);
        assert_eq!(mcp.resource_type(), "mcp-server");
    }
}

/// Arguments for adding a script dependency
#[derive(Debug, Clone, Args)]
pub struct ScriptDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a hook dependency
#[derive(Debug, Clone, Args)]
pub struct HookDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a skill dependency
#[derive(Debug, Clone, Args)]
pub struct SkillDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}
