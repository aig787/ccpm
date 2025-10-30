//! Integration tests for incremental dependency addition with `agpm add dep`.
//!
//! These tests verify that transitive dependency relationships are properly maintained
//! when dependencies are added incrementally to a project manifest.

use agpm_cli::utils::normalize_path_for_storage;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Helper to create a test project with manifest and resource files
async fn setup_test_project() -> Result<TestProject> {
    let project = TestProject::new().await?;

    // Create a local source directory for resources
    let resources_dir = project.sources_path().join("local-resources");
    fs::create_dir_all(&resources_dir).await?;
    fs::create_dir_all(resources_dir.join("commands")).await?;
    fs::create_dir_all(resources_dir.join("agents")).await?;
    fs::create_dir_all(resources_dir.join("snippets")).await?;

    // Create manifest with local source
    let resources_url = normalize_path_for_storage(&resources_dir);
    let manifest_content = ManifestBuilder::new()
        .add_source("local", &resources_url)
        .with_target_config(|t| {
            t.agents(".claude/agents")
                .snippets(".agpm/snippets")
                .commands(".claude/commands")
                .mcp_servers(".mcp-servers")
                .scripts(".claude/scripts")
                .hooks(".hooks")
                .gitignore(true)
        })
        .build();
    project.write_manifest(&manifest_content).await?;

    // Create command file with transitive dependencies in local source
    let command_content = r#"---
title: Test Command
description: A test command with dependencies
dependencies:
  agents:
    - path: ../agents/test-agent.md
  snippets:
    - path: ../snippets/test-snippet.md
---

# Test Command

This command depends on an agent and a snippet.
"#;
    fs::write(resources_dir.join("commands/test-command.md"), command_content).await?;

    // Create agent file with its own transitive dependency in local source
    let agent_content = r#"---
title: Test Agent
description: A test agent with dependencies
dependencies:
  snippets:
    - path: ../snippets/helper-snippet.md
---

# Test Agent

This agent depends on a helper snippet.
"#;
    fs::write(resources_dir.join("agents/test-agent.md"), agent_content).await?;

    // Create snippet files in local source
    fs::write(resources_dir.join("snippets/test-snippet.md"), "# Test Snippet\n\nA test snippet.")
        .await?;
    fs::write(
        resources_dir.join("snippets/helper-snippet.md"),
        "# Helper Snippet\n\nA helper snippet.",
    )
    .await?;

    Ok(project)
}

/// Helper to read and parse lockfile dependencies for a resource
async fn get_lockfile_dependencies(lockfile_path: &PathBuf, resource_name: &str) -> Vec<String> {
    let lockfile_content = fs::read_to_string(lockfile_path).await.unwrap();
    let lockfile: toml::Value = toml::from_str(&lockfile_content).unwrap();

    // Search in all resource type arrays
    for resource_type in
        &["agents", "snippets", "commands", "scripts", "hooks", "mcp-servers", "skills"]
    {
        if let Some(resources) = lockfile.get(resource_type).and_then(|v| v.as_array()) {
            for resource in resources {
                // Match by name OR manifest_alias for backward compatibility
                let name_matches = resource
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| n == resource_name)
                    .unwrap_or(false);
                let alias_matches = resource
                    .get("manifest_alias")
                    .and_then(|v| v.as_str())
                    .map(|a| a == resource_name)
                    .unwrap_or(false);

                if name_matches || alias_matches {
                    if let Some(deps) = resource.get("dependencies").and_then(|v| v.as_array()) {
                        return deps.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                    }
                    return Vec::new();
                }
            }
        }
    }

    Vec::new()
}

#[tokio::test]
async fn test_incremental_add_preserves_transitive_dependencies() {
    let project = setup_test_project().await.unwrap();
    let lockfile_path = project.project_path().join("agpm.lock");

    // Step 1: Add command dependency (should discover transitive deps)
    let output = project
        .run_agpm(&[
            "add",
            "dep",
            "command",
            "local:commands/test-command.md",
            "--name",
            "test-command",
        ])
        .unwrap();

    output.assert_success().assert_stdout_contains("Added command 'test-command'");

    // Verify lockfile has transitive dependencies
    assert!(lockfile_path.exists(), "Lockfile should be created");

    let command_deps = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    assert!(
        !command_deps.is_empty(),
        "Command should have transitive dependencies after first add. Found: {:?}",
        command_deps
    );
    assert!(
        command_deps.iter().any(|d| d.contains("test-agent")),
        "Command should depend on test-agent. Found: {:?}",
        command_deps
    );

    // Step 2: Add agent dependency explicitly (making it a base dependency)
    let output2 = project
        .run_agpm(&["add", "dep", "agent", "local:agents/test-agent.md", "--name", "test-agent"])
        .unwrap();

    output2.assert_success().assert_stdout_contains("Added agent 'test-agent'");

    // CRITICAL: Verify command still has its dependencies after agent becomes a base dep
    let command_deps_after = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    assert!(
        !command_deps_after.is_empty(),
        "Command dependencies should be preserved after adding agent as base dep! \
         This was the bug we fixed. Found: {:?}",
        command_deps_after
    );
    assert!(
        command_deps_after.iter().any(|d| d.contains("test-agent")),
        "Command -> Agent dependency should be maintained! Found: {:?}",
        command_deps_after
    );

    // Verify agent also has its dependencies
    let agent_deps = get_lockfile_dependencies(&lockfile_path, "test-agent").await;
    assert!(
        !agent_deps.is_empty(),
        "Agent should have its transitive dependencies. Found: {:?}",
        agent_deps
    );
    assert!(
        agent_deps.iter().any(|d| d.contains("helper-snippet")),
        "Agent should depend on helper-snippet. Found: {:?}",
        agent_deps
    );
}

