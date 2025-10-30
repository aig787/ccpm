//! Enhanced template error handling for AGPM
//!
//! This module provides structured error types for template rendering with detailed
//! context information and user-friendly formatting.

use std::path::PathBuf;

use super::renderer::DependencyChainEntry;
use crate::core::ResourceType;

/// Enhanced template errors with detailed context
#[derive(Debug)]
pub enum TemplateError {
    /// Variable referenced in template was not found in context
    VariableNotFound {
        /// The name of the variable that was not found in the template context
        variable: String,
        /// Complete list of variables available in the current template context
        available_variables: Box<Vec<String>>,
        /// Suggested similar variable names based on Levenshtein distance analysis
        suggestions: Box<Vec<String>>,
        /// Location information including resource, file path, and dependency chain
        location: Box<ErrorLocation>,
    },

    /// Circular dependency detected in template rendering
    CircularDependency {
        /// Complete dependency chain showing the circular reference path
        chain: Box<Vec<DependencyChainEntry>>,
    },

    /// Template syntax parsing or validation error
    SyntaxError {
        /// Human-readable description of the syntax error
        message: String,
        /// Location information including resource, file path, and dependency chain
        location: Box<ErrorLocation>,
    },

    /// Failed to render a dependency template
    DependencyRenderFailed {
        /// Name/identifier of the dependency that failed to render
        dependency: String,
        /// Underlying error that caused the render failure
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Location information including resource, file path, and dependency chain
        location: Box<ErrorLocation>,
    },

    /// Error occurred during content filter processing
    ContentFilterError {
        /// Recursion depth when the error occurred (for debugging infinite loops)
        depth: usize,
        /// Underlying error that caused the content filter failure
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Location information including resource, file path, and dependency chain
        location: Box<ErrorLocation>,
    },
}

/// Location information for template errors
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    /// Resource where error occurred
    pub resource_name: String,
    pub resource_type: ResourceType,
    /// Full dependency chain to this resource
    pub dependency_chain: Vec<DependencyChainEntry>,
    /// File path if known
    pub file_path: Option<PathBuf>,
    /// Line number if available from Tera
    pub line_number: Option<usize>,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::VariableNotFound {
                variable,
                ..
            } => {
                write!(f, "Template variable not found: '{}'", variable)
            }
            TemplateError::SyntaxError {
                message,
                ..
            } => {
                write!(f, "Template syntax error: {}", message)
            }
            TemplateError::CircularDependency {
                chain,
            } => {
                if let Some(first) = chain.first() {
                    write!(f, "Circular dependency detected: {}", first.name)
                } else {
                    write!(f, "Circular dependency detected")
                }
            }
            TemplateError::DependencyRenderFailed {
                dependency,
                source,
                ..
            } => {
                write!(f, "Failed to render dependency '{}': {}", dependency, source)
            }
            TemplateError::ContentFilterError {
                source,
                ..
            } => {
                write!(f, "Content filter error: {}", source)
            }
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TemplateError::DependencyRenderFailed {
                source,
                ..
            } => Some(source.as_ref()),
            TemplateError::ContentFilterError {
                source,
                ..
            } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl TemplateError {
    /// Generate user-friendly error message with context and suggestions
    pub fn format_with_context(&self) -> String {
        match self {
            TemplateError::VariableNotFound {
                variable,
                available_variables,
                suggestions,
                location,
            } => format_variable_not_found_error(
                variable,
                available_variables,
                suggestions,
                location,
            ),
            TemplateError::CircularDependency {
                chain,
            } => format_circular_dependency_error(chain),
            TemplateError::SyntaxError {
                message,
                location,
            } => format_syntax_error(message, location),
            TemplateError::DependencyRenderFailed {
                dependency,
                source,
                location,
            } => format_dependency_render_error(dependency, source.as_ref(), location),
            TemplateError::ContentFilterError {
                depth,
                source,
                location,
            } => format_content_filter_error(*depth, source.as_ref(), location),
        }
    }
}

