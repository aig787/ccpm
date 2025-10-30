//! Remove sources and dependencies from AGPM projects.
//!
//! This module provides the `remove` command which allows users to remove
//! sources and dependencies from the project manifest (`agpm.toml`).
//! It complements the `add` command by providing removal functionality.
//!
//! # Features
//!
//! - **Source Removal**: Remove Git repository sources from the manifest
//! - **Dependency Removal**: Remove agents, snippets, commands, and MCP servers
//! - **Safe Operations**: Validates existence before removing
//! - **Clear Feedback**: Provides confirmation of what was removed
//!
//! # Examples
//!
//! Remove a source:
//! ```bash
//! agpm remove source private
//! ```
//!
//! Remove dependencies:
//! ```bash
//! agpm remove dep agent code-reviewer
//! agpm remove dep snippet utils
//! agpm remove dep command deploy
//! agpm remove dep mcp-server filesystem
//! ```
//!
//! Force removal without confirmation:
//! ```bash
//! agpm remove source old-repo --force
//! ```

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};
use colored::Colorize;

use crate::core::ResourceType;
use crate::lockfile::LockFile;
use crate::manifest::{Manifest, ResourceDependency, find_manifest_with_optional};
use std::collections::HashMap;
use std::path::PathBuf;

/// Command to remove sources and dependencies from a AGPM project.
#[derive(Args)]
pub struct RemoveCommand {
    /// The specific remove operation to perform
    #[command(subcommand)]
    command: RemoveSubcommand,
}

/// Subcommands for the remove command.
#[derive(Subcommand)]
enum RemoveSubcommand {
    /// Remove a Git repository source from the manifest
    Source {
        /// Name of the source to remove
        name: String,

        /// Force removal without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Remove a resource dependency from the manifest
    #[command(subcommand)]
    Dep(RemoveDependencySubcommand),
}

/// Dependency removal subcommands for different resource types
#[derive(Subcommand)]
enum RemoveDependencySubcommand {
    /// Remove an agent dependency
    Agent {
        /// Name of the agent to remove
        name: String,
    },

    /// Remove a snippet dependency
    Snippet {
        /// Name of the snippet to remove
        name: String,
    },

    /// Remove a command dependency
    Command {
        /// Name of the command to remove
        name: String,
    },

    /// Remove an MCP server dependency
    McpServer {
        /// Name of the MCP server to remove
        name: String,
    },

    /// Remove a script dependency
    Script {
        /// Name of the script to remove
        name: String,
    },

    /// Remove a hook dependency
    Hook {
        /// Name of the hook to remove
        name: String,
    },

