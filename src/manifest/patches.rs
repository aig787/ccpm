//! Manifest patch support for overriding resource metadata.
//!
//! Patches allow users to override YAML frontmatter fields in resources without forking
//! upstream repositories. They work at both project-level (`agpm.toml`) and user-level
//! (`agpm.private.toml`) with clear precedence rules.
//!
//! # Examples
//!
//! ```toml
//! # In agpm.toml or agpm.private.toml
//! [patch.agents.my-agent]
//! model = "claude-3-haiku"
//! temperature = "0.7"
//!
//! [patch.commands.deploy]
//! timeout = "300"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Collection of patches for all resource types.
///
/// Patches are keyed by resource type (agents, snippets, commands, etc.) and then by
/// manifest alias. Each patch contains arbitrary key-value pairs that will be merged
/// into the resource's YAML frontmatter or JSON fields.
///
/// # Precedence
///
/// When patches are defined in both `agpm.toml` and `agpm.private.toml`:
/// - Private patches silently override project patches for the same field
/// - If a field is only defined in one location, that value is used
/// - No error is raised for conflicts - private always wins
///
/// # Examples
///
/// ```toml
/// [patch.agents.gpt-agent]
/// model = "claude-3-haiku"
/// temperature = "0.7"
///
/// [patch.commands.deploy]
/// timeout = "300"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ManifestPatches {
    /// Patches for agent resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: BTreeMap<String, PatchData>,

    /// Patches for snippet resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub snippets: BTreeMap<String, PatchData>,

    /// Patches for command resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commands: BTreeMap<String, PatchData>,

    /// Patches for script resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scripts: BTreeMap<String, PatchData>,

    /// Patches for MCP server resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", rename = "mcp-servers")]
    pub mcp_servers: BTreeMap<String, PatchData>,

    /// Patches for hook resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hooks: BTreeMap<String, PatchData>,

    /// Patches for skill resources.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, PatchData>,
}

/// Arbitrary key-value pairs to override in a resource's metadata.
///
/// This is a free-form map that can contain any valid TOML values (strings, numbers,
/// booleans, arrays, tables). The values will be merged into the resource's YAML
/// frontmatter (for Markdown files) or top-level fields (for JSON files).
///
/// # Examples
///
/// ```toml
/// [patch.agents.my-agent]
/// model = "claude-3-haiku"
/// temperature = "0.7"
/// max_tokens = 2000
/// ```
pub type PatchData = BTreeMap<String, toml::Value>;

/// Result of applying patches, separated by origin.
///
/// This structure tracks which patches came from project-level configuration
/// (`agpm.toml`) vs private configuration (`agpm.private.toml`). This separation
/// ensures that lockfiles remain deterministic across team members.
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::manifest::patches::AppliedPatches;
/// use std::collections::BTreeMap;
///
/// let applied = AppliedPatches {
///     project: BTreeMap::from([
///         ("model".to_string(), toml::Value::String("haiku".into())),
///     ]),
///     private: BTreeMap::from([
///         ("temperature".to_string(), toml::Value::String("0.9".into())),
///     ]),
/// };
///
/// assert!(!applied.is_empty());
/// assert_eq!(applied.total_count(), 2);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppliedPatches {
    /// Patches from `agpm.toml` (project-level).
    pub project: BTreeMap<String, toml::Value>,
    /// Patches from `agpm.private.toml` (user-level).
    pub private: BTreeMap<String, toml::Value>,
}

impl AppliedPatches {
    /// Creates an empty applied patches collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an AppliedPatches from a lockfile's combined patches HashMap.
    ///
    /// The lockfile doesn't distinguish between project and private patches,
    /// so this method places all patches in the `project` field.
    pub fn from_lockfile_patches(patches: &BTreeMap<String, toml::Value>) -> Self {
        Self {
            project: patches.clone(),
            private: BTreeMap::new(),
        }
    }

    /// Checks if no patches were applied.
    pub fn is_empty(&self) -> bool {
        self.project.is_empty() && self.private.is_empty()
    }

    /// Returns the total number of patches applied.
    pub fn total_count(&self) -> usize {
        self.project.len() + self.private.len()
    }
}

/// Origin of a patch (project or private configuration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchOrigin {
    /// Patch defined in project-level `agpm.toml`.
    Project,
    /// Patch defined in user-level `agpm.private.toml`.
    Private,
}

/// A merged patch with its origin information.
///
/// Tracks which fields came from which configuration file for debugging and
/// diagnostic purposes.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedPatch {
    /// The merged patch data.
    pub data: PatchData,
    /// Origin of each field (for diagnostics).
    pub field_origins: BTreeMap<String, PatchOrigin>,
}