#[tokio::test]
async fn test_incremental_add_chain_of_three_dependencies() {
    let project = setup_test_project().await.unwrap();
    let lockfile_path = project.project_path().join("agpm.lock");

    // Add command (discovers agent and snippet as transitive deps)
    project
        .run_agpm(&[
            "add",
            "dep",
            "command",
            "local:commands/test-command.md",
            "--name",
            "test-command",
        ])
        .unwrap()
        .assert_success();

    let command_deps_step1 = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    assert!(!command_deps_step1.is_empty(), "Command should have dependencies");

    // Add agent explicitly (was transitive, now base)
    project
        .run_agpm(&["add", "dep", "agent", "local:agents/test-agent.md", "--name", "test-agent"])
        .unwrap()
        .assert_success();

    let command_deps_step2 = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    let agent_deps_step2 = get_lockfile_dependencies(&lockfile_path, "test-agent").await;

    assert!(!command_deps_step2.is_empty(), "Command deps should persist after step 2");
    assert!(!agent_deps_step2.is_empty(), "Agent should have dependencies");

    // Add helper-snippet explicitly (was transitive of agent, now base)
    project
        .run_agpm(&[
            "add",
            "dep",
            "snippet",
            "local:snippets/helper-snippet.md",
            "--name",
            "helper-snippet",
        ])
        .unwrap()
        .assert_success();

    // Verify ALL dependency relationships are still intact
    let command_deps_final = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    let agent_deps_final = get_lockfile_dependencies(&lockfile_path, "test-agent").await;

    assert!(
        !command_deps_final.is_empty(),
        "Command should still have dependencies after all adds"
    );
    assert!(
        !agent_deps_final.is_empty(),
        "Agent should still have dependencies after helper-snippet becomes base dep"
    );

    // Verify the specific dependency relationships
    assert!(
        command_deps_final.iter().any(|d| d.contains("test-agent")),
        "Command -> Agent dependency should be maintained"
    );
    assert!(
        agent_deps_final.iter().any(|d| d.contains("helper-snippet")),
        "Agent -> Snippet dependency should be maintained"
    );
}

#[tokio::test]
async fn test_incremental_add_with_shared_dependency() {
    let project = setup_test_project().await.unwrap();

    // Get the resources directory path to create second command there
    let resources_dir = project.sources_path().join("local-resources");

    // Create a second command that shares the test-snippet dependency
    let command2_content = r#"---
title: Second Command
dependencies:
  snippets:
    - path: ../snippets/test-snippet.md
---

# Second Command

This command also uses test-snippet.
"#;
    fs::write(resources_dir.join("commands/second-command.md"), command2_content).await.unwrap();

    let lockfile_path = project.project_path().join("agpm.lock");

    // Add first command
    project
        .run_agpm(&[
            "add",
            "dep",
            "command",
            "local:commands/test-command.md",
            "--name",
            "test-command",
        ])
        .unwrap()
        .assert_success();

    // Add second command
    project
        .run_agpm(&[
            "add",
            "dep",
            "command",
            "local:commands/second-command.md",
            "--name",
            "second-command",
        ])
        .unwrap()
        .assert_success();

    // Both commands should have their dependencies
    let cmd1_deps = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    let cmd2_deps = get_lockfile_dependencies(&lockfile_path, "second-command").await;

    assert!(!cmd1_deps.is_empty(), "First command should have dependencies");
    assert!(!cmd2_deps.is_empty(), "Second command should have dependencies");

    // Now add the shared snippet explicitly
    project
        .run_agpm(&[
            "add",
            "dep",
            "snippet",
            "local:snippets/test-snippet.md",
            "--name",
            "test-snippet",
        ])
        .unwrap()
        .assert_success();

    // Both commands should STILL have their dependencies
    let cmd1_deps_after = get_lockfile_dependencies(&lockfile_path, "test-command").await;
    let cmd2_deps_after = get_lockfile_dependencies(&lockfile_path, "second-command").await;

    assert!(
        cmd1_deps_after.iter().any(|d| d.contains("test-snippet")),
        "First command should still depend on test-snippet"
    );
    assert!(
        cmd2_deps_after.iter().any(|d| d.contains("test-snippet")),
        "Second command should still depend on test-snippet"
    );
}
