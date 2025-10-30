//! Tool configuration types for multi-tool support.
//!
//! This module defines the types and structures used to configure different AI coding
//! assistant tools (Claude Code, OpenCode, AGPM, and custom tools) in AGPM manifests.
//!
//! # Overview
//!
//! AGPM supports multiple AI coding tools through a flexible configuration system:
//! - **Claude Code**: The primary AI coding assistant (enabled by default)
//! - **OpenCode**: Alternative AI coding assistant (enabled by default for consistency)
//! - **AGPM**: Internal tool for shared infrastructure like snippets (enabled by default)
//! - **Custom Tools**: User-defined tools with custom configurations (enabled by default)
//!
//! # Tool Configuration
//!
//! Each tool defines:
//! - A base directory (e.g., `.claude`, `.opencode`, `.agpm`)
//! - Resource type mappings (agents, commands, snippets, etc.)
//! - Installation paths or merge targets for each resource type
//! - Default flatten behavior for directory structure preservation
//! - An enabled/disabled state
//!
//! # Key Types
//!
//! - [`WellKnownTool`]: Enum representing officially supported tools
//! - [`ResourceConfig`]: Configuration for a specific resource type within a tool
//! - [`ArtifactTypeConfig`]: Complete configuration for a tool
//! - [`ToolsConfig`]: Top-level configuration mapping tool names to their configs
//!
//! # Examples
//!
//! ```toml
//! [tools.claude-code]
//! path = ".claude"
//! enabled = true
//!
//! [tools.claude-code.resources.agents]
//! path = "agents"
//! flatten = true
//!
//! [tools.opencode]
//! path = ".opencode"
//! enabled = true   # Enabled by default
//!
//! [tools.opencode.resources.agents]
//! path = "agent"  # Singular in OpenCode
//! flatten = true
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

// Cached default configuration to avoid repeated allocations
static DEFAULT_TOOLS_CONFIG: OnceLock<ToolsConfig> = OnceLock::new();

/// Resource configuration within a tool.
///
/// Defines the installation path for a specific resource type within a tool.
/// Resources can either:
/// - Install to a subdirectory (via `path`)
/// - Merge into a configuration file (via `merge_target`)
///
/// At least one of `path` or `merge_target` should be set for a resource type
/// to be considered supported by a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceConfig {
    /// Subdirectory path for this resource type relative to the tool's base directory.
    ///
    /// Used for resources that install as separate files (agents, snippets, commands, scripts).
    /// When None, this resource type either uses merge_target or is not supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Target configuration file for merging this resource type.
    ///
    /// Used for resources that merge into configuration files (hooks, MCP servers).
    /// The path is relative to the project root.
    ///
    /// # Examples
    ///
    /// - Hooks: `.claude/settings.local.json`
    /// - MCP servers: `.mcp.json` or `.opencode/opencode.json`
    #[serde(skip_serializing_if = "Option::is_none", rename = "merge-target")]
    pub merge_target: Option<String>,

    /// Default flatten behavior for this resource type.
    ///
    /// When `true`: Only the filename is used for installation (e.g., `nested/dir/file.md` → `file.md`)
    /// When `false`: Full relative path is preserved (e.g., `nested/dir/file.md` → `nested/dir/file.md`)
    ///
    /// This default can be overridden per-dependency using the `flatten` field.
    /// If not specified, defaults to `false` (preserve directory structure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatten: Option<bool>,
}

/// Well-known tool types with specific default behaviors.
///
/// This enum represents the officially supported tools and their
/// specific default configurations, particularly for the `enabled` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WellKnownTool {
    /// Claude Code - the primary AI coding assistant tool.
    /// Enabled by default since most users rely on Claude Code.
    ClaudeCode,

    /// OpenCode - an alternative AI coding assistant tool.
    /// Enabled by default for consistency with other tools.
    OpenCode,

    /// AGPM - internal tool for shared infrastructure (snippets).
    /// Enabled by default for backward compatibility and shared resources.
    Agpm,

    /// Generic/custom tools not in the well-known set.
    /// Enabled by default for backward compatibility.
    Generic,
}

impl WellKnownTool {
    /// Identifies a well-known tool from its string name.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool (e.g., "claude-code", "opencode", "agpm")
    ///
    /// # Returns
    ///
    /// The corresponding `WellKnownTool` variant, or `Generic` for custom tools.
    pub fn from_name(tool_name: &str) -> Self {
        match tool_name {
            "claude-code" => WellKnownTool::ClaudeCode,
            "opencode" => WellKnownTool::OpenCode,
            "agpm" => WellKnownTool::Agpm,
            _ => WellKnownTool::Generic,
        }
    }