impl ManifestPatches {
    /// Creates an empty patches collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if there are any patches defined.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
            && self.snippets.is_empty()
            && self.commands.is_empty()
            && self.scripts.is_empty()
            && self.mcp_servers.is_empty()
            && self.hooks.is_empty()
            && self.skills.is_empty()
    }

    /// Gets the patch data for a specific resource type and alias.
    ///
    /// Returns `None` if no patch is defined for the given resource type and alias.
    pub fn get(&self, resource_type: &str, alias: &str) -> Option<&PatchData> {
        match resource_type {
            "agents" => self.agents.get(alias),
            "snippets" => self.snippets.get(alias),
            "commands" => self.commands.get(alias),
            "scripts" => self.scripts.get(alias),
            "mcp-servers" => self.mcp_servers.get(alias),
            "hooks" => self.hooks.get(alias),
            "skills" => self.skills.get(alias),
            _ => None,
        }
    }

    /// Merges another patches collection into this one.
    ///
    /// Fields from `other` take precedence over fields in `self`. This is used to
    /// merge private patches over project patches.
    ///
    /// # Arguments
    ///
    /// * `other` - The patches to merge in (higher precedence)
    ///
    /// # Returns
    ///
    /// A new `ManifestPatches` with merged data and a map of conflicts detected.
    pub fn merge_with(&self, other: &ManifestPatches) -> (ManifestPatches, Vec<PatchConflict>) {
        let mut merged = self.clone();
        let mut conflicts = Vec::new();

        // Merge each resource type
        Self::merge_resource_patches(&mut merged.agents, &other.agents, "agents", &mut conflicts);
        Self::merge_resource_patches(
            &mut merged.snippets,
            &other.snippets,
            "snippets",
            &mut conflicts,
        );
        Self::merge_resource_patches(
            &mut merged.commands,
            &other.commands,
            "commands",
            &mut conflicts,
        );
        Self::merge_resource_patches(
            &mut merged.scripts,
            &other.scripts,
            "scripts",
            &mut conflicts,
        );
        Self::merge_resource_patches(
            &mut merged.mcp_servers,
            &other.mcp_servers,
            "mcp-servers",
            &mut conflicts,
        );
        Self::merge_resource_patches(&mut merged.hooks, &other.hooks, "hooks", &mut conflicts);
        Self::merge_resource_patches(&mut merged.skills, &other.skills, "skills", &mut conflicts);

        (merged, conflicts)
    }

    /// Helper to merge patches for a specific resource type.
    fn merge_resource_patches(
        base: &mut BTreeMap<String, PatchData>,
        overlay: &BTreeMap<String, PatchData>,
        resource_type: &str,
        conflicts: &mut Vec<PatchConflict>,
    ) {
        for (alias, overlay_patch) in overlay {
            if let Some(base_patch) = base.get_mut(alias) {
                // Merge patches for this alias
                for (key, overlay_value) in overlay_patch {
                    if let Some(base_value) = base_patch.get(key) {
                        // Check for conflicts
                        if base_value != overlay_value {
                            conflicts.push(PatchConflict {
                                resource_type: resource_type.to_string(),
                                alias: alias.clone(),
                                field: key.clone(),
                                project_value: base_value.clone(),
                                private_value: overlay_value.clone(),
                            });
                        }
                    }
                    // Private patch takes precedence
                    base_patch.insert(key.clone(), overlay_value.clone());
                }
            } else {
                // No existing patch, insert the entire private patch
                base.insert(alias.clone(), overlay_patch.clone());
            }
        }
    }

    /// Get all patch data for a specific resource type.
    ///
    /// Returns a reference to the map of patches for the given resource type.
    pub fn get_for_resource_type(
        &self,
        resource_type: &str,
    ) -> Option<&BTreeMap<String, PatchData>> {
        match resource_type {
            "agents" => Some(&self.agents),
            "snippets" => Some(&self.snippets),
            "commands" => Some(&self.commands),
            "scripts" => Some(&self.scripts),
            "mcp-servers" => Some(&self.mcp_servers),
            "hooks" => Some(&self.hooks),
            "skills" => Some(&self.skills),
            _ => None,
        }
    }
}

/// Apply patches to resource file content.
///
/// This function applies patch data to either Markdown files (with YAML frontmatter)
/// or JSON files, returning the modified content and the patches that were applied.
///
/// # Arguments
///
/// * `content` - The original file content
/// * `file_path` - Path to the file (used to determine file type)
/// * `patch_data` - The patches to apply
///
/// # Returns
///
/// A tuple of:
/// - Modified content string
/// - Map of applied patches (for lockfile tracking)
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::manifest::patches::apply_patches_to_content;
/// use std::collections::BTreeMap;
///
/// let content = "---\nmodel: claude-3-opus\n---\n# Agent\n\nContent here.";
/// let mut patches = BTreeMap::new();
/// patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
///
/// let (new_content, applied) = apply_patches_to_content(
///     content,
///     "agent.md",
///     &patches
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn apply_patches_to_content(
    content: &str,
    file_path: &str,
    patch_data: &PatchData,
) -> anyhow::Result<(String, BTreeMap<String, toml::Value>)> {
    tracing::info!(
        "apply_patches_to_content: file={}, patches_empty={}, patch_count={}",
        file_path,
        patch_data.is_empty(),
        patch_data.len()
    );

    if patch_data.is_empty() {
        return Ok((content.to_string(), BTreeMap::new()));
    }

    let file_ext =
        std::path::Path::new(file_path).extension().and_then(|s| s.to_str()).unwrap_or("");

    match file_ext {
        "md" => apply_patches_to_markdown(content, file_path, patch_data),
        "json" => apply_patches_to_json(content, patch_data),
        _ => {
            // For other file types, we can't apply patches
            tracing::warn!(
                "Cannot apply patches to file type '{}' for file: {}",
                file_ext,
                file_path
            );
            Ok((content.to_string(), BTreeMap::new()))
        }
    }
}

