//! Integration tests for Claude Skills functionality.
//!
//! These tests verify that skills work correctly with the full AGPM workflow
//! including installation, dependency resolution, patching, and validation.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use std::fs;

#[tokio::test]
async fn test_install_single_skill() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill in the source repo
    source
        .create_skill(
            "rust-helper",
            r#"---
name: Rust Helper
description: Helps with Rust development
model: claude-3-opus
temperature: "0.5"
---
# Rust Helper

I help with Rust development tasks.
"#,
        )
        .await?;

    // Create a dependency snippet
    source
        .create_file(
            "snippets/rust-patterns.md",
            r#"---
name: Rust Patterns
description: Common Rust patterns
---
# Rust Patterns

Useful Rust patterns and idioms.
"#,
        )
        .await?;

    source.commit_all("Add rust-helper skill and dependency")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("rust-helper", |d| d.source("test").path("skills/rust-helper").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill was installed
    let skill_path = project.project_path().join(".claude/skills/rust-helper");

    assert!(skill_path.exists());
    assert!(skill_path.join("SKILL.md").exists());

    // Verify content is correct
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();
    assert!(content.contains("name: Rust Helper"));
    assert!(content.contains("description: Helps with Rust development"));
    Ok(())
}

#[tokio::test]
async fn test_install_skill_with_patches() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill in the source repo
    source
        .create_skill(
            "my-skill",
            r#"---
name: My Test Skill
description: A skill for testing
model: claude-3-opus
temperature: "0.5"
---
# My Test Skill

This is a test skill.
"#,
        )
        .await?;

    source.commit_all("Add my-skill")?;
    let source_url = source.bare_file_url(project.sources_path())?;

    // Create manifest with patches
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("my-skill", |d| d.source("test").path("skills/my-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Create private manifest with patches
    let private_content = r#"
[patch.skills.my-skill]
model = "claude-3-haiku"
temperature = "0.7"
max_tokens = 2000
"#;
    project.write_private_manifest(private_content).await?;

    project.run_agpm(&["install"])?;

    // Verify patches were applied
    let skill_path = project.project_path().join(".claude/skills/my-skill");
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();

    assert!(content.contains("model: claude-3-haiku"));
    assert!(content.contains("temperature: '0.7'"));
    assert!(content.contains("max_tokens: 2000"));
    // Original value should be overridden
    assert!(!content.contains("claude-3-opus"));
    assert!(!content.contains("temperature: \"0.5\""));
    Ok(())
}

#[tokio::test]
async fn test_install_multiple_skills_pattern() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create multiple skills
    source
        .create_skill(
            "skill1",
            r#"---
name: Skill One
description: First test skill
---
# Skill One
"#,
        )
        .await?;

    source
        .create_skill(
            "skill2",
            r#"---
name: Skill Two
description: Second test skill
---
# Skill Two
"#,
        )
        .await?;

    source
        .create_skill(
            "skill3",
            r#"---
name: Skill Three
description: Third test skill
---
# Skill Three
"#,
        )
        .await?;

    source.commit_all("Add multiple skills")?;

    // Install with pattern
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("all", |d| d.source("test").path("skills/*").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify all skills were installed
    assert!(project.project_path().join(".claude/skills/skill1").exists());
    assert!(project.project_path().join(".claude/skills/skill2").exists());
    assert!(project.project_path().join(".claude/skills/skill3").exists());

    // Verify content
    let expected_names =
        [("skill1", "Skill One"), ("skill2", "Skill Two"), ("skill3", "Skill Three")];
    for (skill_name, expected_display_name) in expected_names {
        let skill_path = project.project_path().join(".claude/skills").join(skill_name);
        assert!(skill_path.exists(), "Skill directory {} does not exist", skill_name);

        let skill_md_path = skill_path.join("SKILL.md");
        assert!(skill_md_path.exists(), "SKILL.md does not exist in {}", skill_name);

        let content = fs::read_to_string(&skill_md_path).unwrap();
        assert!(content.contains(&format!("name: {}", expected_display_name)));
    }
    Ok(())
}

#[tokio::test]
async fn test_skill_with_transitive_dependencies() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create dependency resources
    source
        .add_resource(
            "agents",
            "base-agent",
            r#"---
name: Base Agent
description: A base agent for testing
---
# Base Agent
"#,
        )
        .await?;

    source
        .create_file(
            "snippets/utils.md",
            r#"---
name: Utility Snippets
description: Useful utility snippets
---
# Utility Snippets
"#,
        )
        .await?;

    // Create skill that depends on both
    source
        .create_skill(
            "complex-skill",
            r#"---
name: Complex Skill
description: A skill with dependencies
dependencies:
  agents:
    - path: agents/base-agent.md
  snippets:
    - path: snippets/utils.md
---
# Complex Skill

This skill depends on other resources.
"#,
        )
        .await?;

    source.commit_all("Add skill with dependencies")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("complex-skill", |d| {
            d.source("test").path("skills/complex-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill and its dependencies were installed
    assert!(project.project_path().join(".claude/skills/complex-skill").exists());
    assert!(project.project_path().join(".claude/agents/base-agent.md").exists());
    assert!(project.project_path().join(".claude/snippets/utils.md").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_validation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a valid skill
    source
        .create_skill(
            "valid-skill",
            r#"---
name: Valid Skill
description: A properly formatted skill
---
# Valid Skill
"#,
        )
        .await?;

    source.commit_all("Add valid skill")?;

    // Create manifest
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Install the skill
    project.run_agpm(&["install"])?;

    // Run validation to verify the installation
    let result = project.run_agpm(&["validate", "--paths"])?;
    assert!(result.success);

    // Verify skill was installed correctly
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_list_command() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create skills
    source
        .create_skill(
            "skill-a",
            r#"---
name: Skill A
description: First skill for listing
---
# Skill A
"#,
        )
        .await?;

    source
        .create_skill(
            "skill-b",
            r#"---
name: Skill B
description: Second skill for listing
---
# Skill B
"#,
        )
        .await?;

    source.commit_all("Add skills for listing")?;

    // Create manifest
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("skill-a", |d| d.source("test").path("skills/skill-a").version("HEAD"))
        .add_skill("skill-b", |d| d.source("test").path("skills/skill-b").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Install skills
    project.run_agpm(&["install"])?;

    // List skills
    let result = project.run_agpm(&["list", "--skills"])?;
    assert!(result.success);
    assert!(result.stdout.contains("skill-a"));
    assert!(result.stdout.contains("skill-b"));
    Ok(())
}

#[tokio::test]
async fn test_remove_skill() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill
    source
        .create_skill(
            "removable-skill",
            r#"---
name: Removable Skill
description: A skill that can be removed
---
# Removable Skill
"#,
        )
        .await?;

    source.commit_all("Add removable skill")?;

    // Create manifest and install
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("removable-skill", |d| {
            d.source("test").path("skills/removable-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill is installed
    assert!(project.project_path().join(".claude/skills/removable-skill").exists());

    // Remove skill from manifest
    project.run_agpm(&["remove", "dep", "skill", "removable-skill"])?;

    // Verify skill was removed from manifest
    let manifest_content = fs::read_to_string(project.project_path().join("agpm.toml")).unwrap();
    assert!(!manifest_content.contains("removable-skill"));
    Ok(())
}

#[tokio::test]
async fn test_skill_complete_removal_and_reinstallation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with multiple files for comprehensive testing
    source
        .create_skill(
            "comprehensive-skill",
            r#"---
name: Comprehensive Test Skill
description: A skill with multiple files for testing complete removal
model: claude-3-opus
temperature: "0.7"
---
# Comprehensive Test Skill

This skill tests complete removal and reinstallation.
"#,
        )
        .await?;

    // Add additional files to the skill directory
    let skill_source_dir = source.path.join("skills").join("comprehensive-skill");
    fs::write(skill_source_dir.join("config.json"), r#"{"setting": "value"}"#)?;
    fs::write(skill_source_dir.join("script.sh"), "#!/bin/bash\necho 'Hello World'")?;

    // Create a subdirectory with nested content
    fs::create_dir_all(skill_source_dir.join("utils"))?;
    fs::write(skill_source_dir.join("utils/helper.txt"), "Helper content")?;

    source.commit_all("Add comprehensive skill with multiple files")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("comprehensive-skill", |d| {
            d.source("test").path("skills/comprehensive-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill was installed completely
    let skill_path = project.project_path().join(".claude/skills/comprehensive-skill");
    assert!(skill_path.exists(), "Skill directory should exist after installation");
    assert!(skill_path.is_dir(), "Skill should be a directory");
    assert!(skill_path.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(skill_path.join("config.json").exists(), "config.json should exist");
    assert!(skill_path.join("script.sh").exists(), "script.sh should exist");
    assert!(skill_path.join("utils/helper.txt").exists(), "Nested file should exist");

    // Verify skill appears in lockfile with checksum
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("comprehensive-skill"), "Skill should be in lockfile");
    assert!(lockfile_content.contains("checksum = \"sha256:"), "Skill should have checksum");

    // Add an extra file directly to installed directory (should be removed during reinstallation)
    fs::write(skill_path.join("extra-file.txt"), "This should be removed")?;
    assert!(skill_path.join("extra-file.txt").exists(), "Extra file should exist initially");

    // Remove the skill from the manifest
    project.run_agpm(&["remove", "dep", "skill", "comprehensive-skill"])?;

    // Verify complete removal: directory should be gone
    assert!(!skill_path.exists(), "Skill directory should be completely removed after removal");
    assert!(
        !project.project_path().join(".claude/skills/comprehensive-skill").exists(),
        "Skill directory should not exist in any form"
    );

    // Verify skill was removed from manifest
    let updated_manifest = fs::read_to_string(project.project_path().join("agpm.toml")).unwrap();
    assert!(
        !updated_manifest.contains("comprehensive-skill"),
        "Skill should be removed from manifest"
    );

    // Verify skill was removed from lockfile
    let updated_lockfile = project.read_lockfile().await?;
    assert!(
        !updated_lockfile.contains("comprehensive-skill"),
        "Skill should be removed from lockfile"
    );

    // Verify no artifacts remain - the entire skills directory structure for this skill should be gone
    let skills_dir = project.project_path().join(".claude/skills");
    if skills_dir.exists() {
        let entries: Vec<_> = fs::read_dir(skills_dir)?.collect::<Result<Vec<_>, _>>()?;
        assert!(
            !entries.iter().any(|entry| {
                entry.file_name().to_string_lossy().contains("comprehensive-skill")
            }),
            "No skill-related artifacts should remain"
        );
    }

    // Now re-add the skill to the manifest and reinstall
    let reinstallation_manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("comprehensive-skill", |d| {
            d.source("test").path("skills/comprehensive-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&reinstallation_manifest).await?;

    project.run_agpm(&["install"])?;

    // Verify successful reinstallation
    assert!(skill_path.exists(), "Skill directory should exist after reinstallation");
    assert!(skill_path.is_dir(), "Skill should be a directory after reinstallation");
    assert!(skill_path.join("SKILL.md").exists(), "SKILL.md should exist after reinstallation");
    assert!(
        skill_path.join("config.json").exists(),
        "config.json should exist after reinstallation"
    );
    assert!(skill_path.join("script.sh").exists(), "script.sh should exist after reinstallation");
    assert!(
        skill_path.join("utils/helper.txt").exists(),
        "Nested file should exist after reinstallation"
    );

    // Verify extra file was removed during reinstallation (clean reinstall)
    assert!(
        !skill_path.join("extra-file.txt").exists(),
        "Extra file should be removed during clean reinstallation"
    );

    // Verify skill appears back in lockfile with new checksum
    let final_lockfile = project.read_lockfile().await?;
    assert!(final_lockfile.contains("comprehensive-skill"), "Skill should be back in lockfile");
    assert!(
        final_lockfile.contains("checksum = \"sha256:"),
        "Skill should have checksum after reinstallation"
    );

    // Verify content integrity after reinstallation
    let skill_content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();
    assert!(skill_content.contains("Comprehensive Test Skill"), "Skill content should be correct");

    let config_content = fs::read_to_string(skill_path.join("config.json")).unwrap();
    assert!(config_content.contains("\"setting\""), "Config file should be correct");

    let helper_content = fs::read_to_string(skill_path.join("utils/helper.txt")).unwrap();
    assert_eq!(helper_content, "Helper content", "Nested file content should be correct");

    Ok(())
}

#[tokio::test]
async fn test_skill_with_private_patches() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill
    source
        .create_skill(
            "patchable-skill",
            r#"---
name: Patchable Skill
description: A skill for testing private patches
model: claude-3-opus
---
# Patchable Skill
"#,
        )
        .await?;

    source.commit_all("Add patchable skill")?;

    // Create manifest with project patches
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("patchable-skill", |d| {
            d.source("test").path("skills/patchable-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Create project patches
    let project_patches = r#"
[patch.skills.patchable-skill]
model = "claude-3-sonnet"
temperature = "0.5"
"#;
    fs::write(
        project.project_path().join("agpm.toml"),
        format!("{}\n{}", manifest_content, project_patches),
    )?;

    // Create private patches file
    let private_patches = r#"
[patch.skills.patchable-skill]
temperature = "0.9"
max_tokens = 1000
"#;
    fs::write(project.project_path().join("agpm.private.toml"), private_patches)?;

    // Install with both project and private patches
    project.run_agpm(&["install"])?;

    // Verify patches were applied
    let skill_path = project.project_path().join(".claude/skills/patchable-skill");
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();

    // Project patch should be overridden by private patch
    assert!(content.contains("model: claude-3-sonnet"));
    assert!(content.contains("temperature: '0.9'")); // Private wins
    assert!(content.contains("max_tokens: 1000")); // Private only
    Ok(())
}

// Error scenario tests for skills

#[tokio::test]
async fn test_skill_missing_skill_md() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill directory without SKILL.md
    let skill_dir = source.path.join("skills").join("incomplete-skill");
    fs::create_dir_all(&skill_dir)?;
    // Create a different file but not SKILL.md
    fs::write(skill_dir.join("README.md"), "# Readme")?;

    source.commit_all("Add incomplete skill")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("incomplete-skill", |d| {
            d.source("test").path("skills/incomplete-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("SKILL.md not found")
            || result.stderr.contains("missing SKILL.md")
            || result.stderr.contains("missing required SKILL.md")
            || result.stderr.contains("file access")
            || result.stderr.contains("cannot be found")
            || result.stderr.contains("Skill directory missing required SKILL.md")
            || result.stderr.contains("Failed to fetch resource")  // Error wrapping in transitive resolver
            || result.stderr.contains("Installation incomplete"), // Top-level installation error
        "Expected error about missing SKILL.md, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/incomplete-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_invalid_frontmatter() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with malformed YAML frontmatter
    source
        .create_skill(
            "invalid-frontmatter",
            r#"---
name: Invalid Frontmatter
description: A skill with bad YAML
model: claude-3-opus
temperature: "0.5"
invalid_yaml: [unclosed array
---
# Invalid Frontmatter

This skill has malformed YAML.
"#,
        )
        .await?;

    source.commit_all("Add skill with invalid frontmatter")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("invalid-frontmatter", |d| {
            d.source("test").path("skills/invalid-frontmatter").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("Failed to parse")
            || result.stderr.contains("YAML")
            || result.stderr.contains("frontmatter"),
        "Expected error about parsing failure, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/invalid-frontmatter").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_missing_required_fields() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill missing required 'name' field
    source
        .create_skill(
            "missing-name",
            r#"---
description: A skill missing the name field
model: claude-3-opus
---
# Missing Name

This skill is missing the required name field.
"#,
        )
        .await?;

    source.commit_all("Add skill missing required field")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("missing-name", |d| d.source("test").path("skills/missing-name").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("missing required field")
            || result.stderr.contains("name")
            || result.stderr.contains("validation"),
        "Expected error about missing required field, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/missing-name").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_path_traversal_attempt() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a normal skill but use malicious path in manifest
    source
        .create_skill(
            "malicious-skill",
            r#"---
name: Malicious Skill
description: A skill trying to escape directory
model: claude-3-opus
---
# Malicious Skill

This skill tries to traverse paths.
"#,
        )
        .await?;

    source.commit_all("Add malicious skill")?;

    // Try to install the skill using a path that tries to traverse directories
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("malicious", |d| {
            d.source("test").path("skills/../../../malicious-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("path traversal")
            || result.stderr.contains("invalid path")
            || result.stderr.contains("security")
            || result.stderr.contains("outside")
            || result.stderr.contains("file access")
            || result.stderr.contains("Invalid skill directory")
            || result.stderr.contains("Installation incomplete")
            || result.stderr.contains("Failed to fetch resource"),
        "Expected error about path traversal, got: {}",
        result.stderr
    );

    // Verify nothing was installed outside the skills directory
    assert!(!project.project_path().join(".claude/malicious-skill").exists());
    assert!(!project.project_path().join("malicious-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_resource_size_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with a very large file to test size limits
    let large_content = "x".repeat(200 * 1024 * 1024); // 200MB (exceeds 100MB limit)
    source
        .create_skill(
            "large-skill",
            &format!(
                r#"---
name: Large Skill
description: A skill with oversized content
model: claude-3-opus
---
# Large Skill

This skill contains a large file.

{}

Large content here.
"#,
                large_content
            ),
        )
        .await?;

    source.commit_all("Add oversized skill")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("large-skill", |d| d.source("test").path("skills/large-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("size limit")
            || result.stderr.contains("too large")
            || result.stderr.contains("resource limit")
            || result.stderr.contains("exceeds the maximum limit")
            || result.stderr.contains("Invalid skill directory")
            || result.stderr.contains("Installation incomplete")
            || result.stderr.contains("Failed to fetch resource"),
        "Expected error about size limit, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/large-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_file_count_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill directory with many files to test file count limit
    let skill_dir = source.path.join("skills").join("many-files-skill");
    fs::create_dir_all(&skill_dir)?;

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Many Files Skill
description: A skill with too many files
model: claude-3-opus
---
# Many Files Skill

This skill has too many files.
"#,
    )?;

    // Create many additional files (exceeding 1000 file limit)
    for i in 0..1100 {
        fs::write(skill_dir.join(format!("file_{:04}.txt", i)), format!("Content of file {}", i))?;
    }

    source.commit_all("Add skill with too many files")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("many-files-skill", |d| {
            d.source("test").path("skills/many-files-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("file count limit")
            || result.stderr.contains("too many files")
            || result.stderr.contains("resource limit")
            || result.stderr.contains("exceeds the maximum limit")
            || result.stderr.contains("exceeds maximum file count")
            || result.stderr.contains("Invalid skill directory")
            || result.stderr.contains("Installation incomplete")
            || result.stderr.contains("Failed to fetch resource"),
        "Expected error about file count limit, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/many-files-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_installation_rollback() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a valid skill first
    source
        .create_skill(
            "valid-skill",
            r#"---
name: Valid Skill
description: A valid skill for rollback test
model: claude-3-opus
---
# Valid Skill

This skill should install successfully.
"#,
        )
        .await?;

    source.commit_all("Add valid skill")?;

    // Install the valid skill first
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify the valid skill was installed
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());

    // Now create an invalid skill and add it to the manifest
    source
        .create_skill(
            "invalid-skill",
            r#"---
description: Missing required name field
model: claude-3-opus
---
# Invalid Skill

This skill should fail.
"#,
        )
        .await?;

    source.commit_all("Add invalid skill")?;

    // Update manifest to include both skills
    let updated_manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .add_skill("invalid-skill", |d| {
            d.source("test").path("skills/invalid-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&updated_manifest_content).await?;

    // Try to install again - the invalid one should fail but the valid one should remain
    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");

    // Verify the valid skill still exists (AGPM doesn't rollback on partial failures)
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());
    // Verify the invalid skill was not installed
    assert!(!project.project_path().join(".claude/skills/invalid-skill").exists());

    Ok(())
}

#[tokio::test]
async fn test_skill_sensitive_path_validation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a normal skill but try to install it to a sensitive path via manifest
    source
        .create_skill(
            "sensitive-skill",
            r#"---
name: Sensitive Skill
description: A skill being installed to sensitive path
model: claude-3-opus
---
# Sensitive Skill

This skill is being installed to a sensitive path.
"#,
        )
        .await?;

    source.commit_all("Add skill for sensitive path test")?;

    // Try to install the skill to a sensitive path
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("sensitive", |d| d.source("test").path("skills/.git").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("sensitive")
            || result.stderr.contains("reserved")
            || result.stderr.contains("invalid path")
            || result.stderr.contains("file access")
            || result.stderr.contains("Invalid skill directory")
            || result.stderr.contains("Installation incomplete")
            || result.stderr.contains("Failed to fetch resource"),
        "Expected error about sensitive path, got: {}",
        result.stderr
    );

    // Verify .git directory was not touched
    let git_dir = project.project_path().join(".claude/skills/.git");
    assert!(!git_dir.exists(), "Sensitive .git directory should not exist");
    Ok(())
}
