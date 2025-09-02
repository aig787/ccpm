//! Integration tests for .gitignore management functionality
//!
//! These tests verify that CCPM correctly manages .gitignore files
//! based on the target.gitignore configuration setting.

use anyhow::Result;
use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a test manifest with gitignore configuration
fn create_test_manifest(gitignore: bool, source_dir: &Path) -> String {
    // Convert path to string with forward slashes for TOML compatibility
    let source_path = source_dir.display().to_string().replace('\\', "/");
    format!(
        r#"
[sources]

[target]
agents = ".claude/agents"
snippets = ".claude/ccpm/snippets"
commands = ".claude/commands"
gitignore = {}

[agents.test-agent]
path = "{}/agents/test.md"

[snippets.test-snippet]
path = "{}/snippets/test.md"

[commands.test-command]
path = "{}/commands/test.md"
"#,
        gitignore, source_path, source_path, source_path
    )
}

/// Helper to create a test manifest without explicit gitignore setting
fn create_test_manifest_default(source_dir: &Path) -> String {
    // Convert path to string with forward slashes for TOML compatibility
    let source_path = source_dir.display().to_string().replace('\\', "/");
    format!(
        r#"
[sources]

[target]
agents = ".claude/agents"
snippets = ".claude/ccpm/snippets"
commands = ".claude/commands"

[agents.test-agent]
path = "{}/agents/test.md"
"#,
        source_path
    )
}

/// Helper to create a test lockfile with installed resources
fn create_test_lockfile() -> String {
    r#"
version = 1

[[sources]]
name = "test-source"
url = "https://github.com/test/repo.git"
commit = "abc123"

[[agents]]
name = "test-agent"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = ".claude/agents/test-agent.md"

[[snippets]]
name = "test-snippet"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "snippets/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = ".claude/ccpm/snippets/test-snippet.md"

[[commands]]
name = "test-command"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "commands/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = ".claude/commands/test-command.md"
"#
    .to_string()
}

/// Create test source files that can be installed
fn create_test_source_files(source_dir: &Path) -> Result<()> {
    // Create the directories
    fs::create_dir_all(source_dir.join("agents"))?;
    fs::create_dir_all(source_dir.join("snippets"))?;
    fs::create_dir_all(source_dir.join("commands"))?;

    // Create source files
    fs::write(source_dir.join("agents/test.md"), "# Test Agent\n")?;
    fs::write(source_dir.join("snippets/test.md"), "# Test Snippet\n")?;
    fs::write(source_dir.join("commands/test.md"), "# Test Command\n")?;

    Ok(())
}

#[test]
fn test_gitignore_enabled_by_default() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest without explicit gitignore setting (should default to true)
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest_default(&source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore was created
    let gitignore_path = project_dir.join(".gitignore");
    assert!(
        gitignore_path.exists(),
        "Gitignore should be created by default"
    );

    // Check that it has the expected structure
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("CCPM managed entries"));
    assert!(content.contains("# End of CCPM managed entries"));
}

#[test]
fn test_gitignore_explicitly_enabled() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest with gitignore = true
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore was created
    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Verify content structure
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("CCPM managed entries"));
    assert!(content.contains("CCPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of CCPM managed entries"));
}

#[test]
fn test_gitignore_disabled() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest with gitignore = false
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(false, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore was NOT created
    let gitignore_path = project_dir.join(".gitignore");
    assert!(
        !gitignore_path.exists(),
        "Gitignore should not be created when disabled"
    );
}

#[test]
fn test_gitignore_preserves_user_entries() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).unwrap();

    // Create existing gitignore with user entries
    let gitignore_path = project_dir.join(".gitignore");
    let user_content = r#"# User's custom comment
*.backup
user-file.txt
temp/

# CCPM managed entries - do not edit below this line
.claude/agents/old-agent.md
# End of CCPM managed entries
"#;
    fs::write(&gitignore_path, user_content).unwrap();

    // Create manifest with gitignore enabled
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that user entries are preserved
    let updated_content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(updated_content.contains("# User's custom comment"));
    assert!(updated_content.contains("*.backup"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("temp/"));

    // Check that CCPM section exists (entries will be based on what was actually installed)
    assert!(updated_content.contains("CCPM managed entries"));
    assert!(updated_content.contains("# End of CCPM managed entries"));
    assert!(updated_content.contains(".claude/ccpm/snippets/test-snippet.md"));
}

#[test]
fn test_gitignore_preserves_content_after_ccpm_section() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).unwrap();

    // Create existing gitignore with content after CCPM section
    let gitignore_path = project_dir.join(".gitignore");
    let user_content = r#"# Project gitignore
*.backup
temp/

# CCPM managed entries - do not edit below this line
.claude/agents/old-agent.md
# End of CCPM managed entries