/// Apply patches from both project and private sources, tracking origins separately.
///
/// This function applies project patches first, then private patches, and tracks which
/// patches came from which source. This separation is critical for maintaining
/// deterministic lockfiles while allowing user-level customization.
///
/// # Arguments
///
/// * `content` - The original file content
/// * `file_path` - Path to the file (used to determine file type)
/// * `project_patches` - Patches from `agpm.toml`
/// * `private_patches` - Patches from `agpm.private.toml`
///
/// # Returns
///
/// A tuple of:
/// - Modified content string
/// - `AppliedPatches` struct with separated project and private patches
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::manifest::patches::apply_patches_to_content_with_origin;
/// use std::collections::BTreeMap;
///
/// let content = "---\nmodel: claude-3-opus\n---\n# Agent\n";
/// let project = BTreeMap::from([
///     ("model".to_string(), toml::Value::String("haiku".into())),
/// ]);
/// let private = BTreeMap::from([
///     ("temperature".to_string(), toml::Value::String("0.9".into())),
/// ]);
///
/// let (new_content, applied) = apply_patches_to_content_with_origin(
///     content,
///     "agent.md",
///     &project,
///     &private
/// )?;
///
/// assert_eq!(applied.project.len(), 1);
/// assert_eq!(applied.private.len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn apply_patches_to_content_with_origin(
    content: &str,
    file_path: &str,
    project_patches: &PatchData,
    private_patches: &PatchData,
) -> anyhow::Result<(String, AppliedPatches)> {
    // Merge patches first, with private taking precedence over project
    let mut merged_patches = project_patches.clone();
    for (key, value) in private_patches {
        merged_patches.insert(key.clone(), value.clone());
    }

    // Apply the merged patches in a single pass to avoid duplicate frontmatter
    let (final_content, all_applied) =
        apply_patches_to_content(content, file_path, &merged_patches)?;

    // Track which patches were actually applied by origin
    // Note: When both project and private define the same key, we track BOTH
    // even though only the private value ends up in the content
    let mut project_applied = BTreeMap::new();
    let mut private_applied = BTreeMap::new();

    for key in all_applied.keys() {
        // Track project patches
        if let Some(value) = project_patches.get(key) {
            project_applied.insert(key.clone(), value.clone());
        }
        // Track private patches (may override project)
        if let Some(value) = private_patches.get(key) {
            private_applied.insert(key.clone(), value.clone());
        }
    }

    Ok((
        final_content,
        AppliedPatches {
            project: project_applied,
            private: private_applied,
        },
    ))
}

/// Apply patches to Markdown file with YAML frontmatter.
fn apply_patches_to_markdown(
    content: &str,
    file_path: &str,
    patch_data: &PatchData,
) -> anyhow::Result<(String, BTreeMap<String, toml::Value>)> {
    use crate::markdown::MarkdownDocument;

    // Parse the markdown file (pass file_path for warning deduplication)
    let mut md_doc = MarkdownDocument::parse_with_operation_context(
        content,
        Some(file_path),
        None, // No operation context available here, but file path helps deduplication
    )?;

    let mut applied_patches = BTreeMap::new();

    // Apply each patch to the metadata in deterministic order (sorted by key)
    // This ensures consistent file content across runs
    let mut sorted_keys: Vec<_> = patch_data.keys().cloned().collect();
    sorted_keys.sort();

    for key in sorted_keys {
        let value = &patch_data[&key];
        // Convert toml::Value to serde_json::Value for the extra fields
        let json_value = toml_value_to_json(value)?;

        // Get or create metadata
        let metadata = md_doc.metadata.get_or_insert_with(Default::default);

        // Update the extra fields
        metadata.extra.insert(key.clone(), json_value);

        applied_patches.insert(key.clone(), value.clone());
    }

    // Update the document with the modified metadata
    if let Some(metadata) = md_doc.metadata.clone() {
        md_doc.set_metadata(metadata);
    }

    // Return the raw content with frontmatter
    Ok((md_doc.raw, applied_patches))
}

