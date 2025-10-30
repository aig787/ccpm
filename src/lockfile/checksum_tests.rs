use anyhow::Result;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

use agpm_cli::lockfile::LockFile;

/// Create a test directory with files for testing directory checksums
fn create_test_directory(dir: &Path, files: &[(&str, &str)]) -> Result<()> {
    for (relative_path, content) in files {
        let file_path = dir.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, content)?;
    }
    Ok(())
}

#[test]
fn test_compute_directory_checksum_basic() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create test files
    create_test_directory(
        test_dir,
        &[
            ("file1.txt", "Hello, World!"),
            ("file2.txt", "Goodbye, World!"),
            ("subdir/nested.txt", "Nested content"),
        ],
    )?;

    // Compute checksum
    let checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Verify checksum format
    assert!(checksum.starts_with("sha256:"));
    assert_eq!(checksum.len(), 71); // "sha256:" + 64 hex chars

    println!("Directory checksum: {}", checksum);
    Ok(())
}

#[test]
fn test_compute_directory_checksum_deterministic() -> Result<()> {
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    // Create identical directory structures
    let files = &[("a.txt", "content1"), ("b.txt", "content2"), ("subdir/c.txt", "content3")];

    create_test_directory(temp_dir1.path(), files)?;
    create_test_directory(temp_dir2.path(), files)?;

    // Compute checksums for both directories
    let checksum1 = LockFile::compute_directory_checksum(temp_dir1.path())?;
    let checksum2 = LockFile::compute_directory_checksum(temp_dir2.path())?;

    // Checksums should be identical for identical content
    assert_eq!(checksum1, checksum2);

    Ok(())
}

#[test]
fn test_compute_directory_checksum_detects_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create initial files
    create_test_directory(
        test_dir,
        &[("file1.txt", "original content"), ("file2.txt", "unchanged")],
    )?;

    // Compute initial checksum
    let initial_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Modify a file
    fs::write(test_dir.join("file1.txt"), "modified content")?;

    // Compute new checksum
    let modified_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Checksums should be different
    assert_ne!(initial_checksum, modified_checksum);

    Ok(())
}

#[test]
fn test_compute_directory_checksum_detects_file_addition() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create initial files
    create_test_directory(test_dir, &[("file1.txt", "content")])?;

    // Compute initial checksum
    let initial_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Add a new file
    fs::write(test_dir.join("file2.txt"), "new content")?;

    // Compute new checksum
    let new_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Checksums should be different
    assert_ne!(initial_checksum, new_checksum);

    Ok(())
}

#[test]
fn test_compute_directory_checksum_detects_file_deletion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create initial files
    create_test_directory(test_dir, &[("file1.txt", "content1"), ("file2.txt", "content2")])?;

    // Compute initial checksum
    let initial_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Delete a file
    fs::remove_file(test_dir.join("file2.txt"))?;

    // Compute new checksum
    let deleted_checksum = LockFile::compute_directory_checksum(test_dir)?;

    // Checksums should be different
    assert_ne!(initial_checksum, deleted_checksum);

    Ok(())
}

#[test]
fn test_compute_directory_checksum_ignores_hidden_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create visible and hidden files
    create_test_directory(
        test_dir,
        &[
            ("visible.txt", "visible content"),
            (".hidden.txt", "hidden content"),
            ("subdir/.nested_hidden", "nested hidden"),
        ],
    )?;

    // Compute checksum
    let checksum1 = LockFile::compute_directory_checksum(test_dir)?;

    // Add another hidden file
    fs::write(test_dir.join(".another_hidden"), "more hidden content")?;

    // Compute checksum again
    let checksum2 = LockFile::compute_directory_checksum(test_dir)?;

    // Checksums should be the same (hidden files ignored)
    assert_eq!(checksum1, checksum2);

    Ok(())
}

#[test]
fn test_compute_directory_checksum_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path();

    // Should fail on empty directory
    let result = LockFile::compute_directory_checksum(test_dir);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Directory contains no files"));
}

