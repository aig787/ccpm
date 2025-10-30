//! Unit tests for tool configuration types and functionality.
//!
//! This module provides comprehensive test coverage for the tool configuration
//! system including WellKnownTool enum, ToolsConfig deserialization, and
//! default value handling.

use super::*;
use crate::manifest::tool_config::*;
use serde_json;
use std::collections::HashMap;
use toml;

mod well_known_tool {
    use super::*;

    #[test]
    fn test_from_name_valid_tools() {
        assert_eq!(WellKnownTool::from_name("claude-code"), WellKnownTool::ClaudeCode);
        assert_eq!(WellKnownTool::from_name("opencode"), WellKnownTool::OpenCode);
        assert_eq!(WellKnownTool::from_name("agpm"), WellKnownTool::Agpm);
    }

    #[test]
    fn test_from_name_custom_tools() {
        assert_eq!(WellKnownTool::from_name("custom-tool"), WellKnownTool::Generic);
        assert_eq!(WellKnownTool::from_name("my-tool"), WellKnownTool::Generic);
        assert_eq!(WellKnownTool::from_name("unknown"), WellKnownTool::Generic);
    }

    #[test]
    fn test_from_name_edge_cases() {
        assert_eq!(WellKnownTool::from_name(""), WellKnownTool::Generic);
        assert_eq!(WellKnownTool::from_name("CLAUDE-CODE"), WellKnownTool::Generic); // case sensitive
        assert_eq!(WellKnownTool::from_name("claude-code "), WellKnownTool::Generic); // whitespace
        assert_eq!(WellKnownTool::from_name(" claude-code"), WellKnownTool::Generic); // leading whitespace
    }

    #[test]
    fn test_default_enabled_values() {
        assert!(WellKnownTool::ClaudeCode.default_enabled());
        assert!(WellKnownTool::OpenCode.default_enabled());
        assert!(WellKnownTool::Agpm.default_enabled());
        assert!(WellKnownTool::Generic.default_enabled());
    }

    #[test]
    fn test_well_known_tool_properties() {
        // Test that WellKnownTool is Copy, Clone, PartialEq, Eq
        let tool1 = WellKnownTool::ClaudeCode;
        let tool2 = tool1; // Copy
        assert_eq!(tool1, tool2); // PartialEq
        assert_eq!(tool1, WellKnownTool::ClaudeCode); // Eq
    }
}

mod resource_config {
    use super::*;