/// Apply patches to JSON file.
fn apply_patches_to_json(
    content: &str,
    patch_data: &PatchData,
) -> anyhow::Result<(String, BTreeMap<String, toml::Value>)> {
    // Parse the JSON
    let mut json_value: serde_json::Value = serde_json::from_str(content)?;

    let mut applied_patches = BTreeMap::new();

    // Apply each patch to the top-level JSON object in deterministic order (sorted by key)
    // This ensures consistent file content across runs
    if let serde_json::Value::Object(ref mut map) = json_value {
        let mut sorted_keys: Vec<_> = patch_data.keys().cloned().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            let value = &patch_data[&key];
            // Convert toml::Value to serde_json::Value
            let json_val = toml_value_to_json(value)?;
            map.insert(key.clone(), json_val);
            applied_patches.insert(key.clone(), value.clone());
        }
    } else {
        anyhow::bail!("JSON file must have a top-level object to apply patches");
    }

    // Serialize back to pretty JSON
    let new_content = serde_json::to_string_pretty(&json_value)?;

    Ok((new_content, applied_patches))
}

/// Convert toml::Value to serde_json::Value.
pub(crate) fn toml_value_to_json(value: &toml::Value) -> anyhow::Result<serde_json::Value> {
    toml_value_to_json_with_depth(value, 0)
}

/// Convert toml::Value to serde_json::Value with recursion depth limit.
///
/// # Arguments
///
/// * `value` - The TOML value to convert
/// * `depth` - Current recursion depth
///
/// # Returns
///
/// The converted JSON value, or an error if depth limit is exceeded
fn toml_value_to_json_with_depth(
    value: &toml::Value,
    depth: usize,
) -> anyhow::Result<serde_json::Value> {
    const MAX_DEPTH: usize = 100;

    if depth > MAX_DEPTH {
        anyhow::bail!(
            "TOML value nesting exceeds maximum depth of {}. \
             This may indicate a malformed patch configuration.",
            MAX_DEPTH
        );
    }

    match value {
        toml::Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        toml::Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        toml::Value::Float(f) => {
            let num = serde_json::Number::from_f64(*f)
                .ok_or_else(|| anyhow::anyhow!("Invalid float value: {}", f))?;
            Ok(serde_json::Value::Number(num))
        }
        toml::Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        toml::Value::Array(arr) => {
            let json_arr: Result<Vec<_>, _> =
                arr.iter().map(|v| toml_value_to_json_with_depth(v, depth + 1)).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        toml::Value::Table(table) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in table {
                json_map.insert(k.clone(), toml_value_to_json_with_depth(v, depth + 1)?);
            }
            Ok(serde_json::Value::Object(json_map))
        }
        toml::Value::Datetime(dt) => Ok(serde_json::Value::String(dt.to_string())),
    }
}

