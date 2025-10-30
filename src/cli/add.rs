//! Add command implementation for AGPM
//!
//! This module provides functionality to add sources and dependencies
//! to a AGPM project manifest. It supports both Git repository sources
//! and various types of resource dependencies (agents, snippets, commands, MCP servers).

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use colored::Colorize;
use regex::Regex;
use std::path::Path;

use crate::manifest::{
    DetailedDependency, Manifest, ResourceDependency, find_manifest_with_optional,
};
use crate::models::{
    AgentDependency, CommandDependency, DependencyType, HookDependency, McpServerDependency,
    ScriptDependency, SkillDependency, SnippetDependency, SourceSpec,
};

/// Command to add sources and dependencies to a AGPM project.
#[derive(Args)]
pub struct AddCommand {
    /// The specific add operation to perform
    #[command(subcommand)]
    command: AddSubcommand,
}

/// Subcommands for the add command.
#[derive(Subcommand)]
enum AddSubcommand {
    /// Add a new Git repository source to the manifest
    Source {
        /// Name for the source
        name: String,
        /// Git repository URL
        url: String,
    },

    /// Add a resource dependency to the manifest
    #[command(subcommand)]
    Dep(DependencySubcommand),
}

/// Dependency subcommands for different resource types
#[derive(Subcommand)]
enum DependencySubcommand {
    /// Add an agent dependency
    Agent(AgentDependency),

    /// Add a snippet dependency
    Snippet(SnippetDependency),

    /// Add a command dependency
    Command(CommandDependency),

    /// Add a script dependency
    Script(ScriptDependency),

    /// Add a hook dependency
    Hook(HookDependency),

    /// Add an MCP server dependency
    McpServer(McpServerDependency),

    /// Add a skill dependency
    Skill(SkillDependency),
}

impl AddCommand {
    /// Execute the add command with an optional manifest path.
    ///
    /// This method allows specifying a custom path to the agpm.toml manifest file.
    /// If no path is provided, it will search for agpm.toml in the current directory
    /// and parent directories.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional path to the agpm.toml file
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the add operation completed successfully
    /// - `Err(anyhow::Error)` if the operation fails (e.g., invalid manifest, source not found)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use agpm_cli::cli::add::{AddCommand, AddSubcommand};
    /// use std::path::PathBuf;
    ///
    /// let cmd = AddCommand {
    ///     command: AddSubcommand::Source {
    ///         name: "my-source".to_string(),
    ///         url: "https://github.com/example/repo.git".to_string(),
    ///     }
    /// };
    ///
    /// // Use default manifest location
    /// cmd.execute_with_manifest_path(None).await?;
    ///
    /// // Or specify custom manifest path
    /// cmd.execute_with_manifest_path(Some(PathBuf::from("/path/to/agpm.toml"))).await?;
    /// ```
    pub async fn execute_with_manifest_path(
        self,
        manifest_path: Option<std::path::PathBuf>,
    ) -> Result<()> {
        match self.command {
            AddSubcommand::Source {
                name,
                url,
            } => {
                add_source_with_manifest_path(
                    SourceSpec {
                        name,
                        url,
                    },
                    manifest_path,
                )
                .await
            }
            AddSubcommand::Dep(dep_command) => {
                let dep_type = match dep_command {
                    DependencySubcommand::Agent(agent) => DependencyType::Agent(agent),
                    DependencySubcommand::Snippet(snippet) => DependencyType::Snippet(snippet),
                    DependencySubcommand::Command(command) => DependencyType::Command(command),
                    DependencySubcommand::Script(script) => DependencyType::Script(script),
                    DependencySubcommand::Hook(hook) => DependencyType::Hook(hook),
                    DependencySubcommand::McpServer(mcp) => DependencyType::McpServer(mcp),
                    DependencySubcommand::Skill(skill) => DependencyType::Skill(skill),
                };
                add_dependency_with_manifest_path(dep_type, manifest_path).await
            }
        }
    }
}