    /// Returns the default `enabled` value for this tool.
    ///
    /// # Default Values
    ///
    /// - **Claude Code**: `true` (most users rely on it)
    /// - **OpenCode**: `true` (enabled by default for consistency)
    /// - **AGPM**: `true` (shared infrastructure)
    /// - **Generic**: `true` (backward compatibility)
    pub const fn default_enabled(self) -> bool {
        match self {
            WellKnownTool::ClaudeCode => true,
            WellKnownTool::OpenCode => true,
            WellKnownTool::Agpm => true,
            WellKnownTool::Generic => true,
        }
    }
}

/// Tool configuration (internal deserialization structure).
///
/// This is used during deserialization to capture optional fields.
/// The public API uses `ArtifactTypeConfig` with required `enabled` field.
#[derive(Debug, Clone, Deserialize)]
struct ArtifactTypeConfigRaw {
    /// Base directory for this tool (e.g., ".claude", ".opencode", ".agpm")
    path: PathBuf,

    /// Map of resource type -> configuration
    #[serde(default)]
    resources: HashMap<String, ResourceConfig>,

    /// Whether this tool is enabled (optional during deserialization)
    ///
    /// When None, the tool-specific default will be applied based on the tool name.
    #[serde(default)]
    enabled: Option<bool>,
}

/// Tool configuration.
///
/// Defines how a specific tool (e.g., claude-code, opencode, agpm)
/// organizes its resources. Each tool has a base directory and
/// a map of resource types to their subdirectory configurations.
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactTypeConfig {
    /// Base directory for this tool (e.g., ".claude", ".opencode", ".agpm")
    pub path: PathBuf,

    /// Map of resource type -> configuration
    pub resources: HashMap<String, ResourceConfig>,

    /// Whether this tool is enabled.
    ///
    /// When disabled, dependencies for this tool will not be resolved,
    /// installed, or included in the lockfile.
    ///
    /// # Defaults
    ///
    /// - **claude-code**: `true` (most users rely on it)
    /// - **opencode**: `true` (enabled by default for consistency)
    /// - **agpm**: `true` (shared infrastructure)
    /// - **custom tools**: `true` (backward compatibility)
    pub enabled: bool,
}

/// Top-level tools configuration.
///
/// Maps tool type names to their configurations. This replaces the old
/// `[target]` section and enables multi-tool support.
#[derive(Debug, Clone, Serialize)]
pub struct ToolsConfig {
    /// Map of tool type name -> configuration
    #[serde(flatten)]
    pub types: HashMap<String, ArtifactTypeConfig>,
}