/// Represents a conflict between project and private patches.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchConflict {
    /// Resource type (agents, snippets, etc.).
    pub resource_type: String,
    /// Manifest alias.
    pub alias: String,
    /// Field name that has conflicting values.
    pub field: String,
    /// Value from project-level patch.
    pub project_value: toml::Value,
    /// Value from private-level patch.
    pub private_value: toml::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_patches() {
        let patches = ManifestPatches::new();
        assert!(patches.is_empty());
        assert_eq!(patches.get("agents", "test"), None);
    }

    #[test]
    fn test_get_patch() {
        let mut patches = ManifestPatches::new();
        let mut patch_data = BTreeMap::new();
        patch_data.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        patches.agents.insert("test-agent".to_string(), patch_data.clone());

        assert!(!patches.is_empty());
        assert_eq!(patches.get("agents", "test-agent"), Some(&patch_data));
        assert_eq!(patches.get("agents", "other"), None);
        assert_eq!(patches.get("snippets", "test-agent"), None);
    }

    #[test]
    fn test_merge_no_conflict() {
        let mut base = ManifestPatches::new();
        let mut base_patch = BTreeMap::new();
        base_patch.insert("model".to_string(), toml::Value::String("claude-3-opus".to_string()));
        base.agents.insert("test".to_string(), base_patch);

        let mut overlay = ManifestPatches::new();
        let mut overlay_patch = BTreeMap::new();
        overlay_patch.insert("temperature".to_string(), toml::Value::String("0.7".to_string()));
        overlay.agents.insert("test".to_string(), overlay_patch);

        let (merged, conflicts) = base.merge_with(&overlay);
        assert!(conflicts.is_empty());
        assert_eq!(merged.agents.get("test").unwrap().len(), 2);
        assert_eq!(
            merged.agents.get("test").unwrap().get("model").unwrap(),
            &toml::Value::String("claude-3-opus".to_string())
        );
        assert_eq!(
            merged.agents.get("test").unwrap().get("temperature").unwrap(),
            &toml::Value::String("0.7".to_string())
        );
    }

    #[test]
    fn test_merge_with_conflict() {
        let mut base = ManifestPatches::new();
        let mut base_patch = BTreeMap::new();
        base_patch.insert("model".to_string(), toml::Value::String("claude-3-opus".to_string()));
        base.agents.insert("test".to_string(), base_patch);

        let mut overlay = ManifestPatches::new();
        let mut overlay_patch = BTreeMap::new();
        overlay_patch
            .insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        overlay.agents.insert("test".to_string(), overlay_patch);

        let (merged, conflicts) = base.merge_with(&overlay);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resource_type, "agents");
        assert_eq!(conflicts[0].alias, "test");
        assert_eq!(conflicts[0].field, "model");

        // Private (overlay) should win
        assert_eq!(
            merged.agents.get("test").unwrap().get("model").unwrap(),
            &toml::Value::String("claude-3-haiku".to_string())
        );
    }

    #[test]
    fn test_apply_patches_to_markdown_simple() {
        let content = r#"---
model: claude-3-opus
temperature: "0.5"
---
# Test Agent

This is a test agent.
"#;

        let mut patches = BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));

        let (new_content, applied) =
            apply_patches_to_content(content, "agent.md", &patches).unwrap();

        // Verify the patch was applied
        assert_eq!(applied.len(), 1);
        assert_eq!(
            applied.get("model").unwrap(),
            &toml::Value::String("claude-3-haiku".to_string())
        );

        // Verify the content contains the new model
        assert!(new_content.contains("model: claude-3-haiku"));
        assert!(new_content.contains("# Test Agent"));
    }

    #[test]
    fn test_apply_patches_to_markdown_multiple_fields() {
        let content = r#"---
model: claude-3-opus
temperature: "0.5"
---
# Test Agent
"#;

        let mut patches = BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        patches.insert("temperature".to_string(), toml::Value::String("0.7".to_string()));
        patches.insert("max_tokens".to_string(), toml::Value::Integer(2000));

        let (new_content, applied) =
            apply_patches_to_content(content, "agent.md", &patches).unwrap();

        // Verify all patches were applied
        assert_eq!(applied.len(), 3);
        assert!(new_content.contains("model: claude-3-haiku"));
        // Temperature can be serialized as "0.7" or 0.7 depending on YAML serializer
        assert!(new_content.contains("temperature:"));
        assert!(new_content.contains("0.7"));
        assert!(new_content.contains("max_tokens: 2000"));
    }

    #[test]
    fn test_apply_patches_to_markdown_create_frontmatter() {
        let content = "# Test Agent\n\nThis is a test agent without frontmatter.";

        let mut patches = BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        patches.insert("temperature".to_string(), toml::Value::String("0.7".to_string()));

        let (new_content, applied) =
            apply_patches_to_content(content, "agent.md", &patches).unwrap();

        // Verify patches were applied
        assert_eq!(applied.len(), 2);

        // Verify frontmatter was created
        assert!(new_content.starts_with("---\n"));
        assert!(new_content.contains("model: claude-3-haiku"));
        // Temperature can be serialized as "0.7" or 0.7 depending on YAML serializer
        assert!(new_content.contains("temperature:"));
        assert!(new_content.contains("0.7"));
        assert!(new_content.contains("# Test Agent"));
    }

    #[test]
    fn test_apply_patches_to_json_simple() {
        let content = r#"{
  "name": "test-server",
  "command": "npx",
  "args": ["server"]
}"#;

        let mut patches = BTreeMap::new();
        patches.insert("timeout".to_string(), toml::Value::Integer(300));

        let (new_content, applied) =
            apply_patches_to_content(content, "server.json", &patches).unwrap();

        // Verify patch was applied
        assert_eq!(applied.len(), 1);

        // Parse JSON to verify structure
        let json: serde_json::Value = serde_json::from_str(&new_content).unwrap();
        assert_eq!(json["timeout"], 300);
        assert_eq!(json["name"], "test-server");
        assert_eq!(json["command"], "npx");
    }

    #[test]
    fn test_apply_patches_to_json_nested() {
        let content = r#"{
  "name": "test-server",
  "config": {
    "host": "localhost"
  }
}"#;

        let mut patches = BTreeMap::new();

        // Add nested object
        let mut nested_table = toml::value::Table::new();
        nested_table.insert("port".to_string(), toml::Value::Integer(8080));
        nested_table.insert("ssl".to_string(), toml::Value::Boolean(true));
        patches.insert("server".to_string(), toml::Value::Table(nested_table));

        // Add array
        let array = vec![
            toml::Value::String("option1".to_string()),
            toml::Value::String("option2".to_string()),
        ];
        patches.insert("options".to_string(), toml::Value::Array(array));

        let (new_content, applied) =
            apply_patches_to_content(content, "server.json", &patches).unwrap();

        // Verify patches were applied
        assert_eq!(applied.len(), 2);

        // Parse JSON to verify structure
        let json: serde_json::Value = serde_json::from_str(&new_content).unwrap();
        assert_eq!(json["name"], "test-server");
        assert_eq!(json["server"]["port"], 8080);
        assert_eq!(json["server"]["ssl"], true);
        assert_eq!(json["options"][0], "option1");
        assert_eq!(json["options"][1], "option2");
    }

    #[test]
    fn test_apply_patches_to_content_empty_patches() {
        let content = r#"---
model: claude-3-opus
---
# Test Agent
"#;

        let patches = BTreeMap::new();

        let (new_content, applied) =
            apply_patches_to_content(content, "agent.md", &patches).unwrap();

        // Verify no patches were applied
        assert!(applied.is_empty());

        // Content should be unchanged
        assert_eq!(new_content, content);
    }

    #[test]
    fn test_apply_patches_to_content_unsupported_extension() {
        let content = "This is a text file.";

        let mut patches = BTreeMap::new();
        patches.insert("field".to_string(), toml::Value::String("value".to_string()));

        let (new_content, applied) =
            apply_patches_to_content(content, "file.txt", &patches).unwrap();

        // Verify no patches were applied (unsupported file type)
        assert!(applied.is_empty());

        // Content should be unchanged
        assert_eq!(new_content, content);
    }

    #[test]
    fn test_toml_value_to_json_conversions() {
        // Test string conversion
        let toml_str = toml::Value::String("test".to_string());
        let json_str = toml_value_to_json(&toml_str).unwrap();
        assert_eq!(json_str, serde_json::Value::String("test".to_string()));

        // Test integer conversion
        let toml_int = toml::Value::Integer(42);
        let json_int = toml_value_to_json(&toml_int).unwrap();
        assert_eq!(json_int, serde_json::json!(42));

        // Test float conversion
        let toml_float = toml::Value::Float(2.5);
        let json_float = toml_value_to_json(&toml_float).unwrap();
        assert_eq!(json_float, serde_json::json!(2.5));

        // Test boolean conversion
        let toml_bool = toml::Value::Boolean(true);
        let json_bool = toml_value_to_json(&toml_bool).unwrap();
        assert_eq!(json_bool, serde_json::Value::Bool(true));

        // Test array conversion
        let toml_array =
            toml::Value::Array(vec![toml::Value::String("a".to_string()), toml::Value::Integer(1)]);
        let json_array = toml_value_to_json(&toml_array).unwrap();
        assert_eq!(json_array, serde_json::json!(["a", 1]));

        // Test table (object) conversion
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), toml::Value::String("value".to_string()));
        table.insert("num".to_string(), toml::Value::Integer(123));
        let toml_table = toml::Value::Table(table);
        let json_table = toml_value_to_json(&toml_table).unwrap();
        assert_eq!(json_table, serde_json::json!({"key": "value", "num": 123}));

        // Test datetime conversion
        let datetime_str = "2025-01-01T12:00:00Z";
        let toml_datetime = toml::Value::Datetime(datetime_str.parse().unwrap());
        let json_datetime = toml_value_to_json(&toml_datetime).unwrap();
        assert_eq!(json_datetime, serde_json::Value::String(datetime_str.to_string()));
    }

    #[test]
    fn test_get_for_resource_type() {
        let mut patches = ManifestPatches::new();
        let mut agent_patch = BTreeMap::new();
        agent_patch.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        patches.agents.insert("test-agent".to_string(), agent_patch.clone());

        let mut snippet_patch = BTreeMap::new();
        snippet_patch.insert("lang".to_string(), toml::Value::String("rust".to_string()));
        patches.snippets.insert("test-snippet".to_string(), snippet_patch.clone());

        // Test getting valid resource types
        assert_eq!(patches.get_for_resource_type("agents").unwrap().len(), 1);
        assert_eq!(patches.get_for_resource_type("snippets").unwrap().len(), 1);
        assert_eq!(patches.get_for_resource_type("commands").unwrap().len(), 0);

        // Test invalid resource type
        assert!(patches.get_for_resource_type("invalid").is_none());
    }

    #[test]
    fn test_patch_origin_serialization() {
        let project = PatchOrigin::Project;
        let private = PatchOrigin::Private;

        // Test serialization
        let project_str = serde_json::to_string(&project).unwrap();
        let private_str = serde_json::to_string(&private).unwrap();

        assert_eq!(project_str, r#""project""#);
        assert_eq!(private_str, r#""private""#);

        // Test deserialization
        let project_de: PatchOrigin = serde_json::from_str(&project_str).unwrap();
        let private_de: PatchOrigin = serde_json::from_str(&private_str).unwrap();

        assert_eq!(project_de, PatchOrigin::Project);
        assert_eq!(private_de, PatchOrigin::Private);
    }

    #[test]
    fn test_merge_different_resource_types() {
        let mut base = ManifestPatches::new();
        let mut base_agent_patch = BTreeMap::new();
        base_agent_patch
            .insert("model".to_string(), toml::Value::String("claude-3-opus".to_string()));
        base.agents.insert("test".to_string(), base_agent_patch);

        let mut overlay = ManifestPatches::new();
        let mut overlay_snippet_patch = BTreeMap::new();
        overlay_snippet_patch.insert("lang".to_string(), toml::Value::String("rust".to_string()));
        overlay.snippets.insert("test".to_string(), overlay_snippet_patch);

        let (merged, conflicts) = base.merge_with(&overlay);

        // No conflicts since they're different resource types
        assert!(conflicts.is_empty());
        assert_eq!(merged.agents.len(), 1);
        assert_eq!(merged.snippets.len(), 1);
    }

    #[test]
    fn test_merge_adds_new_aliases() {
        let mut base = ManifestPatches::new();
        let mut base_patch = BTreeMap::new();
        base_patch.insert("model".to_string(), toml::Value::String("claude-3-opus".to_string()));
        base.agents.insert("agent1".to_string(), base_patch);

        let mut overlay = ManifestPatches::new();
        let mut overlay_patch = BTreeMap::new();
        overlay_patch
            .insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        overlay.agents.insert("agent2".to_string(), overlay_patch);

        let (merged, conflicts) = base.merge_with(&overlay);

        // No conflicts since they're different aliases
        assert!(conflicts.is_empty());
        assert_eq!(merged.agents.len(), 2);
        assert!(merged.agents.contains_key("agent1"));
        assert!(merged.agents.contains_key("agent2"));
    }

    #[test]
    fn test_apply_patches_preserves_markdown_body() {
        let content = r#"---
model: claude-3-opus
---
# Test Agent

This is the agent body with **markdown** formatting.

- Item 1
- Item 2

```rust
fn main() {
    println!("Hello");
}
```
"#;

        let mut patches = BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));

        let (new_content, _) = apply_patches_to_content(content, "agent.md", &patches).unwrap();

        // Verify body is preserved
        assert!(new_content.contains("# Test Agent"));
        assert!(new_content.contains("This is the agent body"));
        assert!(new_content.contains("**markdown**"));
        assert!(new_content.contains("- Item 1"));
        assert!(new_content.contains("```rust"));
        assert!(new_content.contains("fn main()"));
    }

    #[test]
    fn test_json_patch_requires_object() {
        let content = r#"["array", "of", "strings"]"#;

        let mut patches = BTreeMap::new();
        patches.insert("field".to_string(), toml::Value::String("value".to_string()));

        let result = apply_patches_to_json(content, &patches);

        // Should fail because JSON is not an object
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("top-level object"));
    }

    #[test]
    fn test_merge_multiple_conflicts() {
        let mut base = ManifestPatches::new();
        let mut base_patch = BTreeMap::new();
        base_patch.insert("model".to_string(), toml::Value::String("claude-3-opus".to_string()));
        base_patch.insert("temperature".to_string(), toml::Value::String("0.5".to_string()));
        base.agents.insert("test".to_string(), base_patch);

        let mut overlay = ManifestPatches::new();
        let mut overlay_patch = BTreeMap::new();
        overlay_patch
            .insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        overlay_patch.insert("temperature".to_string(), toml::Value::String("0.7".to_string()));
        overlay.agents.insert("test".to_string(), overlay_patch);

        let (merged, conflicts) = base.merge_with(&overlay);

        // Should have 2 conflicts
        assert_eq!(conflicts.len(), 2);

        // Check both conflicts are present
        let model_conflict = conflicts.iter().find(|c| c.field == "model").unwrap();
        assert_eq!(model_conflict.project_value, toml::Value::String("claude-3-opus".to_string()));
        assert_eq!(model_conflict.private_value, toml::Value::String("claude-3-haiku".to_string()));

        let temp_conflict = conflicts.iter().find(|c| c.field == "temperature").unwrap();
        assert_eq!(temp_conflict.project_value, toml::Value::String("0.5".to_string()));
        assert_eq!(temp_conflict.private_value, toml::Value::String("0.7".to_string()));

        // Private values should win
        assert_eq!(
            merged.agents.get("test").unwrap().get("model").unwrap(),
            &toml::Value::String("claude-3-haiku".to_string())
        );
        assert_eq!(
            merged.agents.get("test").unwrap().get("temperature").unwrap(),
            &toml::Value::String("0.7".to_string())
        );
    }

    #[test]
    fn test_applied_patches_struct() {
        let applied = AppliedPatches {
            project: BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]),
            private: BTreeMap::from([(
                "temperature".to_string(),
                toml::Value::String("0.9".into()),
            )]),
        };

        assert!(!applied.is_empty());
        assert_eq!(applied.total_count(), 2);
        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 1);

        let empty = AppliedPatches::new();
        assert!(empty.is_empty());
        assert_eq!(empty.total_count(), 0);
    }

    #[test]
    fn test_apply_patches_with_origin_separates_project_and_private() {
        let content = "---\nmodel: gpt-4\n---\n# Test\n";
        let project = BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]);
        let private =
            BTreeMap::from([("temperature".to_string(), toml::Value::String("0.9".into()))]);

        let (result, applied) =
            apply_patches_to_content_with_origin(content, "test.md", &project, &private).unwrap();

        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 1);
        assert!(result.contains("model: haiku"));
        assert!(result.contains("temperature:"));
        assert!(result.contains("0.9"));
    }

    #[test]
    fn test_apply_patches_with_origin_empty_patches() {
        let content = "---\nmodel: gpt-4\n---\n# Test\n";
        let project = BTreeMap::new();
        let private = BTreeMap::new();

        let (result, applied) =
            apply_patches_to_content_with_origin(content, "test.md", &project, &private).unwrap();

        assert!(applied.is_empty());
        assert_eq!(result, content);
    }

    #[test]
    fn test_apply_patches_with_origin_only_project() {
        let content = "---\nmodel: gpt-4\n---\n# Test\n";
        let project = BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]);
        let private = BTreeMap::new();

        let (result, applied) =
            apply_patches_to_content_with_origin(content, "test.md", &project, &private).unwrap();

        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 0);
        assert!(result.contains("model: haiku"));
    }

    #[test]
    fn test_apply_patches_with_origin_only_private() {
        let content = "---\nmodel: gpt-4\n---\n# Test\n";
        let project = BTreeMap::new();
        let private =
            BTreeMap::from([("temperature".to_string(), toml::Value::String("0.9".into()))]);

        let (result, applied) =
            apply_patches_to_content_with_origin(content, "test.md", &project, &private).unwrap();

        assert_eq!(applied.project.len(), 0);
        assert_eq!(applied.private.len(), 1);
        assert!(result.contains("temperature:"));
        assert!(result.contains("0.9"));
    }

    #[test]
    fn test_apply_patches_with_origin_private_overrides_project() {
        let content = "---\nmodel: gpt-4\n---\n# Test\n";
        let project = BTreeMap::from([("model".to_string(), toml::Value::String("haiku".into()))]);
        let private = BTreeMap::from([("model".to_string(), toml::Value::String("sonnet".into()))]);

        let (result, applied) =
            apply_patches_to_content_with_origin(content, "test.md", &project, &private).unwrap();

        // Both patches are tracked separately
        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 1);

        // But private wins in the final content
        assert!(result.contains("model: sonnet"));
        assert!(!result.contains("model: haiku"));
    }

    #[test]
    fn test_manifest_patches_deserialization() {
        // Test that patches can be deserialized from TOML correctly
        let toml_str = r#"
[agents.all-helpers]
model = "claude-3-haiku"
max_tokens = "4096"
category = "utility"
"#;

        let patches: ManifestPatches = toml::from_str(toml_str).unwrap();
        println!("Deserialized patches: {:?}", patches);
        println!("Agents: {:?}", patches.agents);

        // Verify we can get the patches
        let agent_patches = patches.get("agents", "all-helpers");
        println!("Got patches: {:?}", agent_patches);
        assert!(agent_patches.is_some(), "Should find patches for 'all-helpers'");

        let patch_data = agent_patches.unwrap();
        assert_eq!(patch_data.len(), 3, "Should have 3 patch fields");
        assert_eq!(patch_data.get("model").unwrap().as_str().unwrap(), "claude-3-haiku");
        assert_eq!(patch_data.get("max_tokens").unwrap().as_str().unwrap(), "4096");
        assert_eq!(patch_data.get("category").unwrap().as_str().unwrap(), "utility");
    }

    #[test]
    fn test_full_manifest_with_patches() {
        // Test that patches work correctly when embedded in a full Manifest
        let toml_str = r#"
[sources]
test = "https://example.com/repo.git"

[agents]
all-helpers = { source = "test", path = "agents/helpers/*.md", version = "v1.0.0" }

[patch.agents.all-helpers]
model = "claude-3-haiku"
max_tokens = "4096"
"#;

        let manifest: crate::manifest::Manifest = toml::from_str(toml_str).unwrap();
        println!("Manifest patches: {:?}", manifest.patches);
        println!("Agents patches: {:?}", manifest.patches.agents);

        // Verify we can get the patches using the get method
        let agent_patches = manifest.patches.get("agents", "all-helpers");
        println!("Got patches: {:?}", agent_patches);
        assert!(agent_patches.is_some(), "Should find patches for 'all-helpers' in full manifest");

        let patch_data = agent_patches.unwrap();
        assert_eq!(patch_data.len(), 2, "Should have 2 patch fields");
        assert_eq!(patch_data.get("model").unwrap().as_str().unwrap(), "claude-3-haiku");
    }
}
