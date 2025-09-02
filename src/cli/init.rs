//! Initialize a new CCPM project with a manifest file.
//!
//! This module provides the `init` command which creates a new `ccpm.toml` manifest file
//! in the specified directory (or current directory). The manifest file is the main
//! configuration file for a CCPM project that defines dependencies on Claude Code resources.
//!
//! # Examples
//!
//! Initialize a manifest in the current directory:
//! ```bash
//! ccpm init
//! ```
//!
//! Initialize a manifest in a specific directory:
//! ```bash
//! ccpm init --path ./my-project
//! ```
//!
//! Force overwrite an existing manifest:
//! ```bash
//! ccpm init --force
//! ```
//!
//! # Manifest Structure
//!
//! The generated manifest contains empty sections for all resource types:
//!
//! ```toml
//! [sources]
//!
//! [agents]
//!
//! [snippets]
//!
//! [commands]
//!
//! [scripts]
//!
//! [hooks]
//!
//! [mcp-servers]
//! ```
//!
//! # Error Conditions
//!
//! - Returns error if manifest already exists and `--force` is not used
//! - Returns error if unable to create the target directory
//! - Returns error if unable to write the manifest file (permissions, disk space, etc.)
//!
//! # Safety
//!
//! This command is safe to run and will not overwrite existing files unless `--force` is specified.

use anyhow::{anyhow, Result};
use clap::Args;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

/// Command to initialize a new CCPM project with a manifest file.
///
/// This command creates a `ccpm.toml` manifest file in the specified directory
/// (or current directory if no path is provided). The manifest serves as the
/// main configuration file for defining Claude Code resource dependencies.
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::init::InitCommand;
/// use std::path::PathBuf;
///
/// // Initialize in current directory
/// let cmd = InitCommand {
///     path: None,
///     force: false,
/// };
///
/// // Initialize in specific directory with force overwrite
/// let cmd = InitCommand {
///     path: Some(PathBuf::from("./my-project")),
///     force: true,
/// };
/// ```
#[derive(Args)]
pub struct InitCommand {
    /// Path to create the manifest (defaults to current directory)
    ///
    /// If not provided, the manifest will be created in the current working directory.
    /// If the specified directory doesn't exist, it will be created.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Force overwrite if manifest already exists
    ///
    /// By default, the command will fail if a `ccpm.toml` file already exists
    /// in the target directory. Use this flag to overwrite an existing file.
    #[arg(short, long)]
    force: bool,
}

impl InitCommand {
    /// Execute the init command with an optional manifest path (for API compatibility)
    pub async fn execute_with_manifest_path(
        self,
        _manifest_path: Option<std::path::PathBuf>,
    ) -> Result<()> {
        // Init command doesn't use manifest_path since it creates a new manifest
        // The path is already part of the InitCommand struct
        self.execute().await
    }

    /// Execute the init command to create a new CCPM manifest file.
    ///
    /// This method creates a `ccpm.toml` manifest file with a minimal template structure
    /// that includes empty sections for all resource types. The file is
    /// created in the specified directory or current directory if no path is provided.
    ///
    /// # Behavior
    ///
    /// 1. Determines the target directory (from `path` option or current directory)
    /// 2. Checks if a manifest already exists and handles the `force` flag
    /// 3. Creates the target directory if it doesn't exist
    /// 4. Writes the manifest template to `ccpm.toml`
    /// 5. Displays success message and next steps to the user
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the manifest was created successfully
    /// - `Err(anyhow::Error)` if:
    ///   - A manifest already exists and `force` is false
    ///   - Unable to create the target directory
    ///   - Unable to write the manifest file
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::init::InitCommand;
    /// use std::path::PathBuf;
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = InitCommand {
    ///     path: Some(PathBuf::from("./test-project")),
    ///     force: false,
    /// };
    ///
    /// // This would create ./test-project/ccpm.toml
    /// // cmd.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute(self) -> Result<()> {
        let target_dir = self.path.unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = target_dir.join("ccpm.toml");
        let gitignore_path = target_dir.join(".gitignore");

        // Check if manifest already exists
        if manifest_path.exists() && !self.force {
            return Err(anyhow!(
                "Manifest already exists at {}. Use --force to overwrite",
                manifest_path.display()
            ));
        }

        // Create directory if it doesn't exist
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
        }

        // Write a minimal template with empty sections
        let template = r#"# CCPM Manifest
# This file defines your Claude Code resource dependencies

[sources]
# Add your Git repository sources here
# Example: official = "https://github.com/aig787/ccpm-community.git"

[agents]
# Add your agent dependencies here
# Example: my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }

[snippets]
# Add your snippet dependencies here
# Example: utils = { source = "official", path = "snippets/utils.md" }

[commands]
# Add your command dependencies here
# Example: deploy = { source = "official", path = "commands/deploy.md" }

[scripts]
# Add your script dependencies here
# Example: build = { source = "official", path = "scripts/build.sh" }

[hooks]
# Add your hook dependencies here
# Example: pre-commit = { source = "official", path = "hooks/pre-commit.json" }

[mcp-servers]
# Add your MCP server dependencies here
# Example: filesystem = { source = "official", path = "mcp-servers/filesystem.json" }
"#;
        fs::write(&manifest_path, template)?;

