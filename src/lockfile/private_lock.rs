//! Private lockfile management for user-level patches.
//!
//! The private lockfile (`agpm.private.lock`) tracks patches from `agpm.private.toml`
//! separately from the project lockfile. This allows team members to have different
//! private patches without causing lockfile conflicts.
//!
//! # Structure
//!
//! The private lockfile uses the same array-based format as `agpm.lock`, storing
//! only resources that have private patches applied:
//!
//! ```toml
//! version = 1
//!
//! [[agents]]
//! name = "my-agent"
//! applied_patches = { temperature = "0.9", custom_field = "value" }
//!
//! [[commands]]
//! name = "deploy"
//! applied_patches = { timeout = "300" }
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! use agpm_cli::lockfile::private_lock::PrivateLockFile;
//! use std::path::Path;
//!
//! let project_dir = Path::new(".");
//! let mut private_lock = PrivateLockFile::new();
//!
//! // Add private patches for a resource
//! let patches = std::collections::BTreeMap::from([
//!     ("temperature".to_string(), toml::Value::String("0.9".into())),
//! ]);
//! private_lock.add_private_patches("agents", "my-agent", patches);
//!
//! // Save to disk - creates an array-based lockfile matching agpm.lock format
//! private_lock.save(project_dir)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const PRIVATE_LOCK_FILENAME: &str = "agpm.private.lock";
const PRIVATE_LOCK_VERSION: u32 = 1;

/// A resource entry in the private lockfile.
///
/// Contains only the essential fields needed to identify a resource and
/// track its private patches. This matches the structure used in `agpm.lock`
/// but stores only resources with private patches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrivateLockedResource {
    /// Resource name from the manifest
    pub name: String,

    /// Applied private patches
    ///
    /// Contains the key-value pairs from `agpm.private.toml` that override
    /// the resource's default configuration or project-level patches.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub applied_patches: BTreeMap<String, toml::Value>,
}

/// Private lockfile tracking user-level patches.
///
/// This file is gitignored and contains patches from `agpm.private.toml` only.
/// It works alongside `agpm.lock` to provide full reproducibility while keeping
/// team lockfiles deterministic.
///
/// Uses the same array-based format as `agpm.lock` for consistency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrivateLockFile {
    /// Lockfile format version
    pub version: u32,

    /// Private patches for agents
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<PrivateLockedResource>,

    /// Private patches for snippets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<PrivateLockedResource>,

    /// Private patches for commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<PrivateLockedResource>,

    /// Private patches for scripts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<PrivateLockedResource>,

    /// Private patches for MCP servers
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "mcp-servers")]
    pub mcp_servers: Vec<PrivateLockedResource>,

    /// Private patches for hooks
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<PrivateLockedResource>,

    /// Private patches for skills
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<PrivateLockedResource>,
}

impl Default for PrivateLockFile {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivateLockFile {
    /// Create a new empty private lockfile.
    pub fn new() -> Self {
        Self {
            version: PRIVATE_LOCK_VERSION,
            agents: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            mcp_servers: Vec::new(),
            hooks: Vec::new(),
            skills: Vec::new(),
        }
    }

    /// Load private lockfile from disk.
    ///
    /// Returns `Ok(None)` if the file doesn't exist (no private patches).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::private_lock::PrivateLockFile;
    /// use std::path::Path;
    ///
    /// let project_dir = Path::new(".");
    /// match PrivateLockFile::load(project_dir)? {
    ///     Some(lock) => println!("Loaded {} private patches", lock.total_patches()),
    ///     None => println!("No private lockfile found"),
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load(project_dir: &Path) -> Result<Option<Self>> {
        let path = project_dir.join(PRIVATE_LOCK_FILENAME);
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let lock: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        // Validate version
        if lock.version > PRIVATE_LOCK_VERSION {
            anyhow::bail!(
                "Private lockfile version {} is newer than supported version {}. \
                 Please upgrade AGPM.",
                lock.version,
                PRIVATE_LOCK_VERSION
            );
        }

        Ok(Some(lock))
    }

    /// Save private lockfile to disk.
    ///
    /// Deletes the file if the lockfile is empty (no private patches).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::private_lock::PrivateLockFile;
    /// use std::path::Path;
    ///
    /// let mut lock = PrivateLockFile::new();
    /// // ... add patches ...
    /// lock.save(Path::new("."))?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join(PRIVATE_LOCK_FILENAME);

        // Don't create empty lockfiles; delete if exists
        if self.is_empty() {
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove {}", path.display()))?;
            }
            return Ok(());
        }