/// Custom deserializer that merges user configuration with built-in defaults.
///
/// # Merging Behavior
///
/// For **well-known tools** (claude-code, opencode, agpm):
/// - Starts with built-in default resource configurations
/// - User-provided resources override defaults on a per-resource-type basis
/// - Missing resource types automatically use defaults
///
/// For **custom tools**:
/// - No default merging occurs (user config used as-is)
/// - User must provide complete configuration
///
/// # Example
///
/// ```toml
/// [tools.opencode]
/// path = ".opencode"
/// resources = { commands = { path = "command", flatten = true } }
/// # Snippets not specified - will use default from built-in config
/// ```
///
/// After deserialization, opencode will have:
/// - `commands`: User config (overrides default)
/// - `snippets`: Default config (auto-merged)
impl<'de> serde::Deserialize<'de> for ToolsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First deserialize into the raw structure with Option<bool> for enabled
        let raw_types: HashMap<String, ArtifactTypeConfigRaw> = HashMap::deserialize(deserializer)?;

        // Get default configurations for merging (cached)
        let defaults = DEFAULT_TOOLS_CONFIG.get_or_init(ToolsConfig::default);

        // Convert to the final structure, applying tool-specific defaults
        let types = raw_types
            .into_iter()
            .map(|(tool_name, raw_config)| {
                // Determine the enabled value:
                // - If explicitly set in TOML, use that value
                // - Otherwise, use the tool-specific default
                let well_known_tool = WellKnownTool::from_name(&tool_name);
                let enabled =
                    raw_config.enabled.unwrap_or_else(|| well_known_tool.default_enabled());

                // Merge resources: start with defaults, then overlay user config
                let merged_resources = if let Some(default_config) = defaults.types.get(&tool_name)
                {
                    let mut resources = default_config.resources.clone();
                    // User-provided resources override defaults
                    resources.extend(raw_config.resources);
                    resources
                } else {
                    // No defaults for this tool (custom tool), use as-is
                    raw_config.resources
                };

                let config = ArtifactTypeConfig {
                    path: raw_config.path,
                    resources: merged_resources,
                    enabled,
                };

                (tool_name, config)
            })
            .collect();

        Ok(ToolsConfig {
            types,
        })
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        use crate::core::ResourceType;
        let mut types = HashMap::new();

        // Claude Code configuration
        let mut claude_resources = HashMap::new();
        claude_resources.insert(
            ResourceType::Agent.to_plural().to_string(),
            ResourceConfig {
                path: Some("agents".to_string()),
                merge_target: None,
                flatten: Some(true), // Agents flatten by default
            },
        );
        claude_resources.insert(
            ResourceType::Snippet.to_plural().to_string(),
            ResourceConfig {
                path: Some("snippets".to_string()),
                merge_target: None,
                flatten: Some(false), // Snippets preserve directory structure
            },
        );
        claude_resources.insert(
            ResourceType::Command.to_plural().to_string(),
            ResourceConfig {
                path: Some("commands".to_string()),
                merge_target: None,
                flatten: Some(true), // Commands flatten by default
            },
        );
        claude_resources.insert(
            ResourceType::Script.to_plural().to_string(),
            ResourceConfig {
                path: Some("scripts".to_string()),
                merge_target: None,
                flatten: Some(false), // Scripts preserve directory structure
            },
        );
        claude_resources.insert(
            ResourceType::Hook.to_plural().to_string(),
            ResourceConfig {
                path: None, // Hooks are merged into configuration file
                merge_target: Some(".claude/settings.local.json".to_string()),
                flatten: None, // N/A for merge targets
            },
        );
        claude_resources.insert(
            ResourceType::McpServer.to_plural().to_string(),
            ResourceConfig {
                path: None, // MCP servers are merged into configuration file
                merge_target: Some(".mcp.json".to_string()),
                flatten: None, // N/A for merge targets
            },
        );
        claude_resources.insert(
            ResourceType::Skill.to_plural().to_string(),
            ResourceConfig {
                path: Some("skills".to_string()),
                merge_target: None,
                flatten: Some(true), // Skills flatten by default
            },
        );

        types.insert(
            "claude-code".to_string(),
            ArtifactTypeConfig {
                path: PathBuf::from(".claude"),
                resources: claude_resources,
                enabled: WellKnownTool::ClaudeCode.default_enabled(),
            },
        );

        // OpenCode configuration
        let mut opencode_resources = HashMap::new();
        opencode_resources.insert(
            ResourceType::Agent.to_plural().to_string(),
            ResourceConfig {
                path: Some("agent".to_string()), // Singular
                merge_target: None,
                flatten: Some(true), // Agents flatten by default
            },
        );
        opencode_resources.insert(
            ResourceType::Snippet.to_plural().to_string(),
            ResourceConfig {
                path: Some("snippet".to_string()), // Singular
                merge_target: None,
                flatten: Some(false), // Snippets preserve directory structure
            },
        );
        opencode_resources.insert(
            ResourceType::Command.to_plural().to_string(),
            ResourceConfig {
                path: Some("command".to_string()), // Singular
                merge_target: None,
                flatten: Some(true), // Commands flatten by default
            },
        );
        opencode_resources.insert(
            ResourceType::McpServer.to_plural().to_string(),
            ResourceConfig {
                path: None, // MCP servers are merged into configuration file
                merge_target: Some(".opencode/opencode.json".to_string()),
                flatten: None, // N/A for merge targets
            },
        );

        types.insert(
            "opencode".to_string(),
            ArtifactTypeConfig {
                path: PathBuf::from(".opencode"),
                resources: opencode_resources,
                enabled: WellKnownTool::OpenCode.default_enabled(),
            },
        );

        // AGPM configuration (snippets only)
        let mut agpm_resources = HashMap::new();
        agpm_resources.insert(
            ResourceType::Snippet.to_plural().to_string(),
            ResourceConfig {
                path: Some("snippets".to_string()),
                merge_target: None,
                flatten: Some(false), // Snippets preserve directory structure
            },
        );

        types.insert(
            "agpm".to_string(),
            ArtifactTypeConfig {
                path: PathBuf::from(".agpm"),
                resources: agpm_resources,
                enabled: WellKnownTool::Agpm.default_enabled(),
            },
        );

        Self {
            types,
        }
    }
}