/// Add a new source to the manifest with optional manifest path
async fn add_source_with_manifest_path(
    source: SourceSpec,
    manifest_path: Option<std::path::PathBuf>,
) -> Result<()> {
    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Check if source already exists
    if manifest.sources.contains_key(&source.name) {
        return Err(anyhow!("Source '{}' already exists in manifest", source.name));
    }

    // Add the source
    manifest.sources.insert(source.name.clone(), source.url.clone());

    // Save the manifest
    manifest.save(&manifest_path)?;

    println!("{}", format!("Added source '{}' → {}", source.name, source.url).green());

    Ok(())
}

/// Add a dependency to the manifest and install it with optional manifest path
async fn add_dependency_with_manifest_path(
    dep_type: DependencyType,
    manifest_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let common = dep_type.common();

    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Parse dependency with manifest context for enhanced version handling.
    // The manifest context enables proper detection of local vs Git sources
    // and improves version constraint validation for known sources.
    let (name, mut dependency) =
        parse_dependency_spec(&common.spec, &common.name, Some(&manifest))?;

    // Apply additional fields from CLI arguments to the dependency
    // If we have advanced options (tool, target, filename) and a Simple dependency,
    // convert it to Detailed so we can apply these options
    let needs_detailed =
        common.tool.is_some() || common.target.is_some() || common.filename.is_some();

    if needs_detailed {
        if let ResourceDependency::Simple(path) = &dependency {
            // Convert Simple to Detailed to support advanced options
            let tool = common.tool.clone();
            dependency = ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: path.clone(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: common.target.clone(),
                filename: common.filename.clone(),
                dependencies: None,
                tool,
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));
        }
    }

    // Apply fields to Detailed dependencies
    if let ResourceDependency::Detailed(detailed) = &mut dependency {
        if let Some(tool) = &common.tool {
            detailed.tool = Some(tool.clone());
        }
        if let Some(target) = &common.target {
            detailed.target = Some(target.clone());
        }
        if let Some(filename) = &common.filename {
            detailed.filename = Some(filename.clone());
        }
    }

    // Determine the resource type
    let resource_type = dep_type.resource_type();

    // Handle MCP servers (now using standard ResourceDependency)
    if let DependencyType::McpServer(_) = &dep_type {
        // Check if dependency already exists
        if manifest.mcp_servers.contains_key(&name) && !common.force {
            return Err(anyhow!(
                "MCP server '{name}' already exists in manifest. Use --force to overwrite"
            ));
        }

        // Add to manifest (MCP servers now use standard ResourceDependency)
        manifest.mcp_servers.insert(name.clone(), dependency.clone());
    } else {
        // Handle regular resources (agents, snippets, commands, scripts, hooks, skills)
        let section = match &dep_type {
            DependencyType::Agent(_) => &mut manifest.agents,
            DependencyType::Snippet(_) => &mut manifest.snippets,
            DependencyType::Command(_) => &mut manifest.commands,
            DependencyType::Script(_) => &mut manifest.scripts,
            DependencyType::Hook(_) => &mut manifest.hooks,
            DependencyType::Skill(_) => &mut manifest.skills,
            DependencyType::McpServer(_) => unreachable!(), // Handled above
        };

        // Check if dependency already exists
        if section.contains_key(&name) && !common.force {
            return Err(anyhow!(
                "{resource_type} '{name}' already exists in manifest. Use --force to overwrite"
            ));
        }

        // Add to manifest
        section.insert(name.clone(), dependency.clone());
    }

    // Save the manifest
    manifest.save(&manifest_path)?;

    println!("{}", format!("Added {resource_type} '{name}'").green());

    // Auto-install the dependency unless --no-install is specified
    if !common.no_install {
        println!("{}", "Installing dependency...".cyan());
        install_single_dependency(&name, resource_type, &manifest, &manifest_path).await?;
    }

    Ok(())
}