        let content = serialize_private_lockfile_with_inline_patches(self)?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        Ok(())
    }

    /// Check if the lockfile has any patches.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
            && self.snippets.is_empty()
            && self.commands.is_empty()
            && self.scripts.is_empty()
            && self.mcp_servers.is_empty()
            && self.hooks.is_empty()
    }

    /// Count total number of resources with private patches.
    pub fn total_patches(&self) -> usize {
        self.agents.len()
            + self.snippets.len()
            + self.commands.len()
            + self.scripts.len()
            + self.mcp_servers.len()
            + self.hooks.len()
    }

    /// Add private patches for a resource.
    ///
    /// If patches is empty, this is a no-op. If a resource with the same name
    /// already exists, its patches are replaced.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::private_lock::PrivateLockFile;
    /// use std::collections::BTreeMap;
    ///
    /// let mut lock = PrivateLockFile::new();
    /// let patches = BTreeMap::from([
    ///     ("model".to_string(), toml::Value::String("haiku".into())),
    /// ]);
    /// lock.add_private_patches("agents", "my-agent", patches);
    /// ```
    pub fn add_private_patches(
        &mut self,
        resource_type: &str,
        name: &str,
        patches: BTreeMap<String, toml::Value>,
    ) {
        if patches.is_empty() {
            return;
        }

        let vec = match resource_type {
            "agents" => &mut self.agents,
            "snippets" => &mut self.snippets,
            "commands" => &mut self.commands,
            "scripts" => &mut self.scripts,
            "mcp-servers" => &mut self.mcp_servers,
            "hooks" => &mut self.hooks,
            "skills" => &mut self.skills,
            _ => return,
        };

        // Remove existing entry if present
        vec.retain(|r| r.name != name);

        // Add new entry
        vec.push(PrivateLockedResource {
            name: name.to_string(),
            applied_patches: patches,
        });
    }

    /// Get private patches for a specific resource.
    pub fn get_patches(
        &self,
        resource_type: &str,
        name: &str,
    ) -> Option<&BTreeMap<String, toml::Value>> {
        let vec = match resource_type {
            "agents" => &self.agents,
            "snippets" => &self.snippets,
            "commands" => &self.commands,
            "scripts" => &self.scripts,
            "mcp-servers" => &self.mcp_servers,
            "hooks" => &self.hooks,
            "skills" => &self.skills,
            _ => return None,
        };

        vec.iter().find(|r| r.name == name).map(|r| &r.applied_patches)
    }
}

/// Convert private lockfile to TOML string with inline tables for `applied_patches`.
///
/// Uses `toml_edit` to ensure `applied_patches` fields are serialized as inline tables:
/// ```toml
/// [[agents]]
/// name = "my-agent"
/// applied_patches = { model = "haiku", temperature = "0.9" }
/// ```
///
/// Instead of the confusing separate table format:
/// ```toml
/// [[agents]]
/// name = "my-agent"
///
/// [agents.applied_patches]
/// model = "haiku"
/// ```
fn serialize_private_lockfile_with_inline_patches(lockfile: &PrivateLockFile) -> Result<String> {
    use toml_edit::{DocumentMut, Item};

    // First serialize to a toml_edit document
    let toml_str =
        toml::to_string_pretty(lockfile).context("Failed to serialize private lockfile to TOML")?;
    let mut doc: DocumentMut = toml_str.parse().context("Failed to parse TOML document")?;

    // Convert all `applied_patches` tables to inline tables
    use crate::core::ResourceType;
    let resource_types: Vec<&str> = ResourceType::all().iter().map(|rt| rt.to_plural()).collect();

    for resource_type in &resource_types {
        if let Some(Item::ArrayOfTables(array)) = doc.get_mut(resource_type) {
            for table in array.iter_mut() {
                if let Some(Item::Table(patches_table)) = table.get_mut("applied_patches") {
                    // Convert to inline table
                    let mut inline = toml_edit::InlineTable::new();
                    for (key, val) in patches_table.iter() {
                        if let Some(v) = val.as_value() {
                            inline.insert(key, v.clone());
                        }
                    }
                    table.insert("applied_patches", toml_edit::value(inline));
                }
            }
        }
    }

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn test_new_lockfile_is_empty() {
        let lock = PrivateLockFile::new();
        assert!(lock.is_empty());
        assert_eq!(lock.total_patches(), 0);
    }

    #[test]
    fn test_add_private_patches() {
        let mut lock = PrivateLockFile::new();
        let patches = BTreeMap::from([
            ("model".to_string(), toml::Value::String("haiku".into())),
            ("temp".to_string(), toml::Value::String("0.9".into())),
        ]);

        lock.add_private_patches("agents", "my-agent", patches);

        assert!(!lock.is_empty());
        assert_eq!(lock.total_patches(), 1);
        assert!(lock.agents.iter().any(|r| r.name == "my-agent"));
    }

    #[test]
    fn test_empty_patches_not_added() {
        let mut lock = PrivateLockFile::new();
        lock.add_private_patches("agents", "my-agent", BTreeMap::new());
        assert!(lock.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let mut lock = PrivateLockFile::new();

        let patches = BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]);
        lock.add_private_patches("agents", "test", patches);

        // Save
        lock.save(temp_dir.path()).unwrap();

        // Load
        let loaded = PrivateLockFile::load(temp_dir.path()).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap(), lock);
    }

    #[test]
    fn test_empty_lockfile_deletes_file() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join(PRIVATE_LOCK_FILENAME);

        // Create file
        std::fs::write(&lock_path, "test").unwrap();
        assert!(lock_path.exists());

        // Save empty lockfile should delete
        let lock = PrivateLockFile::new();
        lock.save(temp_dir.path()).unwrap();
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_load_nonexistent_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let result = PrivateLockFile::load(temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_patches() {
        let mut lock = PrivateLockFile::new();
        let patches = BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]);
        lock.add_private_patches("agents", "test", patches.clone());

        let retrieved = lock.get_patches("agents", "test");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &patches);

        let missing = lock.get_patches("agents", "nonexistent");
        assert!(missing.is_none());
    }
}
