//! Common test utilities and fixtures for AGPM integration tests
//!
//! This module consolidates frequently used test patterns to reduce duplication
//! and improve test maintainability.
//!
//! # Quick Start Guide
//!
//! ## Creating Test Projects
//!
//! ```rust
//! let project = TestProject::new().await?;
//! ```
//!
//! ## Creating Repositories
//!
//! ### Simple v1.0.0 Repository (Most Common)
//! ```rust
//! // Old way (6 lines):
//! let repo = project.create_source_repo("official").await?;
//! repo.create_standard_resources().await?;
//! repo.commit_all("Initial commit")?;
//! repo.tag_version("v1.0.0")?;
//! let url = repo.bare_file_url(project.sources_path())?;
//!
//! // New way (1 line):
//! let (repo, url) = project.create_standard_v1_repo("official").await?;
//! ```
//!
//! ## Creating Manifests
//!
//! ### With ManifestBuilder (Recommended)
//! ```rust
//! // Old way (10+ lines of format! strings):
//! let manifest = format!(r#"
//! [sources]
//! official = "{}"
//! community = "{}"
//!
//! [agents]
//! my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
//! helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
//! "#, official_url, community_url);
//!
//! // New way (5 lines, type-safe):
//! let manifest = ManifestBuilder::new()
//!     .add_sources(&[("official", &official_url), ("community", &community_url)])
//!     .add_standard_agent("my-agent", "official", "agents/my-agent.md")
//!     .add_standard_agent("helper", "community", "agents/helper.md")
//!     .build();
//! ```
//!
//! ### Sequential Resources (Stress Tests)
//! ```rust
//! // Old way (loop):
//! for i in 0..10 {
//!     repo.add_resource("agents", &format!("agent-{:02}", i), ...).await?;
//! }
//!
//! // New way (1 line):
//! repo.add_sequential_resources("agents", "agent", 10).await?;
//! ```
//!
//! ## Helper Method Summary
//!
//! ### TestProject
//! - `new()` - Create test project with temp directories
//! - `create_source_repo(name)` - Create empty source repository
//! - `create_standard_v1_repo(name)` - **NEW**: Create repo with v1.0.0 tag
//! - `write_manifest(content)` - Write agpm.toml
//! - `run_agpm(args)` - Run AGPM CLI command
//!
//! ### ManifestBuilder
//! - `new()` - Create new builder
//! - `add_source(name, url)` - Add source repository
//! - `add_standard_agent(name, source, path)` - Add agent with v1.0.0
//! - `add_agent(name, config)` - Add agent with full config
//! - `add_local_agent(name, path)` - Add local agent (no source/version)
//! - See `manifest_builder` module for full API
//!
//! ### TestSourceRepo
//! - `add_resource(type, name, content)` - Add single resource file
//! - `add_sequential_resources(type, prefix, count)` - **NEW**: Add N sequential resources
//! - `create_standard_resources()` - Add agent, snippet, command
//! - `commit_all(message)` - Commit all changes
//! - `tag_version(version)` - Create version tag
//! - `bare_file_url(sources_path)` - Get file:// URL for testing
//!
//! ### Assertions
//! - `FileAssert::exists(path)` - Assert file exists
//! - `FileAssert::contains(path, text)` - Assert file contains text
//! - `DirAssert::exists(path)` - Assert directory exists
//! - `CommandOutput::assert_success()` - Assert command succeeded
//! - `CommandOutput::assert_stdout_contains(text)` - Assert stdout contains text

// Allow dead code because these utilities are used across different test files
// and not all utilities are used in every test file
#![allow(dead_code)]

use agpm_cli::lockfile::LockFile;
use agpm_cli::utils::normalize_path_for_storage;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use tokio::fs;

// Manifest builder for type-safe test manifest creation
mod manifest_builder;
#[allow(unused_imports)] // Used by integration tests, not stress tests
pub use manifest_builder::{
    DependencyBuilder, ManifestBuilder, ResourceConfigBuilder, TargetConfigBuilder,
    ToolConfigBuilder, ToolsConfigBuilder,
};

/// Git command builder for tests
pub struct TestGit {
    repo_path: PathBuf,
}

impl TestGit {
    fn run_git_command(&self, args: &[&str], action: &str) -> Result<std::process::Output> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .with_context(|| action.to_string())?;