#[test]
fn test_compute_directory_checksum_non_directory() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let file_path = temp_file.path();

    // Should fail on file instead of directory
    let result = LockFile::compute_directory_checksum(file_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Path is not a directory"));
}

#[test]
fn test_compute_checksum_smart_file() -> Result<()> {
    let temp_file = tempfile::NamedTempFile::new()?;
    let file_path = temp_file.path();
    let content = "test content for file";

    // Write content to file
    fs::write(file_path, content)?;

    // Compute checksum using smart method
    let checksum = LockFile::compute_checksum_smart(file_path)?;

    // Verify format
    assert!(checksum.starts_with("sha256:"));
    assert_eq!(checksum.len(), 71);

    // Should match regular file checksum
    let expected_checksum = LockFile::compute_checksum(file_path)?;
    assert_eq!(checksum, expected_checksum);

    Ok(())
}

#[test]
fn test_compute_checksum_smart_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create test files
    create_test_directory(test_dir, &[("file1.txt", "content1"), ("file2.txt", "content2")])?;

    // Compute checksum using smart method
    let checksum = LockFile::compute_checksum_smart(test_dir)?;

    // Should match directory checksum
    let expected_checksum = LockFile::compute_directory_checksum(test_dir)?;
    assert_eq!(checksum, expected_checksum);

    Ok(())
}

#[test]
fn test_verify_checksum_file() -> Result<()> {
    let temp_file = tempfile::NamedTempFile::new()?;
    let file_path = temp_file.path();
    let content = "test content for verification";

    fs::write(file_path, content)?;

    // Compute checksum
    let expected = LockFile::compute_checksum(file_path)?;

    // Verify with correct checksum
    assert!(LockFile::verify_checksum(file_path, &expected)?);

    // Verify with incorrect checksum
    assert!(!LockFile::verify_checksum(file_path, "sha256:wrong")?);

    Ok(())
}

#[test]
fn test_verify_checksum_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create test files
    create_test_directory(test_dir, &[("file.txt", "content")])?;

    // Compute checksum
    let expected = LockFile::compute_directory_checksum(test_dir)?;

    // Verify with correct checksum
    assert!(LockFile::verify_checksum(test_dir, &expected)?);

    // Verify with incorrect checksum
    assert!(!LockFile::verify_checksum(test_dir, "sha256:wrong")?);

    Ok(())
}

#[test]
fn test_directory_checksum_order_independence() -> Result<()> {
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    // Create same files but in different creation order
    create_test_directory(
        temp_dir1.path(),
        &[("b.txt", "content2"), ("a.txt", "content1"), ("c.txt", "content3")],
    )?;

    create_test_directory(
        temp_dir2.path(),
        &[("c.txt", "content3"), ("a.txt", "content1"), ("b.txt", "content2")],
    )?;

    // Checksums should be identical regardless of creation order
    let checksum1 = LockFile::compute_directory_checksum(temp_dir1.path())?;
    let checksum2 = LockFile::compute_directory_checksum(temp_dir2.path())?;

    assert_eq!(checksum1, checksum2);

    Ok(())
}

#[test]
fn test_directory_checksum_subdirectories() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create nested directory structure
    create_test_directory(
        test_dir,
        &[
            ("root.txt", "root content"),
            ("sub1/file1.txt", "sub1 content"),
            ("sub1/subsub/nested.txt", "nested content"),
            ("sub2/file2.txt", "sub2 content"),
        ],
    )?;

    // Should compute successfully
    let checksum = LockFile::compute_directory_checksum(test_dir)?;
    assert!(checksum.starts_with("sha256:"));

    // Verify it detects changes in nested files
    fs::write(test_dir.join("sub1/subsub/nested.txt"), "modified nested")?;
    let new_checksum = LockFile::compute_directory_checksum(test_dir)?;

    assert_ne!(checksum, new_checksum);

    Ok(())
}