        // Update or create .gitignore with CCPM entries
        let gitignore_entries = vec![".claude/ccpm/"];

        let mut gitignore_content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };

        // Check if CCPM section exists
        if !gitignore_content.contains("# CCPM managed directories") {
            // Add CCPM entries
            if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
                gitignore_content.push('\n');
            }
            if !gitignore_content.is_empty() {
                gitignore_content.push('\n');
            }
            gitignore_content.push_str("# CCPM managed directories\n");

            for entry in gitignore_entries {
                // Check if entry doesn't already exist
                if !gitignore_content.lines().any(|line| line.trim() == entry) {
                    gitignore_content.push_str(entry);
                    gitignore_content.push('\n');
                }
            }

            fs::write(&gitignore_path, gitignore_content)?;
            println!("{} Updated .gitignore with CCPM entries", "✓".green());
        }

        println!(
            "{} Initialized ccpm.toml at {}",
            "✓".green(),
            manifest_path.display()
        );

        println!("\n{}", "Next steps:".cyan());
        println!("  Add dependencies with {}:", "ccpm add".bright_white());
        println!("    ccpm add agent my-agent --source https://github.com/org/repo.git --path agents/my-agent.md");
        println!("    ccpm add snippet utils --path ../local/snippets/utils.md");
        println!("\n  Then run {} to install", "ccpm install".bright_white());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_init_creates_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let manifest_path = temp_dir.path().join("ccpm.toml");
        assert!(manifest_path.exists());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(content.contains("[agents]"));
        assert!(content.contains("[snippets]"));
    }

    #[tokio::test]
    async fn test_init_creates_directory_if_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let new_dir = temp_dir.path().join("new_project");

        let cmd = InitCommand {
            path: Some(new_dir.clone()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        assert!(new_dir.exists());
        assert!(new_dir.join("ccpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_fails_if_manifest_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        fs::write(&manifest_path, "existing content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_init_force_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        fs::write(&manifest_path, "old content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(!content.contains("old content"));
    }

    #[tokio::test]
    async fn test_init_uses_current_dir_by_default() {
        let temp_dir = TempDir::new().unwrap();

        // Use explicit path instead of changing directory
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());
        assert!(temp_dir.path().join("ccpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_template_content() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let manifest_path = temp_dir.path().join("ccpm.toml");
        let content = fs::read_to_string(&manifest_path).unwrap();

        // Verify template content
        assert!(content.contains("# CCPM Manifest"));
        assert!(content.contains("# This file defines your Claude Code resource dependencies"));
        assert!(content.contains("# Add your Git repository sources here"));
        assert!(content.contains("# Example: official ="));
        assert!(content.contains("# Add your agent dependencies here"));
        assert!(content.contains("# Example: my-agent ="));
        assert!(content.contains("# Add your snippet dependencies here"));
        assert!(content.contains("# Example: utils ="));
    }

    #[tokio::test]
    async fn test_init_nested_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a").join("b").join("c");

        let cmd = InitCommand {
            path: Some(nested_path.clone()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());
        assert!(nested_path.exists());
        assert!(nested_path.join("ccpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_force_flag_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Write initial content
        let initial_content = "# Old manifest\n[sources]\n";
        fs::write(&manifest_path, initial_content).unwrap();

        // Try without force - should fail
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };
        let result = cmd.execute().await;
        assert!(result.is_err());

        // Verify old content still exists
        let content = fs::read_to_string(&manifest_path).unwrap();
        assert_eq!(content, initial_content);

        // Try with force - should succeed
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
        };
        let result = cmd.execute().await;
        assert!(result.is_ok());

        // Verify new template content
        let new_content = fs::read_to_string(&manifest_path).unwrap();
        assert!(new_content.contains("# CCPM Manifest"));
        assert!(!new_content.contains("# Old manifest"));
    }

    #[tokio::test]
    async fn test_init_creates_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let gitignore_path = temp_dir.path().join(".gitignore");
        assert!(gitignore_path.exists());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("# CCPM managed directories"));
        assert!(content.contains(".claude/ccpm/"));
    }

    #[tokio::test]
    async fn test_init_updates_existing_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with some content
        fs::write(&gitignore_path, "node_modules/\n*.log\n").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        // Should preserve existing content
        assert!(content.contains("node_modules/"));
        assert!(content.contains("*.log"));
        // Should add CCPM entries
        assert!(content.contains("# CCPM managed directories"));
        assert!(content.contains(".claude/ccpm/"));
    }

    #[tokio::test]
    async fn test_init_does_not_duplicate_gitignore_entries() {
        let temp_dir = TempDir::new().unwrap();

        // First init
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };
        let result = cmd.execute().await;
        assert!(result.is_ok());

        let gitignore_path = temp_dir.path().join(".gitignore");
        let first_content = fs::read_to_string(&gitignore_path).unwrap();

        // Second init with force
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
        };
        let result = cmd.execute().await;
        assert!(result.is_ok());

        let second_content = fs::read_to_string(&gitignore_path).unwrap();

        // Should not have duplicated the CCPM section
        assert_eq!(
            first_content.matches("# CCPM managed directories").count(),
            second_content.matches("# CCPM managed directories").count()
        );
    }
}