/// Format a detailed "variable not found" error message
fn format_variable_not_found_error(
    variable: &str,
    available_variables: &[String],
    suggestions: &[String],
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    // Header
    msg.push_str("ERROR: Template Variable Not Found\n\n");

    // Variable info
    msg.push_str(&format!("Variable: {}\n", variable));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    msg.push_str(&format!(
        "Resource: {} ({})\n\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    // Dependency chain
    if !location.dependency_chain.is_empty() {
        msg.push_str("Dependency chain:\n");
        for (i, entry) in location.dependency_chain.iter().enumerate() {
            let indent = "  ".repeat(i);
            let arrow = if i > 0 {
                "└─ "
            } else {
                ""
            };
            let warning = if i == location.dependency_chain.len() - 1 {
                " ⚠️ Error occurred here"
            } else {
                ""
            };

            msg.push_str(&format!(
                "{}{}{}: {}{}\n",
                indent,
                arrow,
                format_resource_type(&entry.resource_type),
                entry.name,
                warning
            ));
        }
        msg.push('\n');
    }

    // Suggestions based on variable name analysis
    if variable.starts_with("agpm.deps.") {
        msg.push_str(&format_missing_dependency_suggestion(variable, location));
    } else if !suggestions.is_empty() {
        msg.push_str("Did you mean one of these?\n");
        for suggestion in suggestions.iter() {
            msg.push_str(&format!("  - {}\n", suggestion));
        }
        msg.push('\n');
    }

    // Available variables (truncated list)
    if !available_variables.is_empty() {
        msg.push_str("Available variables in this context:\n");

        // Group by prefix
        let mut grouped = std::collections::BTreeMap::new();
        for var in available_variables.iter() {
            let prefix = var.split('.').next().unwrap_or(var);
            grouped.entry(prefix).or_insert_with(Vec::new).push(var.clone());
        }

        for (prefix, vars) in grouped.iter().take(5) {
            if vars.len() <= 3 {
                for var in vars {
                    msg.push_str(&format!("  {}\n", var));
                }
            } else {
                msg.push_str(&format!("  {}.*  ({} variables)\n", prefix, vars.len()));
            }
        }

        if grouped.len() > 5 {
            msg.push_str(&format!("  ... and {} more\n", grouped.len() - 5));
        }
        msg.push('\n');
    }

    msg
}

/// Format suggestion for missing dependency declaration
fn format_missing_dependency_suggestion(variable: &str, location: &ErrorLocation) -> String {
    // Parse variable name: agpm.deps.<type>.<name>.<property>
    let parts: Vec<&str> = variable.split('.').collect();
    if parts.len() < 4 || parts[0] != "agpm" || parts[1] != "deps" {
        return String::new();
    }

    let dep_type = parts[2]; // "snippets", "agents", etc.
    let dep_name = parts[3]; // "plugin_lifecycle_guide", etc.

    // Convert snake_case back to potential file name
    // (heuristic: replace _ with -)
    let suggested_filename = dep_name.replace('_', "-");

    let mut msg = String::new();
    msg.push_str(&format!(
        "Suggestion: '{}' references '{}' but doesn't declare it as a dependency.\n\n",
        location.resource_name, dep_name
    ));

    msg.push_str(&format!("Fix: Add this to {} frontmatter:\n\n", location.resource_name));
    msg.push_str("---\n");
    msg.push_str("agpm:\n");
    msg.push_str("  templating: true\n");
    msg.push_str("dependencies:\n");
    msg.push_str(&format!("  {}:\n", dep_type));
    msg.push_str(&format!("    - path: ./{}.md\n", suggested_filename));
    msg.push_str("      install: false\n");
    msg.push_str("---\n\n");

    msg.push_str("Note: Adjust the path based on actual file location.\n\n");

    msg
}

/// Format circular dependency error
fn format_circular_dependency_error(chain: &[DependencyChainEntry]) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Circular Dependency Detected\n\n");
    msg.push_str("A resource is attempting to include itself through a chain of dependencies.\n\n");

    msg.push_str("Circular chain:\n");
    for entry in chain.iter() {
        msg.push_str(&format!(
            "  {} ({})\n",
            entry.name,
            format_resource_type(&entry.resource_type)
        ));
        msg.push_str("  ↓\n");
    }
    msg.push_str(&format!("  {} (circular reference)\n\n", chain[0].name));

    msg.push_str("Suggestion: Remove the dependency that creates the cycle.\n");
    msg.push_str("Consider refactoring shared content into a separate resource.\n\n");

    msg
}

/// Format syntax error
fn format_syntax_error(message: &str, location: &ErrorLocation) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Template syntax error\n\n");
    msg.push_str(&format!("Error: {}\n", message));
    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    if !location.dependency_chain.is_empty() {
        msg.push_str("\nDependency chain:\n");
        for entry in &location.dependency_chain {
            msg.push_str(&format!(
                "  {} ({})\n",
                entry.name,
                format_resource_type(&entry.resource_type)
            ));
        }
    }

    msg.push_str("\nSuggestion: Check template syntax for unclosed tags or invalid expressions.\n");
    msg.push_str("Common issues:\n");
    msg.push_str("  - Unclosed {{ }} or {% %} delimiters\n");
    msg.push_str("  - Invalid filter names\n");
    msg.push_str("  - Missing quotes around string values\n\n");

    msg
}