/// Parse a dependency specification string into a name and `ResourceDependency`.
///
/// This function parses dependency specifications with enhanced context awareness,
/// using the manifest to distinguish between local file sources and Git repository
/// sources for improved version handling and validation.
///
/// # Arguments
///
/// * `spec` - The dependency specification string (see format details below)
/// * `custom_name` - Optional custom name for the dependency
/// * `manifest` - Optional manifest context for source type detection
///
/// # Dependency Specification Format
///
/// The dependency spec string supports multiple formats to accommodate both
/// Git-based and local file dependencies:
///
/// ## Remote Dependencies (Git repositories)
///
/// ### Format: `source:path@version`
/// - `source`: Name of a Git source defined in the manifest's `[sources]` section
/// - `path`: Path to the file within the repository (e.g., `agents/reviewer.md`)
/// - `version`: Git ref (tag, branch, or commit SHA) - optional, defaults to "main"
///
/// #### Examples:
/// ```text
/// # With explicit version
/// official:agents/reviewer.md@v1.0.0
/// community:snippets/utils.md@feature-branch
/// myrepo:commands/deploy.md@abc123f
///
/// # Without version (defaults to "main" for Git sources)
/// official:agents/reviewer.md
/// community:snippets/utils.md
/// ```
///
/// ## Local Dependencies
///
/// ### Absolute paths
/// ```text
/// # Unix/Linux/macOS
/// /home/user/resources/agent.md
/// /usr/local/share/agpm/snippets/helper.md
///
/// # Windows
/// C:\Users\name\resources\agent.md
/// \\server\share\resources\command.md
/// ```
///
/// ### Relative paths
/// ```text
/// ./agents/local-agent.md
/// ../shared-resources/snippet.md
/// ../../company-resources/hooks/pre-commit.json
/// ```
///
/// ### File URLs
/// ```text
/// file:///home/user/resources/agent.md
/// file://C:/Users/name/resources/agent.md
/// ```
///
/// ## Pattern Dependencies (Glob patterns)
///
/// Pattern dependencies use the same format as single files but with glob patterns
/// in the path component:
///
/// ```text
/// # All markdown files in agents directory
/// community:agents/*.md@v1.0.0
///
/// # All review-related agents recursively
/// official:agents/**/review*.md@v2.0.0
///
/// # All Python snippets
/// community:snippets/python/*.md
///
/// # Local patterns
/// ./agents/*.md
/// ../resources/**/*.json
/// ```
///
/// ## Automatic Name Derivation
///
/// If no custom name is provided via `--name`, the dependency name is automatically
/// derived from the file path:
///
/// - `agents/reviewer.md` → name: "reviewer"
/// - `snippets/python/utils.md` → name: "utils"
/// - `/path/to/helper.md` → name: "helper"
/// - `commands/deploy.sh` → name: "deploy"
///
/// For pattern dependencies, you should typically provide a custom name:
/// ```bash
/// agpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents
/// ```
///
/// ## Version Handling
///
/// - **Git sources**: If no version specified, defaults to "main"
/// - **Local sources**: Version field is not applicable and will be `None`
/// - **Version formats**: Tags (v1.0.0), branches (main, develop), commits (abc123f)
///
/// ## Source Detection
///
/// The function automatically detects whether a source refers to a Git repository
/// or a local directory by examining the source URL in the manifest:
///
/// - Git sources: `https://`, `git://`, `git@`, etc.
/// - Local sources: `/absolute/path`, `./relative/path`, `../relative/path`, `file://`
///
/// # Returns
///
/// A tuple of `(dependency_name, ResourceDependency)` where:
/// - `dependency_name`: The resolved name for the dependency
/// - `ResourceDependency`: Either `Simple` (for local paths) or `Detailed` (for Git sources)
///
/// # Errors
///
/// Returns an error if:
/// - The regex pattern for parsing fails to compile
/// - The spec string is malformed
///
/// # Examples
///
/// ```ignore
/// // Parse a remote dependency with version
/// let (name, dep) = parse_dependency_spec(
///     "official:agents/reviewer.md@v1.0.0",
///     &None,
///     None
/// ).unwrap();
/// assert_eq!(name, "reviewer");
///
/// // Parse a local file dependency
/// let (name, dep) = parse_dependency_spec(
///     "./local/agent.md",
///     &Some("my-agent".to_string()),
///     None
/// ).unwrap();
/// assert_eq!(name, "my-agent");
///
/// // Parse with manifest context for better source detection
/// let manifest = Manifest::load(Path::new("agpm.toml")).unwrap();
/// let (name, dep) = parse_dependency_spec(
///     "community:snippets/helper.md",
///     &None,
///     Some(&manifest)
/// ).unwrap();
/// ```
#[allow(clippy::ref_option)]
fn parse_dependency_spec(
    spec: &str,
    custom_name: &Option<String>,
    manifest: Option<&Manifest>,
) -> Result<(String, ResourceDependency)> {
    // Check if this is a Windows absolute path (e.g., C:\path\to\file)
    // or Unix absolute path (e.g., /path/to/file)
    let is_absolute_path = {
        #[cfg(windows)]
        {
            // Windows: Check for drive letter (C:) or UNC path (\\server)
            spec.len() >= 3
                && spec.chars().nth(1) == Some(':')
                && spec.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                || spec.starts_with("\\\\")
        }
        #[cfg(not(windows))]
        {
            // Unix: Check for leading /
            spec.starts_with('/')
        }
    };

    // Check if it's a local file path
    let is_local_path = is_absolute_path || spec.starts_with("file:") || Path::new(spec).exists();

    // Pattern: source:path@version or source:path
    // But only apply if it's not a local path
    let remote_pattern = Regex::new(r"^([^:]+):([^@]+)(?:@(.+))?$")?;

    if !is_local_path && remote_pattern.is_match(spec) {
        let captures = remote_pattern.captures(spec).unwrap();
        // Remote dependency
        let source = captures.get(1).unwrap().as_str().to_string();
        let path = captures.get(2).unwrap().as_str().to_string();
        let version = captures.get(3).map(|m| m.as_str().to_string());

        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
        });

        // Check if the source is a local path by looking up the source URL in the manifest
        let source_is_local = if let Some(manifest) = manifest {
            if let Some(source_url) = manifest.sources.get(&source) {
                source_url.starts_with('/')
                    || source_url.starts_with("./")
                    || source_url.starts_with("../")
                    || source_url.starts_with("file://")
                    || (cfg!(windows)
                        && source_url.len() >= 3
                        && source_url.chars().nth(1) == Some(':'))
            } else {
                // Source not found in manifest - assume it's a Git source for safety
                false
            }
        } else {
            // No manifest provided - assume it's a Git source for safety
            false
        };

        // Add default "main" version for Git sources (not local path sources) when no version is specified
        let final_version = if version.is_none() && !source_is_local {
            Some("main".to_string())
        } else {
            version
        };

        Ok((
            name,
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some(source),
                path,
                version: final_version,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: None,
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            })),
        ))
    } else if is_local_path {
        // Local dependency
        let path = if spec.starts_with("file:") {
            spec.trim_start_matches("file:")
        } else {
            spec
        };

        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
        });

        Ok((name, ResourceDependency::Simple(path.to_string())))
    } else {
        // Treat as simple path
        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(spec).file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
        });

        Ok((name, ResourceDependency::Simple(spec.to_string())))
    }
}