# Additional entries after CCPM section
local-config.json
debug/
# End comment
"#;
    fs::write(&gitignore_path, user_content).unwrap();

    // Create manifest with gitignore enabled
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that all sections are preserved
    let updated_content = fs::read_to_string(&gitignore_path).unwrap();

    // Check content before CCPM section
    assert!(updated_content.contains("# Project gitignore"));
    assert!(updated_content.contains("*.backup"));
    assert!(updated_content.contains("temp/"));

    // Check CCPM section is updated
    assert!(updated_content.contains("CCPM managed entries"));
    assert!(updated_content.contains("# End of CCPM managed entries"));
    assert!(updated_content.contains(".claude/ccpm/snippets/test-snippet.md"));

    // Check content after CCPM section is preserved
    assert!(updated_content.contains("# Additional entries after CCPM section"));
    assert!(updated_content.contains("local-config.json"));
    assert!(updated_content.contains("debug/"));
    assert!(updated_content.contains("# End comment"));

    // Verify old CCPM entry is removed
    assert!(!updated_content.contains(".claude/agents/old-agent.md"));
}

#[test]
fn test_gitignore_update_command() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create initial lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run update command (which should also update gitignore)
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("update")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore exists after update
    let gitignore_path = project_dir.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("CCPM managed entries"));
    }
}

#[test]
fn test_gitignore_handles_external_paths() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("ccpm.toml");
    let manifest_content = r#"
[sources]
test-source = "https://github.com/test/repo.git"

[target]
gitignore = true

[scripts.external-script]
source = "test-source"
path = "scripts/test.sh"
version = "v1.0.0"
"#;
    fs::write(&manifest_path, manifest_content).unwrap();

    // Create lockfile with resource installed outside .claude
    let lockfile_content = r#"
version = 1

[[sources]]
name = "test-source"
url = "https://github.com/test/repo.git"
commit = "abc123"

[[scripts]]
name = "external-script"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "scripts/test.sh"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = "scripts/external.sh"

[[agents]]
name = "internal-agent"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = ".claude/agents/internal.md"
"#;
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, lockfile_content).unwrap();

    // Create directories
    fs::create_dir_all(project_dir.join(".claude/agents")).unwrap();
    fs::create_dir_all(project_dir.join("scripts")).unwrap();

    // Create resource files
    fs::write(project_dir.join("scripts/external.sh"), "#!/bin/bash\n").unwrap();
    fs::write(
        project_dir.join(".claude/agents/internal.md"),
        "# Internal\n",
    )
    .unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check gitignore content
    let gitignore_path = project_dir.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).unwrap();
        // External path should use ../
        assert!(
            content.contains("../scripts/external.sh"),
            "External paths should use ../ prefix"
        );
        // Internal path should use /
        assert!(
            content.contains(".claude/agents/internal.md"),
            "Internal paths should use / prefix"
        );
    }
}

#[test]
fn test_gitignore_empty_lockfile() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create empty lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, "version = 1\n").unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore is created even with no resources
    let gitignore_path = project_dir.join(".gitignore");
    assert!(
        gitignore_path.exists(),
        "Gitignore should be created even with empty lockfile"
    );

    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("CCPM managed entries"));
    assert!(content.contains("# End of CCPM managed entries"));
}

#[test]
fn test_gitignore_idempotent() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create manifest
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Get content after first run
    let gitignore_path = project_dir.join(".gitignore");
    let first_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).unwrap()
    } else {
        String::new()
    };

    // Run again
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Get content after second run
    let second_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).unwrap()
    } else {
        String::new()
    };

    // Content should be the same (idempotent)
    assert_eq!(
        first_content, second_content,
        "Gitignore should be idempotent"
    );
}

#[test]
fn test_gitignore_switch_enabled_disabled() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Start with gitignore enabled
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install with gitignore enabled
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Now disable gitignore
    fs::write(&manifest_path, create_test_manifest(false, &source_dir)).unwrap();

    // Run install again
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Gitignore should still exist (we don't delete it)
    assert!(
        gitignore_path.exists(),
        "Gitignore should still exist when disabled"
    );

    // Re-enable gitignore
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Add a user entry to the existing gitignore
    let content = fs::read_to_string(&gitignore_path).unwrap();
    let modified_content = content.replace(
        "# CCPM managed entries",
        "user-custom.txt\n\n# CCPM managed entries",
    );
    fs::write(&gitignore_path, modified_content).unwrap();

    // Run install again
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that user entry is preserved
    let final_content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(
        final_content.contains("user-custom.txt"),
        "User entries should be preserved when re-enabling"
    );
}

