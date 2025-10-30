//! Integration tests for the `install` field functionality.
//!
//! These tests verify that:
//! - Resources with `install=false` don't create files on disk
//! - Content is still available in template context when `install=false`
//! - Lockfile correctly tracks the `install` field
//! - Combined `install=false` + `content` embedding works end-to-end
//! - Files are cleaned up when `install` changes from `true` to `false`

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test that install=false prevents file creation while still tracking in lockfile
#[tokio::test]
async fn test_install_false_skips_file_write() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a snippet that will be marked as install=false in manifest
    test_repo
        .add_resource(
            "snippets",
            "best-practices",
            r#"---
title: Best Practices
---
# Best Practices

1. Write clear code
2. Test thoroughly
3. Document well
"#,
        )
        .await?;

    // Create a regular snippet for comparison
    test_repo
        .add_resource(
            "snippets",
            "regular-snippet",
            r#"---
title: Regular Snippet
---
# Regular Snippet

This snippet should be installed normally.
"#,
        )
        .await?;

    test_repo.commit_all("Add snippets")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with install=false for best-practices
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
best-practices = {{ source = "test-repo", path = "snippets/best-practices.md", version = "v1.0.0", tool = "agpm", install = false }}
regular-snippet = {{ source = "test-repo", path = "snippets/regular-snippet.md", version = "v1.0.0", tool = "agpm" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify regular snippet WAS installed
    let regular_path = project.project_path().join(".agpm/snippets/regular-snippet.md");
    assert!(
        fs::metadata(&regular_path).await.is_ok(),
        "Regular snippet should be installed at {:?}",
        regular_path
    );

    // Verify install=false snippet was NOT installed
    let best_practices_path = project.project_path().join(".agpm/snippets/best-practices.md");
    assert!(
        fs::metadata(&best_practices_path).await.is_err(),
        "install=false snippet should NOT be written to disk at {:?}",
        best_practices_path
    );

    // Verify lockfile contains both entries with correct install field
    let lockfile_content = project.read_lockfile().await?;

    assert!(
        lockfile_content.contains("best-practices"),
        "install=false snippet should be in lockfile"
    );
    assert!(lockfile_content.contains("regular-snippet"), "Regular snippet should be in lockfile");

    // Verify install=false is tracked in lockfile
    assert!(
        lockfile_content.contains("install = false"),
        "Lockfile should track install=false. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that content field is available in templates for all dependencies
#[tokio::test]
async fn test_content_field_in_templates() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a snippet that will be embedded
    test_repo
        .add_resource(
            "snippets",
            "coding-standards",
            r#"---
title: Coding Standards
---
# Coding Standards

- Follow PEP 8
- Write unit tests
- Use type hints
"#,
        )
        .await?;

    // Create an agent that embeds the snippet content
    test_repo
        .add_resource(
            "agents",
            "code-reviewer",
            r#"---
title: Code Reviewer
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/coding-standards.md
      version: v1.0.0
---
# Code Review Agent

## Standards I enforce:

{{ agpm.deps.snippets.coding_standards.content }}

## Review Process

I check your code against the standards above.
"#,
        )
        .await?;

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest - only the agent, snippet is transitive
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("code-reviewer", |d| {
            d.source("test-repo").path("agents/code-reviewer.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Read the installed agent file
    let agent_path = project.project_path().join(".claude/agents/code-reviewer.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Verify content was embedded (frontmatter stripped)
    assert!(content.contains("# Coding Standards"), "Snippet content should be embedded in agent");
    assert!(
        content.contains("Follow PEP 8"),
        "Snippet content should be embedded with actual standards"
    );
    assert!(content.contains("Write unit tests"), "All snippet content should be included");

    // Verify frontmatter was stripped from embedded content
    assert!(
        !content.contains("title: Coding Standards"),
        "Frontmatter should be stripped from embedded content. Content:\n{}",
        content
    );

    // Verify template syntax was replaced
    assert!(!content.contains("{{ agpm.deps"), "Template syntax should be replaced");

    // Verify the snippet was still installed as a separate file (default behavior)
    // Note: Snippet inherits claude-code tool from parent agent, so it's in .claude/snippets/
    let snippet_path = project.project_path().join(".claude/snippets/coding-standards.md");
    assert!(
        fs::metadata(&snippet_path).await.is_ok(),
        "Snippet should still be installed by default at {:?}",
        snippet_path
    );

    Ok(())
}

/// Test combined install=false + content embedding
#[tokio::test]
async fn test_install_false_with_content_embedding() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a snippet that will be embedded only (install=false set in dependency)
    test_repo
        .add_resource(
            "snippets",
            "guidelines",
            r#"---
title: Development Guidelines
---
# Development Guidelines

## Testing
- Write tests first
- Aim for 80% coverage

## Documentation
- Document all public APIs
- Include examples
"#,
        )
        .await?;

    // Create an agent that embeds the guidelines
    test_repo
        .add_resource(
            "agents",
            "dev-assistant",
            r#"---
title: Development Assistant
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/guidelines.md
      version: v1.0.0
      install: false
      name: dev_guidelines
---
# Development Assistant

I help you follow these guidelines:

{{ agpm.deps.snippets.dev_guidelines.content }}

Ask me anything about development best practices!
"#,
        )
        .await?;

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("dev-assistant", |d| {
            d.source("test-repo").path("agents/dev-assistant.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify snippet was NOT installed (install=false)
    let snippet_path = project.project_path().join(".agpm/snippets/guidelines.md");
    assert!(
        fs::metadata(&snippet_path).await.is_err(),
        "Snippet with install=false should NOT be written to disk at {:?}",
        snippet_path
    );

    // Verify agent WAS installed with embedded content
    let agent_path = project.project_path().join(".claude/agents/dev-assistant.md");
    assert!(
        fs::metadata(&agent_path).await.is_ok(),
        "Agent should be installed at {:?}",
        agent_path
    );

    // Read agent content
    let content = fs::read_to_string(&agent_path).await?;

    // Verify guidelines were embedded (without frontmatter)
    assert!(content.contains("# Development Guidelines"), "Guidelines content should be embedded");
    assert!(content.contains("Write tests first"), "Guidelines content should be complete");
    assert!(content.contains("Document all public APIs"), "All guidelines should be included");

    // Verify frontmatter was stripped from the EMBEDDED snippet content
    // Note: The agent file itself still has frontmatter with agpm: metadata
    assert!(
        !content.contains("title: Development Guidelines"),
        "Snippet frontmatter should be stripped from embedded content"
    );

    // Verify template syntax was replaced
    assert!(
        !content.contains("{{ agpm.deps"),
        "Template syntax should be replaced. Content:\n{}",
        content
    );

    // Verify lockfile tracks install=false for the snippet
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("guidelines"),
        "Snippet should be in lockfile even with install=false"
    );
    assert!(
        lockfile_content.contains("install = false"),
        "Lockfile should track install=false for snippet. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that content extraction works for transitive dependencies within the same repo.
///
/// This test verifies that when an agent has transitive dependencies (via `dependencies:`
/// in frontmatter) to other files in the same Git repository, their content is properly
/// extracted and made available in templates. This requires the TemplateContextBuilder
/// to have access to the worktree path and project directory for proper path resolution.
#[tokio::test]
async fn test_content_extraction_from_local_files() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create snippet files in the Git repo that will be used as transitive dependencies
    let local_snippet_content = r#"---
title: Local Helper
---
# Local Helper Functions

- `format_date()` - Format dates consistently
- `validate_input()` - Input validation
- `log_error()` - Error logging
"#;
    test_repo.add_resource("snippets", "local-helper", local_snippet_content).await?;

    // Create another local file that will be used as a dependency
    let utils_content = r#"---
title: Utilities
---
# Utility Functions

## String Utilities
- `trim()` - Remove whitespace
- `uppercase()` - Convert to uppercase
"#;
    test_repo.add_resource("snippets", "utils", utils_content).await?;

    // Create an agent that depends on the snippets and embeds their content
    test_repo
        .add_resource(
            "agents",
            "local-content-agent",
            r#"---
title: Agent with Local Content
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/local-helper.md
      name: local_helper
    - path: ../snippets/utils.md
      name: utils
      install: false
---
# Agent Using Local Resources

## Helper Functions Available:

{{ agpm.deps.snippets.local_helper.content }}

## Utilities (embedded only):

{{ agpm.deps.snippets.utils.content }}

Use these in your work!
"#,
        )
        .await?;

    test_repo.commit_all("Add resources with local dependencies")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest - only reference the agent, snippets are transitive
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
local-content-agent = {{ source = "test-repo", path = "agents/local-content-agent.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. Stdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    // Read the installed agent file
    let agent_path = project.project_path().join(".claude/agents/local-content-agent.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Verify transitive dependency snippet content was embedded (frontmatter stripped)
    assert!(
        content.contains("# Local Helper Functions"),
        "Helper snippet content should be embedded in agent"
    );
    assert!(
        content.contains("`format_date()`"),
        "Helper snippet content should include function details"
    );
    assert!(content.contains("`validate_input()`"), "All helper functions should be included");

    // Verify utils content was also embedded
    assert!(content.contains("# Utility Functions"), "Utils content should be embedded in agent");
    assert!(content.contains("`trim()`"), "Utils functions should be included");

    // Verify frontmatter was stripped from embedded content
    assert!(
        !content.contains("title: Local Helper"),
        "Frontmatter should be stripped from embedded local helper content. Content:\n{}",
        content
    );
    assert!(
        !content.contains("title: Utilities"),
        "Frontmatter should be stripped from embedded utils content"
    );

    // Verify template syntax was replaced
    assert!(
        !content.contains("{{ agpm.deps"),
        "Template syntax should be replaced. Content:\n{}",
        content
    );

    // Verify the transitive dependency snippet was installed (default behavior)
    let helper_path = project.project_path().join(".claude/snippets/local-helper.md");
    assert!(
        fs::metadata(&helper_path).await.is_ok(),
        "Transitive dependency helper should be installed as a file at {:?}",
        helper_path
    );

    // Verify utils was NOT installed (install=false)
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    assert!(
        fs::metadata(&utils_path).await.is_err(),
        "Utils with install=false should NOT be installed at {:?}",
        utils_path
    );

    // Verify lockfile tracks both transitive dependencies
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("local-helper") || lockfile_content.contains("local_helper"),
        "Lockfile should contain local-helper transitive dependency"
    );
    assert!(
        lockfile_content.contains("utils"),
        "Lockfile should contain utils transitive dependency"
    );

    // Verify install=false is tracked for utils
    assert!(
        lockfile_content.contains("install = false"),
        "Lockfile should track install=false for utils. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that content extraction works for JSON files
#[tokio::test]
async fn test_content_extraction_from_json() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a base command that the JSON file will depend on
    test_repo
        .add_resource(
            "commands",
            "base",
            r#"---
title: Base Command
---
# Base Command

Common deployment setup.
"#,
        )
        .await?;

    // Create a JSON command with dependencies metadata
    // Note: add_resource() always adds .md extension, so we manually create the JSON file
    let commands_dir = test_repo.path.join("commands");
    fs::create_dir_all(&commands_dir).await?;
    fs::write(
        commands_dir.join("deploy-config.json"),
        r#"{
  "dependencies": {
    "commands": [
      {
        "path": "base.md",
        "version": "v1.0.0"
      }
    ]
  },
  "config": {
    "environment": "production",
    "region": "us-west-2"
  }
}"#,
    )
    .await?;

    // Create an agent that embeds the JSON content
    test_repo
        .add_resource(
            "agents",
            "deploy-agent",
            r#"---
title: Deploy Agent
agpm:
  templating: true
dependencies:
  commands:
    - path: ../commands/deploy-config.json
      version: v1.0.0
      install: false
      name: config
---
# Deploy Agent

Config:
```json
{{ agpm.deps.commands.config.content }}
```
"#,
        )
        .await?;

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("deploy-agent", |d| {
            d.source("test-repo").path("agents/deploy-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Read the installed agent
    let agent_path = project.project_path().join(".claude/agents/deploy-agent.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Verify JSON content was embedded (without dependencies field)
    assert!(content.contains("\"environment\": \"production\""), "JSON config should be embedded");
    assert!(content.contains("\"region\": \"us-west-2\""), "JSON config should be complete");

    // Verify dependencies field was stripped from embedded content
    assert!(
        !content.contains("\"dependencies\""),
        "Dependencies metadata should be stripped from JSON content. Content:\n{}",
        content
    );

    Ok(())
}

/// Test that files are cleaned up when install changes from true to false
#[tokio::test]
async fn test_cleanup_when_install_changes_to_false() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create two snippets
    test_repo
        .add_resource(
            "snippets",
            "toggleable",
            r#"---
title: Toggleable Snippet
---
# Toggleable Content
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "snippets",
            "permanent",
            r#"---
title: Permanent Snippet
---
# Permanent Content
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // First install: both snippets with default install=true
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
toggleable = {{ source = "test-repo", path = "snippets/toggleable.md", version = "v1.0.0", tool = "claude-code" }}
permanent = {{ source = "test-repo", path = "snippets/permanent.md", version = "v1.0.0", tool = "claude-code" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    // Verify both files exist
    let toggleable_path = project.project_path().join(".claude/snippets/toggleable.md");
    let permanent_path = project.project_path().join(".claude/snippets/permanent.md");

    assert!(
        fs::metadata(&toggleable_path).await.is_ok(),
        "Toggleable should be installed at {:?}",
        toggleable_path
    );
    assert!(
        fs::metadata(&permanent_path).await.is_ok(),
        "Permanent should be installed at {:?}",
        permanent_path
    );

    // Second install: change toggleable to install=false
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
toggleable = {{ source = "test-repo", path = "snippets/toggleable.md", version = "v1.0.0", tool = "claude-code", install = false }}
permanent = {{ source = "test-repo", path = "snippets/permanent.md", version = "v1.0.0", tool = "claude-code" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Second install should succeed. Stderr: {}", output.stderr);

    // Verify cleanup message
    assert!(
        output.stdout.contains("Cleaned up") || output.stdout.contains("moved or removed"),
        "Should report cleanup. Output: {}",
        output.stdout
    );

    // Verify toggleable was removed
    assert!(
        fs::metadata(&toggleable_path).await.is_err(),
        "Toggleable should be removed after install=false at {:?}",
        toggleable_path
    );

    // Verify permanent still exists
    assert!(
        fs::metadata(&permanent_path).await.is_ok(),
        "Permanent should still exist at {:?}",
        permanent_path
    );

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("toggleable"), "Lockfile should contain toggleable");
    assert!(lockfile_content.contains("permanent"), "Lockfile should contain permanent");
    assert!(
        lockfile_content.contains("install = false"),
        "Lockfile should track install=false. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}