    #[test]
    fn test_resource_config_serialization() {
        let config = ResourceConfig {
            path: Some("agents".to_string()),
            merge_target: None,
            flatten: Some(true),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ResourceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_resource_config_with_merge_target() {
        let config = ResourceConfig {
            path: None,
            merge_target: Some(".claude/settings.local.json".to_string()),
            flatten: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ResourceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_resource_config_minimal() {
        let config = ResourceConfig {
            path: None,
            merge_target: None,
            flatten: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ResourceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_resource_config_skip_serializing_if_none() {
        let config = ResourceConfig {
            path: Some("agents".to_string()),
            merge_target: None,
            flatten: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("merge_target"));
        assert!(!json.contains("flatten"));
        assert!(json.contains("path"));
    }
}

mod tools_config_deserialization {
    use super::*;

    #[test]
    fn test_deserialize_complete_tools_config() {
        let toml = r#"
claude-code = { path = ".claude", enabled = true, resources = { agents = { path = "agents", flatten = true }, hooks = { merge-target = ".claude/settings.local.json" } } }
opencode = { path = ".opencode", enabled = false, resources = { agents = { path = "agent" } } }
"#;

        let config: ToolsConfig = toml::from_str(toml).unwrap();

        // Check claude-code config
        let claude_config = config.types.get("claude-code").unwrap();
        assert_eq!(claude_config.path, PathBuf::from(".claude"));
        assert!(claude_config.enabled);

        let claude_agents = claude_config.resources.get("agents").unwrap();
        assert_eq!(claude_agents.path, Some("agents".to_string()));
        assert_eq!(claude_agents.flatten, Some(true));

        // Check opencode config
        let opencode_config = config.types.get("opencode").unwrap();
        assert_eq!(opencode_config.path, PathBuf::from(".opencode"));
        assert!(!opencode_config.enabled);
    }

    #[test]
    fn test_deserialize_with_default_enabled() {
        let toml = r#"
claude-code = { path = ".claude", resources = { agents = { path = "agents" } } }
opencode = { path = ".opencode", resources = { agents = { path = "agent" } } }
"#;

        let config: ToolsConfig = toml::from_str(toml).unwrap();

        // claude-code should default to enabled = true
        let claude_config = config.types.get("claude-code").unwrap();
        assert!(claude_config.enabled);

        // opencode should default to enabled = true
        let opencode_config = config.types.get("opencode").unwrap();
        assert!(opencode_config.enabled);
    }

    #[test]
    fn test_deserialize_explicit_enabled_override() {
        let toml = r#"
claude-code = { path = ".claude", enabled = false, resources = { agents = { path = "agents" } } }
opencode = { path = ".opencode", enabled = true, resources = { agents = { path = "agent" } } }
"#;

        let config: ToolsConfig = toml::from_str(toml).unwrap();

        // claude-code should be explicitly disabled
        let claude_config = config.types.get("claude-code").unwrap();
        assert!(!claude_config.enabled);

        // opencode should be explicitly enabled
        let opencode_config = config.types.get("opencode").unwrap();
        assert!(opencode_config.enabled);
    }

    #[test]
    fn test_deserialize_custom_tool() {
        let toml = r#"
my-custom-tool = { path = ".my-tool", enabled = true, resources = { agents = { path = "agents" } } }
"#;

        let config: ToolsConfig = toml::from_str(toml).unwrap();

        let custom_config = config.types.get("my-custom-tool").unwrap();
        assert_eq!(custom_config.path, PathBuf::from(".my-tool"));
        // Custom tools should default to enabled = true
        assert!(custom_config.enabled);
    }

    #[test]
    fn test_deserialize_empty_tools_config() {
        let toml = "";
        let config: ToolsConfig = toml::from_str(toml).unwrap();
        assert!(config.types.is_empty());
    }

    #[test]
    fn test_deserialize_invalid_toml() {
        let toml = r#"
[tools.claude-code
path = ".claude"  # Missing closing bracket
"#;

        let result: Result<ToolsConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_missing_required_path() {
        let toml = r#"
[tools.claude-code]
# path field missing

[tools.claude-code.resources.agents]
path = "agents"
"#;

        let result: Result<ToolsConfig, _> = toml::from_str(toml);
        // This should fail because path is required
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_roundtrip() {
        let original = ToolsConfig::default();
        let toml = toml::to_string_pretty(&original).unwrap();
        let deserialized: ToolsConfig = toml::from_str(&toml).unwrap();

        assert_eq!(original.types.len(), deserialized.types.len());

        // Check that all default tools are present
        for tool_name in ["claude-code", "opencode", "agpm"] {
            assert!(deserialized.types.contains_key(tool_name));
        }
    }
}

mod artifact_type_config {
    use super::*;

    #[test]
    fn test_artifact_type_config_creation() {
        let mut resources = HashMap::new();
        resources.insert(
            "agents".to_string(),
            ResourceConfig {
                path: Some("agents".to_string()),
                merge_target: None,
                flatten: Some(true),
            },
        );

        let config = ArtifactTypeConfig {
            path: PathBuf::from(".claude"),
            resources,
            enabled: true,
        };

        assert_eq!(config.path, PathBuf::from(".claude"));
        assert!(config.enabled);
        assert_eq!(config.resources.len(), 1);
    }

    #[test]
    fn test_artifact_type_config_serialization() {
        let mut resources = HashMap::new();
        resources.insert(
            "agents".to_string(),
            ResourceConfig {
                path: Some("agents".to_string()),
                merge_target: None,
                flatten: Some(true),
            },
        );

        let config = ArtifactTypeConfig {
            path: PathBuf::from(".claude"),
            resources,
            enabled: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        // Note: ArtifactTypeConfig doesn't implement Deserialize, so we'll skip this test
        // The serialization test above is sufficient for this type

        // Just verify the JSON can be created
        assert!(!json.is_empty());
    }
}

mod tools_config_default {
    use super::*;

    #[test]
    fn test_default_tools_config_structure() {
        let config = ToolsConfig::default();

        // Should have exactly 3 default tools
        assert_eq!(config.types.len(), 3);
        assert!(config.types.contains_key("claude-code"));
        assert!(config.types.contains_key("opencode"));
        assert!(config.types.contains_key("agpm"));
    }

    #[test]
    fn test_claude_code_defaults() {
        let config = ToolsConfig::default();
        let claude = config.types.get("claude-code").unwrap();

        assert_eq!(claude.path, PathBuf::from(".claude"));
        assert!(claude.enabled);

        // Check all expected resource types
        assert!(claude.resources.contains_key("agents"));
        assert!(claude.resources.contains_key("snippets"));
        assert!(claude.resources.contains_key("commands"));
        assert!(claude.resources.contains_key("scripts"));
        assert!(claude.resources.contains_key("hooks"));
        assert!(claude.resources.contains_key("mcp-servers"));

        // Check specific resource configurations
        let agents = claude.resources.get("agents").unwrap();
        assert_eq!(agents.path, Some("agents".to_string()));
        assert_eq!(agents.flatten, Some(true));

        let hooks = claude.resources.get("hooks").unwrap();
        assert_eq!(hooks.path, None);
        assert_eq!(hooks.merge_target, Some(".claude/settings.local.json".to_string()));
    }

    #[test]
    fn test_opencode_defaults() {
        let config = ToolsConfig::default();
        let opencode = config.types.get("opencode").unwrap();

        assert_eq!(opencode.path, PathBuf::from(".opencode"));
        assert!(opencode.enabled); // Enabled by default

        // OpenCode supports agents, commands, snippets, and mcp-servers
        assert!(opencode.resources.contains_key("agents"));
        assert!(opencode.resources.contains_key("commands"));
        assert!(opencode.resources.contains_key("snippets"));
        assert!(opencode.resources.contains_key("mcp-servers"));
        assert!(!opencode.resources.contains_key("scripts"));
        assert!(!opencode.resources.contains_key("hooks"));

        // Check singular paths for OpenCode
        let agents = opencode.resources.get("agents").unwrap();
        assert_eq!(agents.path, Some("agent".to_string())); // Singular

        let snippets = opencode.resources.get("snippets").unwrap();
        assert_eq!(snippets.path, Some("snippet".to_string())); // Singular
    }

    #[test]
    fn test_agpm_defaults() {
        let config = ToolsConfig::default();
        let agpm = config.types.get("agpm").unwrap();

        assert_eq!(agpm.path, PathBuf::from(".agpm"));
        assert!(agpm.enabled);

        // AGPM only supports snippets
        assert!(agpm.resources.contains_key("snippets"));
        assert_eq!(agpm.resources.len(), 1);

        let snippets = agpm.resources.get("snippets").unwrap();
        assert_eq!(snippets.path, Some("snippets".to_string()));
        assert_eq!(snippets.flatten, Some(false)); // Preserve structure
    }

    #[test]
    fn test_default_resource_config_consistency() {
        let config = ToolsConfig::default();

        // Verify that all resource types have proper configurations
        for (tool_name, tool_config) in &config.types {
            for (resource_name, resource_config) in &tool_config.resources {
                // Each resource should have either a path or merge_target
                assert!(
                    resource_config.path.is_some() || resource_config.merge_target.is_some(),
                    "Resource {} in tool {} has neither path nor merge_target",
                    resource_name,
                    tool_name
                );
            }
        }
    }
}

mod integration_tests {
    use super::*;

    #[test]
    fn test_complete_manifest_parsing() {
        let toml = r#"
[sources]
community = "https://github.com/example/community.git"

[tools.claude-code]
path = ".claude"

[tools.claude-code.resources.agents]
path = "agents"
flatten = true

[agents]
my-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
"#;

        // This should parse without errors
        let manifest: crate::manifest::Manifest = toml::from_str(toml).unwrap();

        assert!(manifest.tools.is_some());
        let tools = manifest.tools.unwrap();
        assert!(tools.types.contains_key("claude-code"));

        let claude = tools.types.get("claude-code").unwrap();
        assert!(claude.enabled);
    }

    #[test]
    fn test_tool_config_with_all_resource_types() {
        let toml = r#"
test-tool = { path = ".test", enabled = true, resources = { agents = { path = "agents", flatten = true }, snippets = { path = "snippets", flatten = false }, commands = { path = "commands", flatten = true }, scripts = { path = "scripts", flatten = false }, skills = { path = "skills", flatten = false }, hooks = { merge-target = ".test/settings.json" }, mcp-servers = { merge-target = ".test/mcp.json" } } }
"#;

        let config: ToolsConfig = toml::from_str(toml).unwrap();
        let tool = config.types.get("test-tool").unwrap();

        assert_eq!(tool.resources.len(), 7);

        // Verify file-based resources have paths
        for resource_type in ["agents", "snippets", "commands", "scripts", "skills"] {
            let resource = tool.resources.get(resource_type).unwrap();
            assert!(resource.path.is_some());
            assert!(resource.merge_target.is_none());
        }

        // Verify merge-based resources have merge targets
        for resource_type in ["hooks", "mcp-servers"] {
            let resource = tool.resources.get(resource_type).unwrap();
            assert!(resource.path.is_none());
            assert!(resource.merge_target.is_some());
        }
    }
}