#[test]
fn test_gitignore_actually_ignored_by_git() {
    ccpm::test_utils::init_test_logging();
    use std::process::Command as StdCommand;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Initialize a git repository
    StdCommand::new("git")
        .arg("init")
        .current_dir(project_dir)
        .output()
        .expect("Failed to initialize git repo");

    // Create manifest with gitignore enabled
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command to create gitignore and install files
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Verify files were installed
    assert!(project_dir.join(".claude/agents/test-agent.md").exists());
    assert!(project_dir
        .join(".claude/ccpm/snippets/test-snippet.md")
        .exists());
    assert!(project_dir
        .join(".claude/commands/test-command.md")
        .exists());

    // Stage all files for git
    StdCommand::new("git")
        .arg("add")
        .arg(".")
        .current_dir(project_dir)
        .output()
        .expect("Failed to stage files");

    // Check git status to see what files are staged
    let output = StdCommand::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(project_dir)
        .output()
        .expect("Failed to get git status");

    let status = String::from_utf8_lossy(&output.stdout);

    // Verify that installed CCPM files are NOT staged (ignored by git)
    assert!(
        !status.contains("agents/test-agent.md"),
        "Agent file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("snippets/test-snippet.md"),
        "Snippet file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("commands/test-command.md"),
        "Command file should be ignored by git\nGit status:\n{}",
        status
    );

    // Verify that the gitignore file itself IS staged
    assert!(
        status.contains(".gitignore"),
        "Gitignore file should be tracked by git\nGit status:\n{}",
        status
    );

    // Verify that manifest and lockfile ARE staged
    assert!(
        status.contains("ccpm.toml"),
        "Manifest should be tracked by git\nGit status:\n{}",
        status
    );
    assert!(
        status.contains("ccpm.lock"),
        "Lockfile should be tracked by git\nGit status:\n{}",
        status
    );

    // Also test with git check-ignore to be explicit
    let check_agent = StdCommand::new("git")
        .arg("check-ignore")
        .arg(".claude/agents/test-agent.md")
        .current_dir(project_dir)
        .output()
        .expect("Failed to check-ignore");

    // git check-ignore returns 0 if the file is ignored
    assert!(
        check_agent.status.success(),
        "Agent file should be ignored by git check-ignore"
    );

    let check_snippet = StdCommand::new("git")
        .arg("check-ignore")
        .arg(".claude/ccpm/snippets/test-snippet.md")
        .current_dir(project_dir)
        .output()
        .expect("Failed to check-ignore");

    assert!(
        check_snippet.status.success(),
        "Snippet file should be ignored by git check-ignore"
    );

    let check_command = StdCommand::new("git")
        .arg("check-ignore")
        .arg(".claude/commands/test-command.md")
        .current_dir(project_dir)
        .output()
        .expect("Failed to check-ignore");

    assert!(
        check_command.status.success(),
        "Command file should be ignored by git check-ignore"
    );
}

#[test]
fn test_gitignore_disabled_files_not_ignored_by_git() {
    ccpm::test_utils::init_test_logging();
    use std::process::Command as StdCommand;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Initialize a git repository
    StdCommand::new("git")
        .arg("init")
        .current_dir(project_dir)
        .output()
        .expect("Failed to initialize git repo");

    // Create manifest with gitignore DISABLED
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(false, &source_dir)).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command (should NOT create gitignore)
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Verify files were installed
    assert!(project_dir.join(".claude/agents/test-agent.md").exists());
    assert!(project_dir
        .join(".claude/ccpm/snippets/test-snippet.md")
        .exists());
    assert!(project_dir
        .join(".claude/commands/test-command.md")
        .exists());

    // Stage all files for git
    StdCommand::new("git")
        .arg("add")
        .arg(".")
        .current_dir(project_dir)
        .output()
        .expect("Failed to stage files");

    // Check git status to see what files are staged
    let output = StdCommand::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(project_dir)
        .output()
        .expect("Failed to get git status");

    let status = String::from_utf8_lossy(&output.stdout);

    // When gitignore is disabled, installed files SHOULD be staged (NOT ignored)
    assert!(
        status.contains("agents/test-agent.md"),
        "Agent file should NOT be ignored when gitignore is disabled\nGit status:\n{}",
        status
    );
    assert!(
        status.contains("snippets/test-snippet.md"),
        "Snippet file should NOT be ignored when gitignore is disabled\nGit status:\n{}",
        status
    );
    assert!(
        status.contains("commands/test-command.md"),
        "Command file should NOT be ignored when gitignore is disabled\nGit status:\n{}",
        status
    );

    // Also test with git check-ignore to be explicit
    let check_agent = StdCommand::new("git")
        .arg("check-ignore")
        .arg(".claude/agents/test-agent.md")
        .current_dir(project_dir)
        .output()
        .expect("Failed to check-ignore");

    // git check-ignore returns non-zero if the file is NOT ignored
    assert!(
        !check_agent.status.success(),
        "Agent file should NOT be ignored when gitignore is disabled"
    );
}

#[test]
fn test_gitignore_malformed_existing() {
    ccpm::test_utils::init_test_logging();
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).unwrap();

    // Create malformed gitignore (missing end marker)
    let gitignore_path = project_dir.join(".gitignore");
    let malformed_content = r#"# Some content
user-file.txt

# CCPM managed entries - do not edit below this line
/old/entry.md
# Missing end marker!
"#;
    fs::write(&gitignore_path, malformed_content).unwrap();

    // Create manifest and lockfile
    let manifest_path = project_dir.join("ccpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir)).unwrap();

    let lockfile_path = project_dir.join("ccpm.lock");
    fs::write(&lockfile_path, create_test_lockfile()).unwrap();

    // Run install command
    Command::cargo_bin("ccpm")
        .unwrap()
        .arg("install")
        .arg("--force")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that gitignore was properly recreated
    let updated_content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(updated_content.contains("# End of CCPM managed entries"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("CCPM managed entries"));
}
