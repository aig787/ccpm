//! Integration tests for skills directory-based installation

use tokio::fs;

use crate::common::TestProject;

/// Test that skills install as directories and get checksums
#[tokio::test]
async fn test_skill_directory_installation() {
    let project = TestProject::new().await.unwrap();

    // Create a test skill directory
    let skill_dir = project.sources_path().join("test-skill");
    fs::create_dir_all(&skill_dir).await.unwrap();

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"
---
name: test-skill
description: Test skill for directory installation
version: 0.1.0
agpm:
  templating: false
---

# Test Skill

This is a test skill that installs as a directory.
"#,
    )
    .await
    .unwrap();

    // Create additional files
    fs::write(skill_dir.join("script.sh"), "#!/bin/bash\necho 'Hello World'").await.unwrap();
    fs::write(skill_dir.join("config.json"), r#"{"option": "value"}"#).await.unwrap();

    // Create subdirectory with file
    fs::create_dir_all(skill_dir.join("subdir")).await.unwrap();
    fs::write(skill_dir.join("subdir/nested.txt"), "Nested content").await.unwrap();

    // Create agpm.toml with skill dependency
    let manifest = r#"
[skills]
test-skill = { path = "../sources/test-skill" }
"#;
    project.write_manifest(manifest).await.unwrap();

    // Install the skill
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Check that skill was installed as a directory
    let installed_skill = project.project_path().join(".claude/skills/test-skill");
    assert!(installed_skill.exists());
    assert!(installed_skill.is_dir());

    // Verify all files were copied
    assert!(installed_skill.join("SKILL.md").exists());
    assert!(installed_skill.join("script.sh").exists());
    assert!(installed_skill.join("config.json").exists());
    assert!(installed_skill.join("subdir/nested.txt").exists());

    // Check that checksum is populated in lockfile
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = fs::read_to_string(&lockfile_path).await.unwrap();
    assert!(lockfile_content.contains("checksum = \"sha256:"));

    // Parse lockfile and verify the skill entry has a checksum
    let lockfile: toml::Value = toml::from_str(&lockfile_content).unwrap();
    if let Some(skills) = lockfile.get("skills").and_then(|v| v.as_array()) {
        let skill_entry = skills
            .iter()
            .find(|e| {
                e.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.contains("test-skill"))
                    .unwrap_or(false)
            })
            .expect("Should find test-skill in lockfile");

        let checksum = skill_entry
            .get("checksum")
            .and_then(|c| c.as_str())
            .expect("Skill should have a checksum");

        assert!(checksum.starts_with("sha256:"));
        assert_ne!(checksum, "sha256:");
        println!("Skill checksum: {}", checksum);
    }
}

/// Test that reinstalling a skill removes old files
#[tokio::test]
async fn test_skill_reinstall_cleanup() {
    let project = TestProject::new().await.unwrap();

    // Create initial skill
    let skill_dir = project.sources_path().join("test-skill-v2");
    fs::create_dir_all(&skill_dir).await.unwrap();

    fs::write(
        skill_dir.join("SKILL.md"),
        r#"
---
name: test-skill
description: Test skill for clean reinstallation
version: 0.1.0
---
# Test Skill V2
"#,
    )
    .await
    .unwrap();

    fs::write(skill_dir.join("file1.txt"), "Original content").await.unwrap();
    fs::write(skill_dir.join("obsolete.txt"), "This will be removed").await.unwrap();

    // Create agpm.toml
    let manifest = r#"
[skills]
test-skill = { path = "../sources/test-skill-v2" }
"#;
    project.write_manifest(manifest).await.unwrap();

    // Initial installation
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    let installed_skill = project.project_path().join(".claude/skills/test-skill");

    // Verify all files exist
    assert!(installed_skill.join("file1.txt").exists());
    assert!(installed_skill.join("obsolete.txt").exists());

    // Add an extra file directly to installed directory (should be removed on reinstall)
    fs::write(installed_skill.join("extra.txt"), "Extra file").await.unwrap();
    assert!(installed_skill.join("extra.txt").exists());

    // Update the skill - remove obsolete.txt and add file2.txt
    fs::remove_file(skill_dir.join("obsolete.txt")).await.unwrap();
    fs::write(skill_dir.join("file2.txt"), "New content").await.unwrap();
    fs::write(skill_dir.join("file1.txt"), "Updated content").await.unwrap();

    // Reinstall
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify extra file was removed
    assert!(
        !installed_skill.join("extra.txt").exists(),
        "Extra file should be removed during reinstall"
    );

    // Verify obsolete file is gone
    assert!(
        !installed_skill.join("obsolete.txt").exists(),
        "Obsolete file should be removed during reinstall"
    );

    // Verify new/updated files exist
    assert!(installed_skill.join("file1.txt").exists());
    assert!(installed_skill.join("file2.txt").exists());

    // Verify content was updated
    let file1_content = fs::read_to_string(installed_skill.join("file1.txt")).await.unwrap();
    assert_eq!(file1_content, "Updated content");
}