        if !output.status.success() {
            bail!("{} failed: {}", action, String::from_utf8_lossy(&output.stderr));
        }

        Ok(output)
    }

    /// Create a new TestGit instance for the given repository path
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Initialize a new git repository
    pub fn init(&self) -> Result<()> {
        self.run_git_command(&["init"], "Failed to initialize git repository")?;
        Ok(())
    }

    /// Configure git user for tests
    pub fn config_user(&self) -> Result<()> {
        self.run_git_command(
            &["config", "user.email", "test@agpm.example"],
            "Failed to configure git user email",
        )?;

        self.run_git_command(
            &["config", "user.name", "Test User"],
            "Failed to configure git user name",
        )?;
        Ok(())
    }

    /// Add all files to staging
    pub fn add_all(&self) -> Result<()> {
        self.run_git_command(&["add", "."], "Failed to add files to git")?;
        Ok(())
    }

    /// Create a commit with the given message
    pub fn commit(&self, message: &str) -> Result<()> {
        self.run_git_command(&["commit", "-m", message], "Failed to create git commit")?;
        Ok(())
    }

    /// Create a tag
    pub fn tag(&self, tag_name: &str) -> Result<()> {
        self.run_git_command(&["tag", tag_name], &format!("Failed to create tag: {}", tag_name))?;
        Ok(())
    }

    /// Create and checkout a branch
    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["checkout", "-b", branch_name],
            &format!("Failed to create branch: {}", branch_name),
        )?;
        Ok(())
    }

    /// Checkout an existing branch
    pub fn checkout(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["checkout", branch_name],
            &format!("Failed to checkout branch: {}", branch_name),
        )?;
        Ok(())
    }

    /// Ensure we're on a specific branch, creating it if it doesn't exist
    /// This is useful when the default branch name is unknown (master vs main)
    pub fn ensure_branch(&self, branch_name: &str) -> Result<()> {
        // Try to checkout the branch first
        if self.checkout(branch_name).is_ok() {
            return Ok(());
        }

        // Branch doesn't exist, create it from current HEAD
        self.create_branch(branch_name)?;
        Ok(())
    }

    /// Set the HEAD to point to a branch (making it the default branch)
    pub fn set_head(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["symbolic-ref", "HEAD", &format!("refs/heads/{}", branch_name)],
            &format!("Failed to set HEAD to branch: {}", branch_name),
        )?;
        Ok(())
    }

    /// Get the current commit hash
    pub fn get_commit_hash(&self) -> Result<String> {
        let output = self.run_git_command(&["rev-parse", "HEAD"], "Failed to get commit hash")?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the HEAD SHA (alias for get_commit_hash for compatibility)
    pub fn get_head_sha(&self) -> Result<String> {
        self.get_commit_hash()
    }

    /// Clone current repository to a bare repository
    pub fn clone_to_bare(&self, target_path: &Path) -> Result<()> {
        let output = Command::new("git")
            .args([
                "clone",
                "--bare",
                self.repo_path.to_str().unwrap(),
                target_path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to create bare repository")?;
        if !output.status.success() {
            bail!("Failed to create bare repository: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(())
    }

    /// Return the repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Get porcelain status output
    pub fn status_porcelain(&self) -> Result<String> {
        let output =
            self.run_git_command(&["status", "--porcelain"], "Failed to get git status")?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if path is ignored by git
    pub fn check_ignore(&self, path: &str) -> Result<bool> {
        let output = Command::new("git")
            .args(["check-ignore", path])
            .current_dir(&self.repo_path)
            .output()
            .with_context(|| format!("Failed to run git check-ignore for {}", path))?;

        Ok(output.status.success())
    }
}

/// Test project builder for creating test environments
pub struct TestProject {
    _temp_dir: TempDir, // Keep alive for RAII cleanup
    project_dir: PathBuf,
    cache_dir: PathBuf,
    sources_dir: PathBuf,
}

impl TestProject {
    /// Create a new test project with default structure
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().join("project");
        let cache_dir = temp_dir.path().join(".agpm").join("cache");
        let sources_dir = temp_dir.path().join("sources");

        fs::create_dir_all(&project_dir).await?;
        fs::create_dir_all(&cache_dir).await?;
        fs::create_dir_all(&sources_dir).await?;

        Ok(Self {
            _temp_dir: temp_dir,
            project_dir,
            cache_dir,
            sources_dir,
        })
    }

    /// Get the project directory path
    pub fn project_path(&self) -> &Path {
        &self.project_dir
    }

    /// Get the cache directory path
    pub fn cache_path(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the sources directory path
    pub fn sources_path(&self) -> &Path {
        &self.sources_dir
    }

    /// Write a manifest file to the project directory
    pub async fn write_manifest(&self, content: &str) -> Result<()> {
        let manifest_path = self.project_dir.join("agpm.toml");
        fs::write(&manifest_path, content)
            .await
            .with_context(|| format!("Failed to write manifest to {:?}", manifest_path))?;
        Ok(())
    }

    /// Write agpm.private.toml manifest to project directory
    pub async fn write_private_manifest(&self, content: &str) -> Result<()> {
        let manifest_path = self.project_dir.join("agpm.private.toml");
        fs::write(&manifest_path, content)
            .await
            .with_context(|| format!("Failed to write private manifest to {:?}", manifest_path))?;
        Ok(())
    }

    /// Write a lockfile to the project directory
    pub async fn write_lockfile(&self, content: &str) -> Result<()> {
        let lockfile_path = self.project_dir.join("agpm.lock");
        fs::write(&lockfile_path, content)
            .await
            .with_context(|| format!("Failed to write lockfile to {:?}", lockfile_path))?;
        Ok(())
    }

    /// Read the lockfile from the project directory
    pub async fn read_lockfile(&self) -> Result<String> {
        let lockfile_path = self.project_dir.join("agpm.lock");
        fs::read_to_string(&lockfile_path)
            .await
            .with_context(|| format!("Failed to read lockfile from {:?}", lockfile_path))
    }

    /// Load and parse the lockfile as a LockFile struct
    pub fn load_lockfile(&self) -> Result<LockFile> {
        let lockfile_path = self.project_dir.join("agpm.lock");
        LockFile::load(&lockfile_path)
    }

    /// Create a local resource file
    pub async fn create_local_resource(&self, path: &str, content: &str) -> Result<()> {
        let resource_path = self.project_dir.join(path);
        if let Some(parent) = resource_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&resource_path, content).await?;
        Ok(())
    }

    /// Initialize a git repository inside the project directory
    pub fn init_git_repo(&self) -> Result<TestGit> {
        let git = TestGit::new(self.project_dir.clone());
        git.init()?;
        git.config_user()?;
        Ok(git)
    }

    /// Create a source repository with the given name
    pub async fn create_source_repo(&self, name: &str) -> Result<TestSourceRepo> {
        let source_dir = self.sources_dir.join(name);
        fs::create_dir_all(&source_dir).await?;

        let git = TestGit::new(&source_dir);
        git.init()?;
        git.config_user()?;

        Ok(TestSourceRepo {
            path: source_dir,
            git,
        })
    }

    /// Create a standard test repository with v1.0.0 tag
    ///
    /// This is a convenience method that creates a complete test repository
    /// with standard resources (agent, snippet, command) already tagged at v1.0.0.
    /// Returns both the repository and its bare file:// URL.
    ///
    /// This eliminates the most common test setup pattern (used 72+ times).
    ///
    /// # Arguments
    /// * `name` - The repository name
    ///
    /// # Returns
    /// A tuple of (TestSourceRepo, String) where the String is the bare file:// URL
    ///
    /// # Example
    /// ```rust
    /// let (repo, url) = project.create_standard_v1_repo("official").await?;
    /// // Repository is ready with v1.0.0 tag containing standard resources
    /// ```
    pub async fn create_standard_v1_repo(&self, name: &str) -> Result<(TestSourceRepo, String)> {
        let repo = self.create_source_repo(name).await?;
        repo.create_standard_resources().await?;
        repo.commit_all("Initial v1.0.0")?;
        repo.tag_version("v1.0.0")?;
        let url = repo.bare_file_url(self.sources_path())?;
        Ok((repo, url))
    }

    /// Run a AGPM command in the project directory
    pub fn run_agpm(&self, args: &[&str]) -> Result<CommandOutput> {
        self.run_agpm_with_env(args, &[])
    }

    /// Run a AGPM command with custom environment variables
    pub fn run_agpm_with_env(
        &self,
        args: &[&str],
        env_vars: &[(&str, &str)],
    ) -> Result<CommandOutput> {
        let agpm_binary = env!("CARGO_BIN_EXE_agpm");
        let mut cmd = Command::new(agpm_binary);

        cmd.args(args)
            .current_dir(&self.project_dir)
            .env("AGPM_CACHE_DIR", &self.cache_dir)
            .env("AGPM_TEST_MODE", "true")
            .env("NO_COLOR", "1");

        // Add custom environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().context("Failed to run agpm command")?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            code: output.status.code(),
        })
    }
}

/// Test source repository helper
pub struct TestSourceRepo {
    pub path: PathBuf,
    pub git: TestGit,
}

impl TestSourceRepo {
    /// Add a resource file to the repository
    pub async fn add_resource(&self, resource_type: &str, name: &str, content: &str) -> Result<()> {
        let resource_dir = self.path.join(resource_type);
        fs::create_dir_all(&resource_dir).await?;

        let file_path = resource_dir.join(format!("{}.md", name));

        // Create parent directories if the name contains slashes
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&file_path, content).await?;
        Ok(())
    }

    /// Add a skill directory with SKILL.md file
    pub async fn create_skill(&self, name: &str, content: &str) -> Result<()> {
        let skill_dir = self.path.join("skills").join(name);
        fs::create_dir_all(&skill_dir).await?;

        let skill_md_path = skill_dir.join("SKILL.md");
        fs::write(&skill_md_path, content).await?;
        Ok(())
    }

    /// Create a file at an arbitrary path in the repo
    pub async fn create_file(&self, path: &str, content: &str) -> Result<()> {
        let file_path = self.path.join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&file_path, content).await?;
        Ok(())
    }

    /// Create standard test resources
    pub async fn create_standard_resources(&self) -> Result<()> {
        self.add_resource("agents", "test-agent", "# Test Agent\n\nA test agent").await?;
        self.add_resource("snippets", "test-snippet", "# Test Snippet\n\nA test snippet").await?;
        self.add_resource("commands", "test-command", "# Test Command\n\nA test command").await?;
        Ok(())
    }

    /// Add multiple sequential resources with auto-generated content
    ///
    /// Creates resources named `{prefix}-{i:02}` (e.g., "agent-00", "agent-01")
    /// with generic test content. Useful for stress tests and parallelism tests.
    ///
    /// # Arguments
    /// * `resource_type` - The resource directory (e.g., "agents", "snippets")
    /// * `prefix` - Name prefix for resources (e.g., "agent", "snippet")
    /// * `count` - Number of resources to create
    ///
    /// # Example
    /// ```rust
    /// repo.add_sequential_resources("agents", "test-agent", 10).await?;
    /// // Creates: agents/test-agent-00.md through agents/test-agent-09.md
    /// ```
    pub async fn add_sequential_resources(
        &self,
        resource_type: &str,
        prefix: &str,
        count: usize,
    ) -> Result<()> {
        for i in 0..count {
            let name = format!("{}-{:02}", prefix, i);
            let title = prefix
                .split('-')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let content = format!("# {} {:02}\n\nTest {} {}", title, i, resource_type, i);
            self.add_resource(resource_type, &name, &content).await?;
        }
        Ok(())
    }

    /// Commit all changes with a message
    pub fn commit_all(&self, message: &str) -> Result<()> {
        self.git.add_all()?;
        self.git.commit(message)?;
        Ok(())
    }

    /// Create a version tag
    pub fn tag_version(&self, version: &str) -> Result<()> {
        self.git.tag(version)?;
        Ok(())
    }

    /// Get the file:// URL for this repository
    pub fn file_url(&self) -> String {
        format!("file://{}", normalize_path_for_storage(&self.path))
    }

    /// Clone this repository to a bare repository for reliable serving
    /// Returns the path to the new bare repository
    pub fn to_bare_repo(&self, target_path: &Path) -> Result<PathBuf> {
        let output = Command::new("git")
            .args(["clone", "--bare", self.path.to_str().unwrap(), target_path.to_str().unwrap()])
            .output()
            .context("Failed to create bare repository")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to create bare repository: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Verify the bare repository is ready by listing tags
        // This ensures git has finished writing all references
        let verify_output = Command::new("git")
            .args(["tag", "-l"])
            .current_dir(target_path)
            .output()
            .context("Failed to verify bare repository")?;

        if !verify_output.status.success() {
            return Err(anyhow::anyhow!(
                "Bare repository verification failed: {}",
                String::from_utf8_lossy(&verify_output.stderr)
            ));
        }

        Ok(target_path.to_path_buf())
    }

    /// Get a file:// URL for a bare clone of this repository
    /// Creates the bare repo in the parent's sources directory
    ///
    /// # Implementation Note
    /// Automatically ensures the repository is on the 'main' branch before creating
    /// the bare clone. This prevents "rev-parse: HEAD" errors in CI environments
    /// where bare repositories need a valid default branch reference.
    pub fn bare_file_url(&self, sources_dir: &Path) -> Result<String> {
        // Ensure we're on a proper branch before creating bare clone
        // This is critical for bare repositories to have a valid HEAD reference
        self.git.ensure_branch("main")?;

        let bare_name =
            format!("{}.git", self.path.file_name().and_then(|n| n.to_str()).unwrap_or("repo"));
        let bare_path = sources_dir.join(bare_name);
        self.to_bare_repo(&bare_path)?;
        Ok(format!("file://{}", normalize_path_for_storage(&bare_path)))
    }
}