/// Install a single dependency that was just added using the full resolver
async fn install_single_dependency(
    name: &str,
    resource_type: &str,
    _manifest: &Manifest,
    manifest_path: &Path,
) -> Result<()> {
    // Use the install command's logic for a single dependency
    // This ensures proper transitive dependency resolution
    println!("Installing dependency...");

    // Create an install command to install the new dependency
    // The install command will auto-update the lockfile with the new dependency
    let install_cmd = crate::cli::install::InstallCommand::new();

    // Run the install command which will:
    // 1. Resolve all dependencies including transitive ones
    // 2. Install all required files
    // 3. Update the lockfile with proper dependency tracking
    install_cmd.execute_with_manifest_path(Some(manifest_path.to_path_buf())).await?;

    println!("{}", format!("✓ Installed {resource_type} '{name}' successfully").green());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DependencySpec;
    use crate::utils::normalize_path_for_storage;
    use tempfile::TempDir;

    // Helper function to create a test manifest with basic structure
    fn create_test_manifest(manifest_path: &Path) {
        let manifest_content = r#"[sources]

[agents]

[snippets]

[commands]

[mcp-servers]
"#;
        // Ensure parent directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(manifest_path, manifest_content).unwrap();
    }

    // Helper function to create a test manifest with existing sources and dependencies
    fn create_test_manifest_with_content(manifest_path: &Path) {
        let manifest_content = r#"[sources]
existing = "https://github.com/existing/repo.git"

[agents]
existing-agent = "../local/agent.md"

[snippets]
existing-snippet = { source = "existing", path = "snippets/utils.md", version = "v1.0.0" }

[commands]
existing-command = { source = "existing", path = "commands/deploy.md", version = "v1.0.0" }

[mcp-servers]
existing-mcp = "../local/mcp-servers/existing.json"
"#;
        // Ensure parent directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(manifest_path, manifest_content).unwrap();
    }

    // Test existing functions
    #[test]
    fn test_parse_remote_dependency() {
        let (name, dep) =
            parse_dependency_spec("official:agents/reviewer.md@v1.0.0", &None, None).unwrap();

        assert_eq!(name, "reviewer");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("official".to_string()));
            assert_eq!(detailed.path, "agents/reviewer.md");
            assert_eq!(detailed.version, Some("v1.0.0".to_string()));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_local_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "# Test").unwrap();

        let (name, dep) =
            parse_dependency_spec(test_file.to_str().unwrap(), &Some("my-agent".to_string()), None)
                .unwrap();

        assert_eq!(name, "my-agent");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, test_file.to_str().unwrap());
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_with_custom_name() {
        let (name, _) = parse_dependency_spec(
            "official:snippets/utils.md@v1.0.0",
            &Some("my-utils".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(name, "my-utils");
    }

    #[test]
    fn test_parse_dependency_without_version() {
        let (name, dep) = parse_dependency_spec("source:path/to/file.md", &None, None).unwrap();
        assert_eq!(name, "file");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source.as_deref(), Some("source"));
            assert_eq!(detailed.path, "path/to/file.md");
            // Should get default "main" version for Git sources when no manifest provided
            assert_eq!(detailed.version.as_deref(), Some("main"));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_dependency_with_branch() {
        let (name, dep) = parse_dependency_spec("src:file.md@main", &None, None).unwrap();
        assert_eq!(name, "file");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.version.as_deref(), Some("main"));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_dependency_local_source_no_default_version() {
        // Create a test manifest with a local source
        let mut manifest = Manifest::new();
        manifest.sources.insert("local-src".to_string(), "/path/to/local".to_string());

        let (name, dep) =
            parse_dependency_spec("local-src:path/to/file.md", &None, Some(&manifest)).unwrap();
        assert_eq!(name, "file");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source.as_deref(), Some("local-src"));
            assert_eq!(detailed.path, "path/to/file.md");
            // Should NOT get default "main" version for local sources
            assert!(detailed.version.is_none());
        } else {
            panic!("Expected detailed dependency");
        }
    }

    /// Helper function to convert a manifest path to its corresponding lockfile path
    fn manifest_path_to_lockfile(manifest_path: &std::path::Path) -> std::path::PathBuf {
        manifest_path.with_file_name("agpm.lock")
    }

    #[test]
    fn test_manifest_path_to_lockfile() {
        use std::path::PathBuf;

        let manifest = PathBuf::from("/project/agpm.toml");
        let lockfile = manifest_path_to_lockfile(&manifest);
        assert_eq!(lockfile, PathBuf::from("/project/agpm.lock"));

        let manifest2 = PathBuf::from("./agpm.toml");
        let lockfile2 = manifest_path_to_lockfile(&manifest2);
        assert_eq!(lockfile2, PathBuf::from("./agpm.lock"));
    }

    // NEW COMPREHENSIVE TESTS

    #[tokio::test]
    async fn test_execute_add_source() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Source {
                name: "test-source".to_string(),
                url: "https://github.com/test/repo.git".to_string(),
            },
        };

        let result = add_command.execute_with_manifest_path(Some(manifest_path.clone())).await;

        assert!(result.is_ok(), "Failed to execute add source: {result:?}");

        // Verify source was added to manifest
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("test-source"));
        assert_eq!(
            manifest.sources.get("test-source").unwrap(),
            "https://github.com/test/repo.git"
        );
    }

    #[tokio::test]
    async fn test_execute_add_agent_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Create local agent file for testing
        let agent_file = temp_dir.path().join("test-agent.md");
        std::fs::write(&agent_file, "# Test Agent\nThis is a test agent.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Agent(AgentDependency {
                common: DependencySpec {
                    spec: agent_file.to_string_lossy().to_string(),
                    name: Some("my-test-agent".to_string()),
                    tool: None,
                    target: None,
                    filename: None,
                    force: false,
                    no_install: false,
                },
            })),
        };

        // Execute the command - this should now succeed with local files
        let result = add_command.execute_with_manifest_path(Some(manifest_path.clone())).await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local agent: {result:?}");

        // Verify the agent was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.agents.contains_key("my-test-agent"));
    }

    #[tokio::test]
    async fn test_execute_add_snippet_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("test-snippet.md");
        std::fs::write(&snippet_file, "# Test Snippet\nUseful code snippet.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Snippet(SnippetDependency {
                common: DependencySpec {
                    spec: snippet_file.to_string_lossy().to_string(),
                    name: Some("my-snippet".to_string()),
                    tool: None,
                    target: None,
                    filename: None,
                    force: false,
                    no_install: false,
                },
            })),
        };

        let result = add_command.execute_with_manifest_path(Some(manifest_path.clone())).await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local snippet: {result:?}");

        // Verify the snippet was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.snippets.contains_key("my-snippet"));
    }

    #[tokio::test]
    async fn test_execute_add_command_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("test-command.md");
        std::fs::write(&command_file, "# Test Command\nUseful command.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Command(CommandDependency {
                common: DependencySpec {
                    spec: command_file.to_string_lossy().to_string(),
                    name: Some("my-command".to_string()),
                    tool: None,
                    target: None,
                    filename: None,
                    force: false,
                    no_install: false,
                },
            })),
        };

        let result = add_command.execute_with_manifest_path(Some(manifest_path.clone())).await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local command: {result:?}");

        // Verify the command was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.commands.contains_key("my-command"));
    }

    #[tokio::test]
    async fn test_execute_add_mcp_server_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "npx",
            "args": ["-y", "@test/mcp-server"],
            "env": {}
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::McpServer(McpServerDependency {
                common: DependencySpec {
                    spec: mcp_file_path.to_string_lossy().to_string(),
                    name: Some("test-mcp".to_string()),
                    tool: None,
                    target: None,
                    filename: None,
                    force: false,
                    no_install: false,
                },
            })),
        };

        let result = add_command.execute_with_manifest_path(Some(manifest_path.clone())).await;

        assert!(result.is_ok(), "Failed to add MCP server: {result:?}");

        // Verify the manifest was updated
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("test-mcp"));

        // Check that MCP server was configured in .mcp.json (not installed as file)
        let mcp_config_path = temp_dir.path().join(".mcp.json");
        assert!(mcp_config_path.exists(), "MCP config file should be created");

        // Verify the MCP server is in the config
        let mcp_config: serde_json::Value = crate::utils::read_json_file(&mcp_config_path).unwrap();
        assert!(
            mcp_config.get("mcpServers").and_then(|s| s.get("test-mcp")).is_some(),
            "MCP server should be configured in .mcp.json"
        );
    }

    #[tokio::test]
    async fn test_add_source_success() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory

        let source = SourceSpec {
            name: "new-source".to_string(),
            url: "https://github.com/new/repo.git".to_string(),
        };

        let result = add_source_with_manifest_path(source, Some(manifest_path.clone())).await;
        assert!(result.is_ok(), "Failed to add source: {result:?}");

        // Verify source was added
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("new-source"));
        assert_eq!(manifest.sources.get("new-source").unwrap(), "https://github.com/new/repo.git");
    }

    #[tokio::test]
    async fn test_add_source_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory

        let source = SourceSpec {
            name: "existing".to_string(),
            url: "https://github.com/different/repo.git".to_string(),
        };

        let result = add_source_with_manifest_path(source, Some(manifest_path.clone())).await;
        assert!(result.is_err());

        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("Source 'existing' already exists"));
    }

    #[test]
    fn test_parse_dependency_spec_file_prefix() {
        // Test file: prefix - now correctly treated as local path
        let (name, dep) = parse_dependency_spec("file:/path/to/agent.md", &None, None).unwrap();
        assert_eq!(name, "agent");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "/path/to/agent.md"); // Path without file: prefix
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_simple_path() {
        // Test simple path when file doesn't exist
        let (name, dep) = parse_dependency_spec("nonexistent/path.md", &None, None).unwrap();
        assert_eq!(name, "path");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "nonexistent/path.md");
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_custom_name_simple() {
        let (name, dep) =
            parse_dependency_spec("simple/path.md", &Some("custom-name".to_string()), None)
                .unwrap();
        assert_eq!(name, "custom-name");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "simple/path.md");
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_path_without_extension() {
        let (name, dep) = parse_dependency_spec("source:agents/noext@v1.0", &None, None).unwrap();
        assert_eq!(name, "noext");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("source".to_string()));
            assert_eq!(detailed.path, "agents/noext");
            assert_eq!(detailed.version, Some("v1.0".to_string()));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_unknown_fallback() {
        let (name, dep) = parse_dependency_spec("malformed::", &None, None).unwrap();
        // The regex captures :: as the path part after the first colon
        assert_eq!(name, ":"); // Path ":" produces ":" as filename
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("malformed".to_string())); // "malformed" is the source
            assert_eq!(detailed.path, ":"); // ":" is the path
        } else {
            panic!("Expected detailed dependency");
        }
    }

    // Mock test for install_single_dependency - since we can't easily mock the Cache and Git operations,
    // we'll test the error cases and the MCP server special case
    #[tokio::test]
    async fn test_install_single_dependency_mcp_server() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "node",
            "args": ["server.js", "--port=3000"],
            "env": {
                "NODE_ENV": "production"
            }
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        // Create manifest with MCP server dependency
        let manifest_content = format!(
            r#"[sources]

[agents]

[snippets]

[commands]

[mcp-servers]
test-mcp = "{}"
"#,
            normalize_path_for_storage(&mcp_file_path)
        );

        std::fs::write(&manifest_path, manifest_content).unwrap();

        // Load manifest
        let manifest = Manifest::load(&manifest_path).unwrap();

        let result =
            install_single_dependency("test-mcp", "mcp-server", &manifest, &manifest_path).await;

        // MCP servers should install successfully via full install command
        assert!(result.is_ok(), "MCP server installation should succeed: {result:?}");

        // Check that MCP server was configured in .mcp.json (not installed as file)
        let mcp_config_path = temp_dir.path().join(".mcp.json");
        assert!(mcp_config_path.exists(), "MCP config file should be created");

        // Verify the MCP server is in the config
        let mcp_config: serde_json::Value = crate::utils::read_json_file(&mcp_config_path).unwrap();
        assert!(
            mcp_config.get("mcpServers").and_then(|s| s.get("test-mcp")).is_some(),
            "MCP server should be configured in .mcp.json"
        );
    }

    #[tokio::test]
    async fn test_install_single_dependency_invalid_resource_type() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create a test file
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "# Test content").unwrap();

        // Create manifest with invalid resource type in agents section
        let manifest_content = format!(
            r#"[sources]

[agents]
test = "{}"

[snippets]

[commands]
"#,
            normalize_path_for_storage(&test_file)
        );

        std::fs::write(&manifest_path, manifest_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();

        let result = install_single_dependency(
            "test",
            "invalid-type", // Invalid resource type (doesn't match manifest section)
            &manifest,
            &manifest_path,
        )
        .await;

        // The full install command handles this gracefully by using manifest defaults
        // So this now succeeds, as the install command will find the dependency in agents
        assert!(result.is_ok(), "Install should succeed with full command: {result:?}");
    }

    #[tokio::test]
    async fn test_install_single_dependency_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create manifest with a dependency that references a non-existent source
        let manifest_content = r#"[sources]