/// Test that skills exceeding size limit are rejected
///
/// Related: TODO #3 - Enforce size limits during installation
#[tokio::test]
async fn test_skill_rejects_oversized() {
    let project = TestProject::new().await.unwrap();

    // Create a skill that exceeds the 100MB limit
    let skill_dir = project.sources_path().join("huge-skill");
    fs::create_dir_all(&skill_dir).await.unwrap();

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: huge-skill
description: Skill that exceeds size limit
---
# Huge Skill
"#,
    )
    .await
    .unwrap();

    // Create a 101MB file (just over the limit)
    let large_content = vec![0u8; 101 * 1024 * 1024]; // 101 MB
    fs::write(skill_dir.join("huge.bin"), large_content).await.unwrap();

    // Create agpm.toml with skill dependency
    let manifest = r#"
[skills]
huge-skill = { path = "../sources/huge-skill" }
"#;
    project.write_manifest(manifest).await.unwrap();

    // Install should fail due to size validation
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(!output.success, "Install should fail for oversized skill. Stderr: {}", output.stderr);
    // The error may be wrapped in generic error handling, so just verify install failed
    assert!(
        output.stderr.contains("Skill size validation failed")
            || output.stderr.contains("exceeds")
            || output.stderr.contains("Installation incomplete"),
        "Error message should indicate validation failure. Stderr: {}",
        output.stderr
    );
}

/// Test that skills with too many files are rejected
///
/// Related: TODO #3 - Enforce size limits during installation
#[tokio::test]
async fn test_skill_rejects_too_many_files() {
    let project = TestProject::new().await.unwrap();

    // Create a skill with 1001 files (over the 1000 limit)
    let skill_dir = project.sources_path().join("many-files-skill");
    fs::create_dir_all(&skill_dir).await.unwrap();

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: many-files
description: Skill with too many files
---
# Many Files
"#,
    )
    .await
    .unwrap();

    // Create 1001 small files
    for i in 0..1001 {
        fs::write(skill_dir.join(format!("file{}.txt", i)), format!("content {}", i))
            .await
            .unwrap();
    }

    // Create agpm.toml
    let manifest = r#"
[skills]
many-files = { path = "../sources/many-files-skill" }
"#;
    project.write_manifest(manifest).await.unwrap();

    // Install should fail due to file count validation
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        !output.success,
        "Install should fail for skill with too many files. Stderr: {}",
        output.stderr
    );
    // The error may be wrapped in generic error handling, so just verify install failed
    assert!(
        output.stderr.contains("Skill size validation failed")
            || output.stderr.contains("files")
            || output.stderr.contains("Installation incomplete"),
        "Error message should indicate validation failure. Stderr: {}",
        output.stderr
    );
}

/// Test that skills containing symlinks are rejected for security
///
/// Related: TODO #3 - Enforce size limits during installation (security)
#[tokio::test]
#[cfg(unix)] // Symlinks work differently on Windows
async fn test_skill_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let project = TestProject::new().await.unwrap();

    // Create a skill with a symlink
    let skill_dir = project.sources_path().join("symlink-skill");
    fs::create_dir_all(&skill_dir).await.unwrap();

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: symlink-skill
description: Skill with symlink (should be rejected)
---
# Symlink Skill
"#,
    )
    .await
    .unwrap();

    // Create a regular file
    fs::write(skill_dir.join("regular.txt"), "regular content").await.unwrap();

    // Create a symlink (security risk: could point to sensitive files)
    let link_path = skill_dir.join("dangerous-link");
    symlink("/etc/passwd", &link_path).unwrap();

    // Create agpm.toml
    let manifest = r#"
[skills]
symlink-skill = { path = "../sources/symlink-skill" }
"#;
    project.write_manifest(manifest).await.unwrap();

    // Install should fail
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(!output.success, "Install should fail for skill with symlinks");
    assert!(
        output.stderr.contains("symlink"),
        "Error should mention symlinks. Stderr: {}",
        output.stderr
    );
}
