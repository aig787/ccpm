//! Extract dependency metadata from resource files.
//!
//! This module handles the extraction of transitive dependency information
//! from resource files. Supports YAML frontmatter in Markdown files and
//! JSON fields in JSON configuration files.
//!
//! # Template Support
//!
//! When a `ProjectConfig` is provided, frontmatter is rendered as a Tera template
//! before parsing. This allows dependency paths to reference project variables:
//!
//! ```yaml
//! dependencies:
//!   snippets:
//!     - path: standards/{{ agpm.project.language }}-guide.md
//! ```

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::path::Path;

use crate::core::OperationContext;
use crate::manifest::{DependencyMetadata, dependency_spec::AgpmMetadata};
use crate::markdown::frontmatter::FrontmatterParser;

/// Metadata extractor for resource files.
///
/// Extracts dependency information embedded in resource files:
/// - Markdown files (.md): YAML frontmatter between `---` delimiters
/// - JSON files (.json): `dependencies` field in the JSON structure
/// - Other files: No dependencies supported
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Extract dependency metadata from a file's content.
    ///
    /// Uses operation-scoped context for warning deduplication when provided.
    ///
    /// # Arguments
    /// * `path` - Path to the file (used to determine file type)
    /// * `content` - Content of the file
    /// * `variant_inputs` - Optional template variables (contains project config and any overrides)
    /// * `context` - Optional operation context for warning deduplication
    ///
    /// # Returns
    /// * `DependencyMetadata` - Extracted metadata (may be empty)
    ///
    /// # Template Support
    ///
    /// If `variant_inputs` is provided, frontmatter is rendered as a Tera template
    /// before parsing, allowing references like:
    /// `{{ project.language }}` or `{{ config.model }}`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::OperationContext;
    /// use agpm_cli::metadata::MetadataExtractor;
    /// use std::path::Path;
    ///
    /// let ctx = OperationContext::new();
    /// let path = Path::new("agent.md");
    /// let content = "---\ndependencies:\n  agents:\n    - path: helper.md\n---\n# Agent";
    ///
    /// let metadata = MetadataExtractor::extract(
    ///     path,
    ///     content,
    ///     None,
    ///     Some(&ctx)
    /// ).unwrap();
    /// ```
    pub fn extract(
        path: &Path,
        content: &str,
        variant_inputs: Option<&serde_json::Value>,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            "md" => Self::extract_markdown_frontmatter(content, variant_inputs, path, context),
            "json" => Self::extract_json_field(content, variant_inputs, path, context),
            _ => {
                // Scripts and other files don't support embedded dependencies
                Ok(DependencyMetadata::default())
            }
        }
    }

    /// Extract YAML frontmatter from Markdown content.
    ///
    /// Uses the unified frontmatter parser with templating support to extract
    /// dependency metadata from YAML frontmatter.
    fn extract_markdown_frontmatter(
        content: &str,
        variant_inputs: Option<&serde_json::Value>,
        path: &Path,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        let mut parser = FrontmatterParser::new();
        let result = parser.parse_with_templating::<crate::markdown::MarkdownMetadata>(
            content,
            variant_inputs,
            path,
            context,
        )?;

        // Convert MarkdownMetadata to DependencyMetadata
        if let Some(ref markdown_metadata) = result.data {
            // Extract dependencies from both root-level and agpm section
            let root_dependencies = markdown_metadata.dependencies.clone();
            let agpm_dependencies =
                markdown_metadata.get_agpm_metadata().and_then(|agpm| agpm.dependencies);

            let dependency_metadata = DependencyMetadata::new(
                root_dependencies,
                Some(AgpmMetadata {
                    templating: markdown_metadata
                        .get_agpm_metadata()
                        .and_then(|agpm| agpm.templating),
                    dependencies: agpm_dependencies,
                }),
            );

            // Validate resource types if we successfully parsed metadata
            Self::validate_resource_types(&dependency_metadata, path)?;
            Ok(dependency_metadata)
        } else {
            Ok(DependencyMetadata::default())
        }
    }

    /// Extract dependencies field from JSON content.
    ///
    /// Looks for a `dependencies` field in the top-level JSON object.
    /// Uses unified templating logic to respect per-resource templating settings.
    fn extract_json_field(
        content: &str,
        variant_inputs: Option<&serde_json::Value>,
        path: &Path,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        // Use unified templating logic - always template to catch syntax errors
        let mut parser = FrontmatterParser::new();
        let templated_content = parser.apply_templating(content, variant_inputs, path)?;

        let json: JsonValue = serde_json::from_str(&templated_content)
            .with_context(|| "Failed to parse JSON content")?;

        if let Some(deps) = json.get("dependencies") {
            // The dependencies field should match our expected structure
            match serde_json::from_value::<
                std::collections::BTreeMap<String, Vec<crate::manifest::DependencySpec>>,
            >(deps.clone())
            {
                Ok(dependencies) => {
                    let metadata = DependencyMetadata::new(Some(dependencies), None);
                    // Validate resource types (catch tool names used as types)
                    Self::validate_resource_types(&metadata, path)?;
                    Ok(metadata)
                }
                Err(e) => {
                    // Only warn once per file to avoid spam during transitive dependency resolution
                    if let Some(ctx) = context {
                        if ctx.should_warn_file(path) {
                            eprintln!(
                                "Warning: Unable to parse dependencies field in '{}'.

The document will be processed without metadata, and any declared dependencies
will NOT be resolved or installed.

Parse error: {}

For the correct dependency format, see:
https://github.com/aig787/agpm#transitive-dependencies",
                                path.display(),
                                e
                            );
                        }
                    }
                    Ok(DependencyMetadata::default())
                }
            }
        } else {
            Ok(DependencyMetadata::default())
        }
    }

    /// Validate that resource type names are correct (not tool names).
    ///
    /// Common mistake: using tool names (claude-code, opencode) as section headers
    /// instead of resource types (agents, snippets, commands).
    ///
    /// # Arguments
    /// * `metadata` - The metadata to validate
    /// * `file_path` - Path to the file being validated (for error messages)
    ///
    /// # Returns
    /// * `Ok(())` if validation passes
    /// * `Err` with helpful error message if tool names detected
    fn validate_resource_types(metadata: &DependencyMetadata, file_path: &Path) -> Result<()> {
        const VALID_RESOURCE_TYPES: &[&str] =
            &["agents", "commands", "snippets", "hooks", "mcp-servers", "scripts", "skills"];
        const TOOL_NAMES: &[&str] = &["claude-code", "opencode", "agpm"];

        // Check both root-level and nested dependencies
        if let Some(dependencies) = metadata.get_dependencies() {
            for resource_type in dependencies.keys() {
                if !VALID_RESOURCE_TYPES.contains(&resource_type.as_str()) {
                    if TOOL_NAMES.contains(&resource_type.as_str()) {
                        // Specific error for tool name confusion
                        anyhow::bail!(
                            "Invalid resource type '{}' in dependencies section of '{}'.\n\n\
                            You used a tool name ('{}') as a section header, but AGPM expects resource types.\n\n\
                            ✗ Wrong:\n  dependencies:\n    {}:\n      - path: ...\n\n\
                            ✓ Correct:\n  dependencies:\n    agents:  # or snippets, commands, etc.\n      - path: ...\n        tool: {}  # Specify tool here\n\n\
                            Valid resource types: {}",
                            resource_type,
                            file_path.display(),
                            resource_type,
                            resource_type,
                            resource_type,
                            VALID_RESOURCE_TYPES.join(", ")
                        );
                    }
                    // Generic error for unknown types
                    anyhow::bail!(
                        "Unknown resource type '{}' in dependencies section of '{}'.\n\
                        Valid resource types: {}",
                        resource_type,
                        file_path.display(),
                        VALID_RESOURCE_TYPES.join(", ")
                    );
                }
            }
        }
        Ok(())
    }

    /// Extract metadata from file content without knowing the file type.
    ///
    /// Tries to detect the format automatically.
    pub fn extract_auto(content: &str) -> Result<DependencyMetadata> {
        use std::path::PathBuf;

        // Try YAML frontmatter first (for Markdown)
        if (content.starts_with("---\n") || content.starts_with("---\r\n"))
            && let Ok(metadata) = Self::extract_markdown_frontmatter(
                content,
                None,
                &PathBuf::from("unknown.md"),
                None,
            )
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // Try JSON format
        if content.trim_start().starts_with('{')
            && let Ok(metadata) =
                Self::extract_json_field(content, None, &PathBuf::from("unknown.json"), None)
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // No metadata found
        Ok(DependencyMetadata::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ProjectConfig;

    #[test]
    fn test_extract_markdown_frontmatter() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
    - path: agents/reviewer.md
  snippets:
    - path: snippets/utils.md
---

# My Command

This is the command documentation."#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 2);
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["agents"][0].path, "agents/helper.md");
        assert_eq!(deps["agents"][0].version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_extract_markdown_no_frontmatter() {
        let content = r#"# My Command

This is a command without frontmatter."#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_json_dependencies() {
        let content = r#"{
  "events": ["UserPromptSubmit"],
  "type": "command",
  "command": ".claude/scripts/test.js",
  "dependencies": {
    "scripts": [
      { "path": "scripts/test-runner.sh", "version": "v1.0.0" },
      { "path": "scripts/validator.py" }
    ],
    "agents": [
      { "path": "agents/code-analyzer.md", "version": "~1.2.0" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["scripts"].len(), 2);
        assert_eq!(deps["agents"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/test-runner.sh");
        assert_eq!(deps["scripts"][0].version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_extract_json_no_dependencies() {
        let content = r#"{
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-github"]
}"#;

        let path = Path::new("mcp.json");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_script_file() {
        let content = r#"#!/bin/bash
echo "This is a script file"
# Scripts don't support dependencies"#;

        let path = Path::new("script.sh");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_auto_markdown() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
---

# Content"#;

        let metadata = MetadataExtractor::extract_auto(content).unwrap();
        assert!(metadata.has_dependencies());
        assert_eq!(metadata.dependency_count(), 1);
    }

    #[test]
    fn test_extract_auto_json() {
        let content = r#"{
  "dependencies": {
    "snippets": [
      { "path": "snippets/test.md" }
    ]
  }
}"#;

        let metadata = MetadataExtractor::extract_auto(content).unwrap();
        assert!(metadata.has_dependencies());
        assert_eq!(metadata.dependency_count(), 1);
    }

    #[test]
    fn test_windows_line_endings() {
        let content = "---\r\ndependencies:\r\n  agents:\r\n    - path: agents/test.md\r\n---\r\n\r\n# Content";

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 1);
        assert_eq!(deps["agents"][0].path, "agents/test.md");
    }

    #[test]
    fn test_empty_dependencies() {
        let content = r#"---
dependencies:
---

# Content"#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        // Should parse successfully but have no dependencies
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_malformed_yaml() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
    version: missing dash
---

# Content"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        // With the new frontmatter parser, malformed YAML is handled gracefully
        // and returns default metadata instead of erroring
        assert!(result.is_ok());
        let metadata = result.unwrap();
        // Should have no dependencies due to parsing failure
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_with_tool_field() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/backend.md
      version: v1.0.0
      tool: opencode
    - path: agents/frontend.md
      tool: claude-code
---

# Command with multi-tool dependencies"#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 2);

        // Verify tool fields are preserved
        assert_eq!(deps["agents"][0].path, "agents/backend.md");
        assert_eq!(deps["agents"][0].tool, Some("opencode".to_string()));

        assert_eq!(deps["agents"][1].path, "agents/frontend.md");
        assert_eq!(deps["agents"][1].tool, Some("claude-code".to_string()));
    }

    #[test]
    fn test_extract_unknown_field_warning() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
      version: v1.0.0
      invalid_field: should_warn
---

# Content"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        // Should succeed but return empty metadata due to unknown field
        assert!(result.is_ok());
        let metadata = result.unwrap();
        // With deny_unknown_fields, the parsing fails and we get empty metadata
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_template_frontmatter_with_project_vars() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        config_map.insert("framework".to_string(), toml::Value::String("tokio".into()));
        let project_config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Markdown with templated dependency path
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-guide.md
      version: v1.0.0
  commands:
    - path: configs/{{ agpm.project.framework }}-setup.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&variant_inputs_value), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Check that templates were resolved
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/rust-guide.md");

        assert_eq!(deps["commands"].len(), 1);
        assert_eq!(deps["commands"][0].path, "configs/tokio-setup.md");
    }

    #[test]
    fn test_template_frontmatter_with_missing_vars() {
        // Create a project config with only one variable
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Template references undefined variable (should error with helpful message)
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.undefined }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let result = MetadataExtractor::extract(path, content, Some(&variant_inputs_value), None);

        // Should error on undefined variable
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Failed to render frontmatter template"));
        // Tera error messages indicate undefined variables, but don't specifically suggest "default" filter
        assert!(error_msg.contains("Variable") && error_msg.contains("not found"));
    }

    #[test]
    fn test_template_frontmatter_with_default_filter() {
        // Create a project config with only one variable
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Use default filter for undefined variable (recommended pattern)
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.style | default(value="standard") }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&variant_inputs_value), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Default filter provides fallback value
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/rust-standard-guide.md");
    }

    #[test]
    fn test_template_json_dependencies() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("tool".to_string(), toml::Value::String("linter".into()));
        let project_config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // JSON with templated dependency path
        let content = r#"{
  "events": ["UserPromptSubmit"],
  "command": "node",
  "agpm": {
    "templating": true
  },
  "dependencies": {
    "scripts": [
      { "path": "scripts/{{ agpm.project.tool }}.js", "version": "v1.0.0" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&variant_inputs_value), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Check that template was resolved
        assert_eq!(deps["scripts"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/linter.js");
    }

    #[test]
    fn test_template_with_no_template_syntax() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Content without template syntax - should work normally
        let content = r#"---
dependencies:
  snippets:
    - path: standards/plain-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&variant_inputs_value), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Path should remain unchanged
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/plain-guide.md");
    }

    #[test]
    fn test_template_transitive_dep_path() {
        use std::path::PathBuf;

        // Test that dependency paths in frontmatter are templated correctly
        let content = r#"---
agpm:
  templating: true
dependencies:
  agents:
    - path: agents/{{ agpm.project.language }}-helper.md
      version: v1.0.0
---

# Main Agent
"#;

        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".to_string()));
        let config = ProjectConfig::from(config_map);

        // Convert project config to variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        let path = PathBuf::from("agents/main.md");
        let result = MetadataExtractor::extract(&path, content, Some(&variant_inputs_value), None);

        assert!(result.is_ok(), "Should extract metadata: {:?}", result.err());
        let metadata = result.unwrap();

        // Should have dependencies
        assert!(metadata.dependencies.is_some(), "Should have dependencies");
        let deps = metadata.dependencies.unwrap();

        // Should have agents key
        assert!(deps.contains_key("agents"), "Should have agents dependencies");
        let agents = &deps["agents"];

        // Should have one agent dependency
        assert_eq!(agents.len(), 1, "Should have one agent dependency");

        // Path should be templated (not contain template syntax)
        let dep_path = &agents[0].path;
        assert_eq!(
            dep_path, "agents/rust-helper.md",
            "Path should be templated to rust-helper, got: {}",
            dep_path
        );
        assert!(!dep_path.contains("{{"), "Path should not contain template syntax");
        assert!(!dep_path.contains("}}"), "Path should not contain template syntax");
    }

    #[test]
    fn test_validate_tool_name_as_resource_type_yaml() {
        // YAML using tool name 'opencode' instead of resource type 'agents'
        let content = r#"---
dependencies:
  opencode:
    - path: agents/helper.md
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid resource type 'opencode'"));
        assert!(err_msg.contains("tool name"));
        assert!(err_msg.contains("agents:"));
    }

    #[test]
    fn test_validate_tool_name_as_resource_type_json() {
        // JSON using tool name 'claude-code' instead of resource type 'snippets'
        let content = r#"{
  "dependencies": {
    "claude-code": [
      { "path": "snippets/helper.md" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid resource type 'claude-code'"));
        assert!(err_msg.contains("tool name"));
    }

    #[test]
    fn test_validate_unknown_resource_type() {
        // Using a completely unknown resource type
        let content = r#"---
dependencies:
  foobar:
    - path: something/test.md
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unknown resource type 'foobar'"));
        assert!(err_msg.contains("Valid resource types"));
    }

    #[test]
    fn test_validate_correct_resource_types() {
        // All valid resource types should pass
        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
  snippets:
    - path: snippets/util.md
  commands:
    - path: commands/deploy.md
  skills:
    - path: skills/my-skill
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_warning_deduplication_with_context() {
        use std::path::PathBuf;

        // Create an operation context
        let ctx = OperationContext::new();

        let path = PathBuf::from("test-file.md");
        let different_path = PathBuf::from("different-file.md");

        // First call should return true (first warning)
        assert!(ctx.should_warn_file(&path));

        // Second call should return false (already warned)
        assert!(!ctx.should_warn_file(&path));

        // Third call should also return false
        assert!(!ctx.should_warn_file(&path));

        // Different file should still warn
        assert!(ctx.should_warn_file(&different_path));
    }

    #[test]
    fn test_context_isolation() {
        use std::path::PathBuf;

        // Two separate contexts should be isolated
        let ctx1 = OperationContext::new();
        let ctx2 = OperationContext::new();
        let path = PathBuf::from("test-isolation.md");

        // Both contexts should warn the first time
        assert!(ctx1.should_warn_file(&path));
        assert!(ctx2.should_warn_file(&path));

        // Both should deduplicate independently
        assert!(!ctx1.should_warn_file(&path));
        assert!(!ctx2.should_warn_file(&path));
    }

    #[test]
    fn test_extract_skill_dependencies() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/rust-expert.md
      version: "^1.0.0"
  snippets:
    - path: snippets/rust-patterns.md
  skills:
    - path: skills/code-formatter
      version: v2.0.0
    - path: skills/documentation-helper
---

# My Skill

This skill helps with Rust development.
"#;

        let path = Path::new("SKILL.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Check agents
        assert_eq!(deps["agents"].len(), 1);
        assert_eq!(deps["agents"][0].path, "agents/rust-expert.md");
        assert_eq!(deps["agents"][0].version, Some("^1.0.0".to_string()));

        // Check snippets
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "snippets/rust-patterns.md");

        // Check skills
        assert_eq!(deps["skills"].len(), 2);
        assert_eq!(deps["skills"][0].path, "skills/code-formatter");
        assert_eq!(deps["skills"][0].version, Some("v2.0.0".to_string()));
        assert_eq!(deps["skills"][1].path, "skills/documentation-helper");
    }

    #[test]
    fn test_extract_skill_dependencies_with_tool_specification() {
        let content = r#"---
dependencies:
  skills:
    - path: skills/opencode-formatter
      tool: opencode
      version: v1.5.0
    - path: skills/claude-analyzer
      tool: claude-code
---

# Multi-Tool Skill
"#;

        let path = Path::new("SKILL.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        assert_eq!(deps["skills"].len(), 2);
        assert_eq!(deps["skills"][0].path, "skills/opencode-formatter");
        assert_eq!(deps["skills"][0].tool, Some("opencode".to_string()));
        assert_eq!(deps["skills"][1].path, "skills/claude-analyzer");
        assert_eq!(deps["skills"][1].tool, Some("claude-code".to_string()));
    }
}