    /// Remove a skill dependency
    Skill {
        /// Name of the skill to remove
        name: String,
    },
}

/// Helper function to get dependencies for a specific resource type
const fn get_dependencies_for_type(
    manifest: &Manifest,
    resource_type: ResourceType,
) -> &HashMap<String, ResourceDependency> {
    match resource_type {
        ResourceType::Agent => &manifest.agents,
        ResourceType::Snippet => &manifest.snippets,
        ResourceType::Command => &manifest.commands,
        ResourceType::McpServer => &manifest.mcp_servers,
        ResourceType::Script => &manifest.scripts,
        ResourceType::Hook => &manifest.hooks,
        ResourceType::Skill => &manifest.skills,
    }
}

/// Helper function to get mutable dependencies for a specific resource type
const fn get_dependencies_for_type_mut(
    manifest: &mut Manifest,
    resource_type: ResourceType,
) -> &mut HashMap<String, ResourceDependency> {
    match resource_type {
        ResourceType::Agent => &mut manifest.agents,
        ResourceType::Snippet => &mut manifest.snippets,
        ResourceType::Command => &mut manifest.commands,
        ResourceType::McpServer => &mut manifest.mcp_servers,
        ResourceType::Script => &mut manifest.scripts,
        ResourceType::Hook => &mut manifest.hooks,
        ResourceType::Skill => &mut manifest.skills,
    }
}

/// Helper function to get the installed path for a resource from lockfile
fn get_installed_path_from_lockfile(
    lockfile: &LockFile,
    name: &str,
    resource_type: ResourceType,
    project_root: &std::path::Path,
    _manifest: &Manifest,
) -> Option<std::path::PathBuf> {
    // Use lockfile's get_resources method to find the resource by lookup_name
    lockfile
        .get_resources(&resource_type)
        .iter()
        .find(|r| r.lookup_name() == name)
        .map(|r| project_root.join(&r.installed_at))
}

/// Helper function to remove a resource from lockfile
fn remove_from_lockfile(lockfile: &mut LockFile, name: &str, resource_type: ResourceType) {
    // Use lockfile's get_resources_mut method to get mutable reference and retain by lookup_name
    let resources = lockfile.get_resources_mut(&resource_type);
    resources.retain(|r| r.lookup_name() != name);
}

impl RemoveCommand {
    /// Execute the remove command with an optional manifest path.
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
    /// - `Ok(())` if the remove operation completed successfully
    /// - `Err(anyhow::Error)` if the operation fails (e.g., dependency not found, manifest issues)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use agpm_cli::cli::remove::{RemoveCommand, RemoveSubcommand};
    /// use std::path::PathBuf;
    ///
    /// let cmd = RemoveCommand {
    ///     command: RemoveSubcommand::Source {
    ///         name: "my-source".to_string(),
    ///         force: false,
    ///     }
    /// };
    ///
    /// cmd.execute_with_manifest_path(None).await?;
    /// ```
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        match self.command {
            RemoveSubcommand::Source {
                name,
                force,
            } => remove_source_with_manifest_path(&name, force, manifest_path).await,
            RemoveSubcommand::Dep(dep_command) => match dep_command {
                RemoveDependencySubcommand::Agent {
                    name,
                } => remove_dependency_with_manifest_path(&name, "agent", manifest_path).await,
                RemoveDependencySubcommand::Snippet {
                    name,
                } => remove_dependency_with_manifest_path(&name, "snippet", manifest_path).await,
                RemoveDependencySubcommand::Command {
                    name,
                } => remove_dependency_with_manifest_path(&name, "command", manifest_path).await,
                RemoveDependencySubcommand::McpServer {
                    name,
                } => remove_dependency_with_manifest_path(&name, "mcp-server", manifest_path).await,
                RemoveDependencySubcommand::Script {
                    name,
                } => remove_dependency_with_manifest_path(&name, "script", manifest_path).await,
                RemoveDependencySubcommand::Hook {
                    name,
                } => remove_dependency_with_manifest_path(&name, "hook", manifest_path).await,
                RemoveDependencySubcommand::Skill {
                    name,
                } => remove_dependency_with_manifest_path(&name, "skill", manifest_path).await,
            },
        }
    }
}