/// Command output helper
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub code: Option<i32>,
}

impl CommandOutput {
    /// Assert the command succeeded
    pub fn assert_success(&self) -> &Self {
        assert!(self.success, "Command failed with code {:?}\nStderr: {}", self.code, self.stderr);
        self
    }

    /// Assert stdout contains the given text
    pub fn assert_stdout_contains(&self, text: &str) -> &Self {
        assert!(
            self.stdout.contains(text),
            "Expected stdout to contain '{}'\nActual stdout: {}",
            text,
            self.stdout
        );
        self
    }
}

/// File assertion helpers
pub struct FileAssert;

impl FileAssert {
    /// Assert a file exists
    pub async fn exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        let exists = fs::metadata(path).await.is_ok();
        assert!(exists, "Expected file to exist: {}", path.display());
    }

    /// Assert a file does not exist
    pub async fn not_exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        let exists = fs::metadata(path).await.is_ok();
        assert!(!exists, "Expected file to not exist: {}", path.display());
    }

    /// Assert a file contains specific content
    pub async fn contains(path: impl AsRef<Path>, expected: &str) {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .unwrap_or_else(|e| panic!("Failed to read file {}: {}", path.display(), e));
        assert!(
            content.contains(expected),
            "Expected file {} to contain '{}'\nActual content: {}",
            path.display(),
            expected,
            content
        );
    }

    /// Assert a file has exact content
    pub async fn equals(path: impl AsRef<Path>, expected: &str) {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .unwrap_or_else(|e| panic!("Failed to read file {}: {}", path.display(), e));
        assert_eq!(content, expected, "File {} content mismatch", path.display());
    }
}

/// Directory assertion helpers
pub struct DirAssert;

impl DirAssert {
    /// Assert a directory exists
    pub async fn exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        let metadata = fs::metadata(path).await;
        let is_dir = metadata.map(|m| m.is_dir()).unwrap_or(false);
        assert!(is_dir, "Expected directory to exist: {}", path.display());
    }

    /// Assert a directory contains a file
    pub async fn contains_file(dir: impl AsRef<Path>, file_name: &str) {
        let path = dir.as_ref().join(file_name);
        let exists = fs::metadata(&path).await.is_ok();
        assert!(
            exists,
            "Expected directory {} to contain file '{}'",
            dir.as_ref().display(),
            file_name
        );
    }

    /// Assert a directory is empty
    pub async fn is_empty(path: impl AsRef<Path>) {
        let path = path.as_ref();
        let mut read_dir = fs::read_dir(path)
            .await
            .unwrap_or_else(|e| panic!("Failed to read directory {}: {}", path.display(), e));

        let mut count = 0;
        while read_dir.next_entry().await.unwrap().is_some() {
            count += 1;
        }

        assert_eq!(
            count,
            0,
            "Expected directory {} to be empty, but it contains {} entries",
            path.display(),
            count
        );
    }
}