/// Format dependency render error
fn format_dependency_render_error(
    dependency: &str,
    source: &(dyn std::error::Error + Send + Sync),
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Dependency Render Failed\n\n");
    msg.push_str(&format!("Dependency: {}\n", dependency));
    msg.push_str(&format!("Error: {}\n", source));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    msg.push_str("\nSuggestion: Check the dependency file for template errors.\n");
    msg.push_str("The dependency may contain invalid template syntax or missing variables.\n\n");

    msg
}

/// Format content filter error
fn format_content_filter_error(
    depth: usize,
    source: &(dyn std::error::Error + Send + Sync),
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Content Filter Error\n\n");
    msg.push_str(&format!("Depth: {}\n", depth));
    msg.push_str(&format!("Error: {}\n", source));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    msg.push_str("\nSuggestion: Check the file being included by the content filter.\n");
    msg.push_str("The included file may contain template errors or circular dependencies.\n\n");

    msg
}

/// Format a resource type as a human-readable string.
///
/// Converts the ResourceType enum to lowercase string representation
/// for use in error messages and user-facing output.
///
/// # Arguments
///
/// * `rt` - The ResourceType enum value to format
///
/// # Returns
///
/// String representation of the resource type in lowercase:
/// - "agent" for ResourceType::Agent
/// - "command" for ResourceType::Command
/// - "snippet" for ResourceType::Snippet
/// - "hook" for ResourceType::Hook
/// - "script" for ResourceType::Script
/// - "mcp-server" for ResourceType::McpServer
///
/// # Examples
///
/// ```rust
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::templating::error::format_resource_type;
///
/// let agent_type = ResourceType::Agent;
/// assert_eq!(format_resource_type(&agent_type), "agent");
/// ```
pub fn format_resource_type(rt: &ResourceType) -> String {
    match rt {
        ResourceType::Agent => "agent",
        ResourceType::Command => "command",
        ResourceType::Snippet => "snippet",
        ResourceType::Hook => "hook",
        ResourceType::Script => "script",
        ResourceType::McpServer => "mcp-server",
        ResourceType::Skill => "skill",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::templating::renderer::DependencyChainEntry;
    use std::error::Error;

    #[test]
    fn test_template_error_variable_not_found() {
        let error = TemplateError::VariableNotFound {
            variable: "missing_var".to_string(),
            available_variables: Box::new(vec![
                "var1".to_string(),
                "var2".to_string(),
                "similar_var".to_string(),
            ]),
            suggestions: Box::new(vec![
                "Did you mean 'similar_var'?".to_string(),
                "Check variable spelling".to_string(),
            ]),
            location: Box::new(ErrorLocation {
                resource_name: "test-agent".to_string(),
                resource_type: ResourceType::Agent,
                dependency_chain: vec![DependencyChainEntry {
                    name: "agent1".to_string(),
                    resource_type: ResourceType::Agent,
                    path: Some("agents/agent1.md".to_string()),
                }],
                file_path: None,
                line_number: Some(10),
            }),
        };

        let formatted = error.format_with_context();

        assert!(formatted.contains("Template Variable Not Found"));
        assert!(formatted.contains("missing_var"));
        assert!(formatted.contains("test-agent"));
        assert!(formatted.contains("Line: 10"));
        assert!(formatted.contains("agent1"));
    }

    #[test]
    fn test_template_error_circular_dependency() {
        let error = TemplateError::CircularDependency {
            chain: Box::new(vec![
                DependencyChainEntry {
                    name: "agent-a".to_string(),
                    resource_type: ResourceType::Agent,
                    path: Some("agents/agent-a.md".to_string()),
                },
                DependencyChainEntry {
                    name: "agent-b".to_string(),
                    resource_type: ResourceType::Agent,
                    path: Some("agents/agent-b.md".to_string()),
                },
                DependencyChainEntry {
                    name: "agent-a".to_string(),
                    resource_type: ResourceType::Agent,
                    path: Some("agents/agent-a.md".to_string()),
                },
            ]),
        };

        let formatted = error.format_with_context();

        assert!(formatted.contains("Circular Dependency"));
        assert!(formatted.contains("agent-a"));
        assert!(formatted.contains("agent-b"));
    }

    #[test]
    fn test_template_error_syntax_error() {
        let error = TemplateError::SyntaxError {
            message: "Unexpected end of template".to_string(),
            location: Box::new(ErrorLocation {
                resource_name: "test-snippet".to_string(),
                resource_type: ResourceType::Snippet,
                dependency_chain: vec![],
                file_path: None,
                line_number: Some(25),
            }),
        };

        let formatted = error.format_with_context();

        assert!(formatted.contains("Template syntax error"));
        assert!(formatted.contains("Unexpected end of template"));
        assert!(formatted.contains("test-snippet"));
        assert!(formatted.contains("Line: 25"));
    }

    #[test]
    fn test_template_error_dependency_render_failed() {
        let source_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let error = TemplateError::DependencyRenderFailed {
            dependency: "helper-agent".to_string(),
            source: Box::new(source_error),
            location: Box::new(ErrorLocation {
                resource_name: "main-agent".to_string(),
                resource_type: ResourceType::Agent,
                dependency_chain: vec![DependencyChainEntry {
                    name: "helper-agent".to_string(),
                    resource_type: ResourceType::Agent,
                    path: Some("agents/helper-agent.md".to_string()),
                }],
                file_path: None,
                line_number: None,
            }),
        };

        let formatted = error.format_with_context();

        assert!(formatted.contains("Dependency Render Failed"));
        assert!(formatted.contains("helper-agent"));
        assert!(formatted.contains("File not found"));
        assert!(formatted.contains("main-agent"));
    }

    #[test]
    fn test_template_error_content_filter_error() {
        let source_error =
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
        let error = TemplateError::ContentFilterError {
            depth: 5,
            source: Box::new(source_error),
            location: Box::new(ErrorLocation {
                resource_name: "test-script".to_string(),
                resource_type: ResourceType::Script,
                dependency_chain: vec![],
                file_path: None,
                line_number: Some(15),
            }),
        };

        let formatted = error.format_with_context();

        assert!(formatted.contains("Content Filter Error"));
        assert!(formatted.contains("Depth: 5"));
        assert!(formatted.contains("Access denied"));
        assert!(formatted.contains("test-script"));
        assert!(formatted.contains("Line: 15"));
    }

    #[test]
    fn test_error_location_with_line_number() {
        let location = ErrorLocation {
            resource_name: "test-resource".to_string(),
            resource_type: ResourceType::McpServer,
            dependency_chain: vec![DependencyChainEntry {
                name: "dep1".to_string(),
                resource_type: ResourceType::Agent,
                path: Some("agents/dep1.md".to_string()),
            }],
            file_path: Some(std::path::PathBuf::from("agents/test.md")),
            line_number: Some(42),
        };

        assert_eq!(location.resource_name, "test-resource");
        assert_eq!(location.resource_type, ResourceType::McpServer);
        assert_eq!(location.dependency_chain.len(), 1);
        assert_eq!(location.file_path.as_ref().unwrap().to_str().unwrap(), "agents/test.md");
        assert_eq!(location.line_number, Some(42));
    }

    #[test]
    fn test_error_location_without_line_number() {
        let location = ErrorLocation {
            resource_name: "test-resource".to_string(),
            resource_type: ResourceType::Command,
            dependency_chain: vec![],
            file_path: None,
            line_number: None,
        };

        assert_eq!(location.resource_name, "test-resource");
        assert_eq!(location.resource_type, ResourceType::Command);
        assert!(location.dependency_chain.is_empty());
        assert!(location.file_path.is_none());
        assert!(location.line_number.is_none());
    }

    #[test]
    fn test_format_resource_type() {
        assert_eq!(format_resource_type(&ResourceType::Agent), "agent");
        assert_eq!(format_resource_type(&ResourceType::Snippet), "snippet");
        assert_eq!(format_resource_type(&ResourceType::Command), "command");
        assert_eq!(format_resource_type(&ResourceType::McpServer), "mcp-server");
        assert_eq!(format_resource_type(&ResourceType::Script), "script");
        assert_eq!(format_resource_type(&ResourceType::Hook), "hook");
    }

    #[test]
    fn test_template_error_source() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
        let error = TemplateError::DependencyRenderFailed {
            dependency: "test-dep".to_string(),
            source: Box::new(io_error),
            location: Box::new(ErrorLocation {
                resource_name: "test-resource".to_string(),
                resource_type: ResourceType::Agent,
                dependency_chain: vec![],
                file_path: None,
                line_number: None,
            }),
        };

        // Test that the error implements std::error::Error
        let source = error.source();
        assert!(source.is_some());
    }
}