/// Remove a source from the manifest with optional manifest path
async fn remove_source_with_manifest_path(
    name: &str,
    force: bool,
    manifest_path: Option<PathBuf>,
) -> Result<()> {
    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Check if source exists
    if !manifest.sources.contains_key(name) {
        return Err(anyhow!("Source '{name}' not found in manifest"));
    }

    // Check if source is being used by any dependencies
    if !force {
        let mut used_by = Vec::new();

        // Iterate over all resource types to check for dependencies
        for resource_type in ResourceType::all() {
            let dependencies = get_dependencies_for_type(&manifest, *resource_type);
            for (dep_name, dep) in dependencies {
                if dep.get_source() == Some(name) {
                    used_by.push(format!("{resource_type} '{dep_name}'"));
                }
            }
        }

        if !used_by.is_empty() {
            return Err(anyhow!(
                "Source '{}' is still being used by: {}. Use --force to remove anyway",
                name,
                used_by.join(", ")
            ));
        }
    }

    // Remove the source
    manifest.sources.remove(name);

    // Save the manifest
    manifest.save(&manifest_path)?;

    // Update lockfile to remove entries from this source
    let lockfile_path = manifest_path.parent().unwrap().join("agpm.lock");

    if lockfile_path.exists() {
        // Create command context for enhanced lockfile loading
        let project_root = manifest_path.parent().unwrap();
        let command_context =
            crate::cli::common::CommandContext::new(manifest.clone(), project_root.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let mut lockfile = match command_context.load_lockfile_with_regeneration(true, "remove")? {
            Some(lockfile) => lockfile,
            None => {
                // Lockfile was invalid and has been removed, nothing to update
                return Ok(());
            }
        };

        // Find and remove installed files from this source
        let agents_to_remove: Vec<String> = lockfile
            .agents
            .iter()
            .filter(|a| a.source.as_deref() == Some(name))
            .map(|a| a.installed_at.clone())
            .collect();

        let snippets_to_remove: Vec<String> = lockfile
            .snippets
            .iter()
            .filter(|s| s.source.as_deref() == Some(name))
            .map(|s| s.installed_at.clone())
            .collect();

        let commands_to_remove: Vec<String> = lockfile
            .commands
            .iter()
            .filter(|c| c.source.as_deref() == Some(name))
            .map(|c| c.installed_at.clone())
            .collect();

        // Delete all installed files from this source
        for path_str in agents_to_remove
            .iter()
            .chain(snippets_to_remove.iter())
            .chain(commands_to_remove.iter())
        {
            let path = project_root.join(path_str);
            if path.exists() {
                tokio::fs::remove_file(&path).await.with_context(|| {
                    format!("Failed to remove installed file: {}", path.display())
                })?;
            }
        }

        // Remove the source from lockfile
        lockfile.sources.retain(|s| s.name != name);

        // Remove all dependencies from this source for all resource types
        lockfile.agents.retain(|a| a.source.as_deref() != Some(name));
        lockfile.snippets.retain(|s| s.source.as_deref() != Some(name));
        lockfile.commands.retain(|c| c.source.as_deref() != Some(name));
        lockfile.mcp_servers.retain(|m| m.source.as_deref() != Some(name));
        lockfile.scripts.retain(|s| s.source.as_deref() != Some(name));
        lockfile.hooks.retain(|h| h.source.as_deref() != Some(name));

        // Save the updated lockfile
        lockfile.save(&lockfile_path)?;

        // Update private lockfile - remove entries for removed resources
        use crate::lockfile::PrivateLockFile;
        if let Ok(Some(mut private_lock)) = PrivateLockFile::load(project_root) {
            // Remove entries for resources that were removed
            for agent_name in &agents_to_remove {
                private_lock.agents.retain(|r| &r.name != agent_name);
            }
            for snippet_name in &snippets_to_remove {
                private_lock.snippets.retain(|r| &r.name != snippet_name);
            }
            for command_name in &commands_to_remove {
                private_lock.commands.retain(|r| &r.name != command_name);
            }
            // Save (will delete if empty)
            private_lock.save(project_root)?;
        }
    }

    println!("{}", format!("Removed source '{name}'").green());

    Ok(())
}

/// Remove a dependency from the manifest with optional manifest path
async fn remove_dependency_with_manifest_path(
    name: &str,
    dep_type: &str,
    manifest_path: Option<PathBuf>,
) -> Result<()> {
    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Parse the resource type
    let resource_type: ResourceType =
        dep_type.parse().map_err(|_| anyhow!("Invalid dependency type: {dep_type}"))?;

    // Get the dependencies for this resource type and check if it exists
    let dependencies = get_dependencies_for_type_mut(&mut manifest, resource_type);

    if !dependencies.contains_key(name) {
        let type_display = dep_type.replace('-', " ");
        return Err(anyhow!(
            "{} '{}' not found in manifest",
            type_display.chars().next().unwrap().to_uppercase().collect::<String>()
                + &type_display[1..],
            name
        ));
    }

    // Remove the dependency
    let removed = dependencies.remove(name).is_some();

    if !removed {
        return Err(anyhow!("{} '{}' not found in manifest", dep_type.replace('-', " "), name));
    }

    // Save the manifest
    manifest.save(&manifest_path)?;

    let dep_type_display = dep_type.replace('-', " ");
    println!("{}", format!("Removed {dep_type_display} '{name}'").green());

    let project_root = manifest_path.parent().unwrap();

    // For MCP servers and hooks, also update the settings file
    let settings_path = project_root.join(".claude/settings.local.json");
    if settings_path.exists() {
        match resource_type {
            ResourceType::McpServer => {
                let mut settings = crate::mcp::ClaudeSettings::load_or_default(&settings_path)?;
                if let Some(servers) = &mut settings.mcp_servers {
                    servers.remove(name);
                }
                settings.save(&settings_path)?;
            }
            ResourceType::Hook => {
                let mut settings = crate::mcp::ClaudeSettings::load_or_default(&settings_path)?;
                if let Some(hooks) = &mut settings.hooks
                    && let Some(hooks_obj) = hooks.as_object_mut()
                {
                    hooks_obj.remove(name);
                }
                settings.save(&settings_path)?;
            }
            _ => {}
        }
    }

    // Update lockfile and remove installed files
    let lockfile_path = manifest_path.parent().unwrap().join("agpm.lock");
    if lockfile_path.exists() {
        // Create command context for enhanced lockfile loading
        let project_root = manifest_path.parent().unwrap();
        let command_context =
            crate::cli::common::CommandContext::new(manifest.clone(), project_root.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let mut lockfile = match command_context.load_lockfile_with_regeneration(true, "remove")? {
            Some(lockfile) => lockfile,
            None => {
                // Lockfile was invalid and has been removed, nothing to update
                return Ok(());
            }
        };

        // Find the installed file path and remove it
        let installed_path = get_installed_path_from_lockfile(
            &lockfile,
            name,
            resource_type,
            project_root,
            &manifest,
        );

        // Delete the installed file if it exists
        if let Some(path) = installed_path
            && path.exists()
        {
            // Skills are directories, all other resources are files
            if resource_type == ResourceType::Skill {
                // Remove skill directory recursively
                tokio::fs::remove_dir_all(&path).await.with_context(|| {
                    format!("Failed to remove skill directory: {}", path.display())
                })?;
            } else {
                // Remove single file for other resource types
                tokio::fs::remove_file(&path).await.with_context(|| {
                    format!("Failed to remove installed file: {}", path.display())
                })?;
            }
        }

        // Remove the dependency from the appropriate section
        remove_from_lockfile(&mut lockfile, name, resource_type);

        // Save the updated lockfile
        lockfile.save(&lockfile_path)?;

        // Update private lockfile - remove entry for this resource
        use crate::lockfile::PrivateLockFile;
        if let Ok(Some(mut private_lock)) = PrivateLockFile::load(project_root) {
            // Remove the entry based on resource type
            match resource_type {
                ResourceType::Agent => {
                    private_lock.agents.retain(|r| r.name != name);
                }
                ResourceType::Snippet => {
                    private_lock.snippets.retain(|r| r.name != name);
                }
                ResourceType::Command => {
                    private_lock.commands.retain(|r| r.name != name);
                }
                ResourceType::Script => {
                    private_lock.scripts.retain(|r| r.name != name);
                }
                ResourceType::McpServer => {
                    private_lock.mcp_servers.retain(|r| r.name != name);
                }
                ResourceType::Hook => {
                    private_lock.hooks.retain(|r| r.name != name);
                }
                ResourceType::Skill => {
                    private_lock.skills.retain(|r| r.name != name);
                }
            }
            // Save (will delete if empty)
            private_lock.save(project_root)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_remove_source_not_found() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a minimal manifest
        let manifest_content = r#"
[sources]
existing = "https://github.com/test/repo.git"

[agents]
[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Change to temp directory

        // Try to remove non-existent source
        let result =
            remove_source_with_manifest_path("nonexistent", false, Some(manifest_path.clone()))
                .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_remove_source_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with sources
        let manifest_content = r#"
[sources]
test-source = "https://github.com/test/repo.git"
another-source = "https://github.com/another/repo.git"

[agents]
[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a source
        let result =
            remove_source_with_manifest_path("test-source", false, Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.sources.contains_key("test-source"));
        assert!(manifest.sources.contains_key("another-source"));
    }

    #[tokio::test]
    async fn test_remove_source_in_use() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a source in use
        let manifest_content = r#"
[sources]
used-source = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "used-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Try to remove a source in use without force
        let result =
            remove_source_with_manifest_path("used-source", false, Some(manifest_path.clone()))
                .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("still being used"));
    }

    #[tokio::test]
    async fn test_remove_source_in_use_with_force() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a source in use
        let manifest_content = r#"
[sources]
used-source = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "used-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a source in use with force
        let result =
            remove_source_with_manifest_path("used-source", true, Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify the source was removed from the raw TOML
        // (can't use Manifest::load since the dependency still references the removed source)
        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(!content.contains("used-source = \"https://github.com/test/repo.git\""));
    }

    #[tokio::test]
    async fn test_remove_dependency_not_found() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a minimal manifest
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Try to remove non-existent agent
        let result = remove_dependency_with_manifest_path(
            "nonexistent",
            "agent",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_remove_agent_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with an agent
        let manifest_content = r#"
[sources]
[agents]
test-agent = "../test/agent.md"
another-agent = "../test/another.md"

[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove an agent
        let result = remove_dependency_with_manifest_path(
            "test-agent",
            "agent",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.agents.contains_key("test-agent"));
        assert!(manifest.agents.contains_key("another-agent"));
    }

    #[tokio::test]
    async fn test_remove_snippet_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a snippet
        let manifest_content = r#"
[sources]
[agents]
[snippets]
test-snippet = "../test/snippet.md"

[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a snippet
        let result = remove_dependency_with_manifest_path(
            "test-snippet",
            "snippet",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.snippets.contains_key("test-snippet"));
    }

    #[tokio::test]
    async fn test_remove_command_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a command
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
test-command = "../test/command.md"

[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a command
        let result = remove_dependency_with_manifest_path(
            "test-command",
            "command",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.commands.contains_key("test-command"));
    }

    #[tokio::test]
    async fn test_remove_mcp_server_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with an MCP server
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
test-server = "../local/mcp-servers/test-server.json"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove an MCP server
        let result = remove_dependency_with_manifest_path(
            "test-server",
            "mcp-server",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.mcp_servers.contains_key("test-server"));
    }

    #[tokio::test]
    async fn test_remove_script_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a script
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
[scripts]
test-script = "../test/script.sh"
another-script = "../test/another.sh"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a script
        let result = remove_dependency_with_manifest_path(
            "test-script",
            "script",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.scripts.contains_key("test-script"));
        assert!(manifest.scripts.contains_key("another-script"));
    }

    #[tokio::test]
    async fn test_remove_hook_success() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a hook
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
[scripts]
[hooks]
pre-commit = "../test/hook.json"
post-commit = "../test/another_hook.json"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Remove a hook
        let result =
            remove_dependency_with_manifest_path("pre-commit", "hook", Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify it was removed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(!manifest.hooks.contains_key("pre-commit"));
        assert!(manifest.hooks.contains_key("post-commit"));
    }

    #[tokio::test]
    async fn test_remove_invalid_dependency_type() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a minimal manifest
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Try to remove with invalid type
        let result = remove_dependency_with_manifest_path(
            "test",
            "invalid-type",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid dependency type"));
    }

    #[tokio::test]
    async fn test_remove_dependency_with_lockfile_suggestion() {
        use crate::lockfile::{LockFile, LockedResource};

        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");

        // Create a manifest
        let manifest_content = r#"
[sources]
[agents]
test-agent = "../test/agent.md"

[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create a valid lockfile with the agent
        let mut lockfile = LockFile::new();
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: None,
            url: None,
            path: "../test/agent.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
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
        });
        lockfile.save(&lockfile_path).unwrap();
        // Remove an agent (should update lockfile)
        let result = remove_dependency_with_manifest_path(
            "test-agent",
            "agent",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify the agent was removed from lockfile
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.agents.len(), 0, "Agent should be removed from lockfile");
    }

    #[tokio::test]
    async fn test_remove_source_checks_all_dependency_types() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest with a source used by different dependency types
        let manifest_content = r#"
[sources]
used-source = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "used-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]
test-snippet = { source = "used-source", path = "snippets/test.md", version = "v1.0.0" }

[commands]
test-command = { source = "used-source", path = "commands/test.md", version = "v1.0.0" }

[mcp-servers]
test-server = { source = "used-source", path = "servers/test.toml", version = "v1.0.0", command = "npx", args = ["test"] }

[scripts]
test-script = { source = "used-source", path = "scripts/test.sh", version = "v1.0.0" }

[hooks]
test-hook = { source = "used-source", path = "hooks/test.json", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Try to remove source without force
        let result =
            remove_source_with_manifest_path("used-source", false, Some(manifest_path.clone()))
                .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("agent 'test-agent'"));
        assert!(err_msg.contains("snippet 'test-snippet'"));
        assert!(err_msg.contains("command 'test-command'"));
        assert!(err_msg.contains("mcp-server 'test-server'"));
        assert!(err_msg.contains("script 'test-script'"));
        assert!(err_msg.contains("hook 'test-hook'"));
    }

    #[tokio::test]
    async fn test_execute_remove_command() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create a manifest
        let manifest_content = r#"
[sources]
test = "https://github.com/test/repo.git"

[agents]
[snippets]
[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Test execute method directly
        let cmd = RemoveCommand {
            command: RemoveSubcommand::Source {
                name: "test".to_string(),
                force: false,
            },
        };
        let result = cmd.execute_with_manifest_path(Some(manifest_path.clone())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_remove_deletes_installed_files() {
        use crate::lockfile::{LockedResource, LockedSource};

        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();
        let manifest_path = project_dir.join("agpm.toml");
        let lockfile_path = project_dir.join("agpm.lock");

        // Create manifest with a dependency
        let manifest = r#"
[sources]
test-source = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "test-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]
test-snippet = { source = "test-source", path = "snippets/test.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, manifest).unwrap();

        // Create lockfile with installed paths
        let mut lockfile = LockFile {
            version: 1,
            ..Default::default()
        };

        // Add sources
        lockfile.sources.push(LockedSource {
            name: "test-source".to_string(),
            url: "https://github.com/test/repo.git".to_string(),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Add agent with installed path (relative to project directory)
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("test-source".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/agents/test-agent.md".to_string(),
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

        // Add snippet with installed path (relative to project directory)
        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("test-source".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "snippets/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/snippets/test-snippet.md".to_string(),
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

        lockfile.save(&lockfile_path).unwrap();

        // Create the installed files in the project directory
        let agent_dir = project_dir.join(".claude/agents");
        let snippet_dir = project_dir.join(".claude/snippets");
        let agent_file = agent_dir.join("test-agent.md");
        let snippet_file = snippet_dir.join("test-snippet.md");

        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::create_dir_all(&snippet_dir).unwrap();
        std::fs::write(&agent_file, "# Test Agent").unwrap();
        std::fs::write(&snippet_file, "# Test Snippet").unwrap();

        // Verify files exist
        assert!(agent_file.exists(), "Agent file should exist before removal");
        assert!(snippet_file.exists(), "Snippet file should exist before removal");

        // Remove the snippet
        remove_dependency_with_manifest_path(
            "test-snippet",
            "snippet",
            Some(manifest_path.clone()),
        )
        .await
        .unwrap();

        // Verify snippet file was deleted
        assert!(!snippet_file.exists(), "Snippet file should be deleted after removal");
        // Agent file should still exist
        assert!(agent_file.exists(), "Agent file should still exist after snippet removal");

        // Remove the source (should remove remaining agent)
        remove_source_with_manifest_path("test-source", true, Some(manifest_path.clone()))
            .await
            .unwrap();

        // Verify agent file was also deleted
        assert!(!agent_file.exists(), "Agent file should be deleted after source removal");

        // Verify lockfile was updated
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.agents.len(), 0, "No agents should remain in lockfile");
        assert_eq!(updated_lockfile.snippets.len(), 0, "No snippets should remain in lockfile");
        assert_eq!(updated_lockfile.sources.len(), 0, "No sources should remain in lockfile");
    }

    #[tokio::test]
    async fn test_remove_script_and_hook_from_lockfile() {
        use crate::lockfile::{LockFile, LockedResource};

        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");

        // Create a manifest with scripts and hooks
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
[scripts]
test-script = "../test/script.sh"

[hooks]
test-hook = "../test/hook.json"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create a lockfile with script and hook
        let mut lockfile = LockFile::new();
        lockfile.scripts.push(LockedResource {
            name: "test-script".to_string(),
            source: None,
            url: None,
            path: "../test/script.sh".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/scripts/test-script.sh".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Script,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });
        lockfile.hooks.push(LockedResource {
            name: "test-hook".to_string(),
            source: None,
            url: None,
            path: "../test/hook.json".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/hooks/test-hook.json".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Hook,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });
        lockfile.save(&lockfile_path).unwrap();
        // Remove script
        let result = remove_dependency_with_manifest_path(
            "test-script",
            "script",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify script was removed from lockfile
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.scripts.len(), 0);
        assert_eq!(updated_lockfile.hooks.len(), 1);

        // Remove hook
        let result =
            remove_dependency_with_manifest_path("test-hook", "hook", Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify hook was removed from lockfile
        let final_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(final_lockfile.hooks.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_updates_lockfile() {
        use crate::lockfile::{LockFile, LockedResource, LockedSource};

        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");

        // Create a manifest with dependencies
        let manifest_content = r#"
[sources]
test-source = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "test-source", path = "agents/test.md", version = "v1.0.0" }

[snippets]
test-snippet = "../local/snippet.md"

[commands]
[mcp-servers]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create a lockfile with entries
        let mut lockfile = LockFile::new();
        lockfile.sources.push(LockedSource {
            name: "test-source".to_string(),
            url: "https://github.com/test/repo.git".to_string(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("test-source".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
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
        });
        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: None,
            url: None,
            path: "../local/snippet.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            installed_at: "snippets/test-snippet.md".to_string(),
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
        lockfile.save(&lockfile_path).unwrap();
        // Remove a snippet
        let result = remove_dependency_with_manifest_path(
            "test-snippet",
            "snippet",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify lockfile was updated
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.snippets.len(), 0, "Snippet should be removed from lockfile");
        assert_eq!(updated_lockfile.agents.len(), 1, "Agent should still be in lockfile");

        // Remove the agent
        let result = remove_dependency_with_manifest_path(
            "test-agent",
            "agent",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify lockfile was updated again
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.agents.len(), 0, "Agent should be removed from lockfile");
        assert_eq!(updated_lockfile.sources.len(), 1, "Source should still be in lockfile");

        // Remove the source
        let result =
            remove_source_with_manifest_path("test-source", false, Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify source was removed from lockfile
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.sources.len(), 0, "Source should be removed from lockfile");
    }

    #[tokio::test]
    async fn test_remove_mcp_server_updates_settings() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let settings_dir = temp.path().join(".claude");
        let settings_path = settings_dir.join("settings.local.json");

        // Create manifest with MCP server
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
test-server = "../mcp/test-server.json"
[scripts]
[hooks]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create .claude directory and settings file
        std::fs::create_dir_all(&settings_dir).unwrap();
        let settings_content = r#"
{
  "mcpServers": {
    "test-server": {
      "command": "node",
      "args": ["test.js"]
    },
    "other-server": {
      "command": "python",
      "args": ["other.py"]
    }
  }
}
"#;
        fs::write(&settings_path, settings_content).unwrap();

        // Remove MCP server
        let result = remove_dependency_with_manifest_path(
            "test-server",
            "mcp-server",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify settings file was updated (test-server removed but other-server remains)
        let updated_settings = fs::read_to_string(&settings_path).unwrap();
        assert!(!updated_settings.contains("test-server"));
        assert!(updated_settings.contains("other-server"));
    }

    #[tokio::test]
    async fn test_remove_hook_updates_settings() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let settings_dir = temp.path().join(".claude");
        let settings_path = settings_dir.join("settings.local.json");

        // Create manifest with hook
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
[scripts]
[hooks]
test-hook = "../hooks/test-hook.json"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create .claude directory and settings file
        std::fs::create_dir_all(&settings_dir).unwrap();
        let settings_content = r#"
{
  "hooks": {
    "test-hook": {
      "command": "echo test"
    },
    "other-hook": {
      "command": "echo other"
    }
  }
}
"#;
        fs::write(&settings_path, settings_content).unwrap();

        // Remove hook
        let result =
            remove_dependency_with_manifest_path("test-hook", "hook", Some(manifest_path.clone()))
                .await;
        assert!(result.is_ok());

        // Verify settings file was updated (test-hook removed but other-hook remains)
        let updated_settings = fs::read_to_string(&settings_path).unwrap();
        assert!(!updated_settings.contains("test-hook"));
        assert!(updated_settings.contains("other-hook"));
    }

    #[tokio::test]
    async fn test_remove_script_with_lockfile_entry() {
        use crate::lockfile::{LockFile, LockedResource};

        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");
        let script_dir = temp.path().join(".claude/scripts");
        let script_file = script_dir.join("test-script.sh");

        // Create manifest with script
        let manifest_content = r#"
[sources]
[agents]
[snippets]
[commands]
[mcp-servers]
[scripts]
test-script = "../test/script.sh"
[hooks]
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create lockfile with script entry
        let mut lockfile = LockFile::new();
        lockfile.scripts.push(LockedResource {
            name: "test-script".to_string(),
            source: None,
            url: None,
            path: "../test/script.sh".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/scripts/test-script.sh".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Script,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });
        lockfile.save(&lockfile_path).unwrap();

        // Create the actual script file
        std::fs::create_dir_all(&script_dir).unwrap();
        fs::write(&script_file, "#!/bin/bash\necho test").unwrap();
        assert!(script_file.exists());

        // Remove script
        let result = remove_dependency_with_manifest_path(
            "test-script",
            "script",
            Some(manifest_path.clone()),
        )
        .await;
        assert!(result.is_ok());

        // Verify script file was deleted
        assert!(!script_file.exists());

        // Verify lockfile was updated
        let updated_lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(updated_lockfile.scripts.len(), 0);
    }
}