[agents]
test-agent = { source = "nonexistent-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]

[commands]
"#;
        std::fs::write(&manifest_path, manifest_content).unwrap();

        // The manifest load itself will fail because of the validation that checks
        // if the source exists in the [sources] section
        let manifest_result = Manifest::load(&manifest_path);

        // Should fail during manifest loading due to source validation
        assert!(manifest_result.is_err(), "Should fail to load manifest with nonexistent source");
        let error_msg = manifest_result.err().unwrap().to_string();
        assert!(error_msg.contains("nonexistent-source") || error_msg.contains("not defined"));
    }

    #[tokio::test]
    async fn test_add_dependency_agent_with_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create local agent file for testing (this will be the original)
        let original_agent_file = temp_dir.path().join("original-agent.md");
        std::fs::write(&original_agent_file, "# Original Agent\nOriginal content.").unwrap();

        // Create manifest with existing agent
        let manifest_content = format!(
            r#"[sources]
existing = "https://github.com/existing/repo.git"

[target]
agents = ".claude/agents"
snippets = ".agpm/snippets"
commands = ".claude/commands"

[agents]
existing-agent = "{}"

[snippets]

[commands]
"#,
            normalize_path_for_storage(&original_agent_file)
        );
        std::fs::write(&manifest_path, manifest_content).unwrap();

        // Create local agent file for replacement
        let agent_file = temp_dir.path().join("new-agent.md");
        std::fs::write(&agent_file, "# New Agent\nReplacement agent.").unwrap();

        let dep_type = DependencyType::Agent(AgentDependency {
            common: DependencySpec {
                spec: agent_file.to_string_lossy().to_string(),
                name: Some("existing-agent".to_string()), // Same name as existing
                tool: None,
                target: None,
                filename: None,
                force: true, // Force overwrite
                no_install: false,
            },
        });

        // This should succeed with force flag and overwrite the existing agent
        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        // This should succeed since we're using force flag and a local file
        assert!(result.is_ok(), "Failed to add agent with force flag: {result:?}");

        // Verify the agent was overwritten in the manifest
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.agents.contains_key("existing-agent"));

        // Verify it points to the new file path
        if let ResourceDependency::Simple(path) = manifest.agents.get("existing-agent").unwrap() {
            assert!(path.contains("new-agent.md"));
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[tokio::test]
    async fn test_add_dependency_mcp_server_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "different-command different args".to_string(),
                name: Some("existing-mcp".to_string()), // Same name as existing
                tool: None,
                target: None,
                filename: None,
                force: false, // Don't force overwrite
                no_install: false,
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the mcp server already exists
        assert!(
            error_msg.contains("existing-mcp")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_snippet_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("new-snippet.md");
        std::fs::write(&snippet_file, "# New Snippet\nReplacement snippet.").unwrap();

        // Change to temp directory

        let dep_type = DependencyType::Snippet(SnippetDependency {
            common: DependencySpec {
                spec: snippet_file.to_string_lossy().to_string(),
                name: Some("existing-snippet".to_string()), // Same name as existing
                tool: None,
                target: None,
                filename: None,
                force: false, // Don't force overwrite
                no_install: false,
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the snippet already exists
        assert!(
            error_msg.contains("existing-snippet")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_command_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("new-command.md");
        std::fs::write(&command_file, "# New Command\nReplacement command.").unwrap();

        // Change to temp directory

        let dep_type = DependencyType::Command(CommandDependency {
            common: DependencySpec {
                spec: command_file.to_string_lossy().to_string(),
                name: Some("existing-command".to_string()), // Same name as existing
                tool: None,
                target: None,
                filename: None,
                force: false, // Don't force overwrite
                no_install: false,
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the command already exists
        assert!(
            error_msg.contains("existing-command")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_mcp_server_with_file() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        create_test_manifest(&manifest_path);

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "node",
            "args": ["server.js", "--port=3000"],
            "env": {
                "NODE_ENV": "production"
            }
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: mcp_file_path.to_string_lossy().to_string(),
                name: Some("file-mcp".to_string()),
                tool: None,
                target: None,
                filename: None,
                force: false,
                no_install: false,
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_ok(), "Failed to add MCP server with file: {result:?}");

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("file-mcp"));

        // Check that MCP server was configured in .mcp.json (not installed as file)
        let mcp_config_path = temp_dir.path().join(".mcp.json");
        assert!(mcp_config_path.exists(), "MCP config file should be created");

        // Verify the MCP server is in the config
        let mcp_config: serde_json::Value = crate::utils::read_json_file(&mcp_config_path).unwrap();
        assert!(
            mcp_config.get("mcpServers").and_then(|s| s.get("file-mcp")).is_some(),
            "MCP server should be configured in .mcp.json"
        );
    }
}
