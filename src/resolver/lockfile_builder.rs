//! Lockfile building and management functionality.
//!
//! This module handles the creation, updating, and maintenance of lockfile entries,
//! including conflict detection, stale entry removal, and transitive dependency management.

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource, lockfile_dependency_ref::LockfileDependencyRef};
use crate::manifest::{Manifest, ResourceDependency};
use crate::resolver::types as dependency_helpers;
use anyhow::Result;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

// Type aliases for internal lookups
type ResourceKey = (ResourceType, String, Option<String>);
type ResourceInfo = (Option<String>, Option<String>);

/// Checks if two lockfile entries should be considered duplicates.
///
/// Two entries are duplicates if they have the same:
/// 1. name, source, tool, AND template_vars (standard deduplication)
/// 2. path and tool for local dependencies (source = None)
///
/// **CRITICAL**: template_vars are part of the resource identity! Resources with
/// different template_vars are DISTINCT resources that must all exist in the lockfile.
/// For example, `backend-engineer` with `language=typescript` and `language=javascript`
/// are TWO DIFFERENT resources.
///
/// The second case handles situations where a direct dependency and a transitive
/// dependency point to the same local file but have different names (e.g., manifest
/// name vs path-based name). This prevents false conflicts.
pub fn is_duplicate_entry(existing: &LockedResource, new_entry: &LockedResource) -> bool {
    tracing::info!(
        "is_duplicate_entry: existing.name='{}', new.name='{}', existing.manifest_alias={:?}, new.manifest_alias={:?}, existing.path='{}', new.path='{}'",
        existing.name,
        new_entry.name,
        existing.manifest_alias,
        new_entry.manifest_alias,
        existing.path,
        new_entry.path
    );

    // CRITICAL: manifest_alias is part of resource identity for PATTERN-EXPANDED dependencies!
    // When BOTH entries have manifest_alias (both are direct or pattern-expanded),
    // different aliases mean different resources, even with identical paths/variant_inputs.
    //
    // Example case where we should NOT deduplicate:
    //   - backend-engineer (manifest_alias = Some("backend-engineer"))
    //   - backend-engineer-python (manifest_alias = Some("backend-engineer-python"))
    // Both from same path but represent distinct manifest entries.
    //
    // Example case where we SHOULD deduplicate (let merge strategy decide which wins):
    //   - general-purpose (manifest_alias = Some("general-purpose"), direct from manifest)
    //   - general-purpose (manifest_alias = None, transitive dependency)
    // One is direct, one is transitive - merge strategy will pick the direct one.
    if existing.manifest_alias.is_some()
        && new_entry.manifest_alias.is_some()
        && existing.manifest_alias != new_entry.manifest_alias
    {
        tracing::debug!(
            "NOT duplicates - both are direct/pattern deps with different manifest_alias: existing={:?} vs new={:?} (path={})",
            existing.manifest_alias,
            new_entry.manifest_alias,
            existing.path
        );
        return false; // Different direct dependencies = NOT duplicates
    }

    // Determine if one is direct and one is transitive
    let existing_is_direct = existing.manifest_alias.is_some();
    let new_is_direct = new_entry.manifest_alias.is_some();
    let one_direct_one_transitive = existing_is_direct != new_is_direct;

    // Standard deduplication logic:
    // variant_inputs are ALWAYS part of resource identity - resources with different
    // template_vars are distinct resources that must both exist in the lockfile.
    // This applies regardless of whether dependencies are direct, transitive, or mixed.
    let basic_match = existing.name == new_entry.name
        && existing.source == new_entry.source
        && existing.tool == new_entry.tool;

    let is_duplicate = basic_match && existing.variant_inputs == new_entry.variant_inputs;

    if is_duplicate {
        tracing::debug!(
            "Deduplicating entries: name={}, source={:?}, tool={:?}, manifest_alias existing={:?} new={:?}, one_direct_one_transitive={}",
            existing.name,
            existing.source,
            existing.tool,
            existing.manifest_alias,
            new_entry.manifest_alias,
            one_direct_one_transitive
        );
        return true;
    }

    // Local dependency deduplication: same path and tool
    // Apply same logic as above: variant_inputs are ALWAYS part of resource identity
    if existing.source.is_none() && new_entry.source.is_none() {
        let path_tool_match = existing.path == new_entry.path && existing.tool == new_entry.tool;
        let is_local_duplicate =
            path_tool_match && existing.variant_inputs == new_entry.variant_inputs;

        if is_local_duplicate {
            tracing::debug!(
                "Deduplicating local deps: path={}, tool={:?}, one_direct_one_transitive={}",
                existing.path,
                existing.tool,
                one_direct_one_transitive
            );
            return true;
        }
    }

    tracing::debug!(
        "NOT duplicates: name existing={} new={}, source existing={:?} new={:?}, variant_inputs match={}",
        existing.name,
        new_entry.name,
        existing.source,
        new_entry.source,
        existing.variant_inputs == new_entry.variant_inputs
    );
    false
}

/// Determines if a new lockfile entry should replace an existing duplicate entry.
///
/// Uses a deterministic merge strategy to ensure consistent lockfile generation
/// regardless of processing order (e.g., HashMap iteration order).
///
/// # Merge Priority Rules (highest to lowest)
///
/// 1. **Manifest dependencies win** - Direct manifest dependencies (with `manifest_alias`)
///    always take precedence over transitive dependencies
/// 2. **install=true wins** - Dependencies that create files (`install=true`) are
///    preferred over content-only dependencies (`install=false`)
/// 3. **First wins** - If both have equal priority, keep the existing entry
///
/// This ensures that the lockfile is deterministic even when:
/// - Dependencies are processed in different orders
/// - HashMap iteration order varies between runs
/// - Multiple parents declare the same transitive dependency with different settings
///
/// # Arguments
///
/// * `existing` - The current entry in the lockfile
/// * `new_entry` - The new entry being added
///
/// # Returns
///
/// `true` if the new entry should replace the existing one, `false` otherwise
fn should_replace_duplicate(existing: &LockedResource, new_entry: &LockedResource) -> bool {
    let is_new_manifest = new_entry.manifest_alias.is_some();
    let is_existing_manifest = existing.manifest_alias.is_some();
    let new_install = new_entry.install.unwrap_or(true);
    let existing_install = existing.install.unwrap_or(true);

    let should_replace = if is_new_manifest != is_existing_manifest {
        // Rule 1: Manifest dependencies always win
        is_new_manifest
    } else if new_install != existing_install {
        // Rule 2: Prefer install=true (files that should be written)
        new_install
    } else {
        // Rule 3: Both have same priority, but still replace if new is manifest
        // to ensure direct dependencies override transitive ones
        is_new_manifest
    };

    if new_install != existing_install {
        tracing::debug!(
            "Merge decision for {}: existing.install={:?}, new.install={:?}, should_replace={}",
            new_entry.name,
            existing.install,
            new_entry.install,
            should_replace
        );
    }

    should_replace
}

/// Manages lockfile operations including entry creation, updates, and cleanup.
pub struct LockfileBuilder<'a> {
    manifest: &'a Manifest,
}

impl<'a> LockfileBuilder<'a> {
    /// Create a new lockfile builder with the given manifest.
    pub fn new(manifest: &'a Manifest) -> Self {
        Self {
            manifest,
        }
    }

    /// Add or update a lockfile entry with deterministic merging for duplicates.
    ///
    /// This method handles deduplication by using (name, source, tool) tuples as the unique key.
    /// When duplicates are found, it uses a deterministic merge strategy to ensure consistent
    /// lockfile generation across runs, regardless of processing order.
    ///
    /// # Merge Strategy (deterministic, order-independent)
    ///
    /// When merging duplicate entries:
    /// 1. **Prefer direct manifest dependencies** (has `manifest_alias`) over transitive dependencies
    /// 2. **Prefer install=true** over install=false (prefer dependencies that create files)
    /// 3. Otherwise, keep the existing entry (first-wins for same priority)
    ///
    /// This ensures that even with non-deterministic HashMap iteration order, the same
    /// logical dependency structure produces the same lockfile.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to update
    /// * `name` - The name of the dependency (for documentation purposes)
    /// * `entry` - The locked resource entry to add or update
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut lockfile = LockFile::new();
    /// let entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     source: Some("community".to_string()),
    ///     tool: "claude-code".to_string(),
    ///     // ... other fields
    /// };
    ///
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", entry);
    ///
    /// // Later updates use deterministic merge strategy
    /// let updated_entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     source: Some("community".to_string()),
    ///     tool: "claude-code".to_string(),
    ///     // ... updated fields
    /// };
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", updated_entry);
    /// ```
    pub fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        let resources = lockfile.get_resources_mut(&entry.resource_type);

        if let Some(existing) = resources.iter_mut().find(|e| is_duplicate_entry(e, &entry)) {
            // Use deterministic merge strategy to ensure consistent lockfile generation
            let should_replace = should_replace_duplicate(existing, &entry);

            tracing::trace!(
                "Duplicate entry for {}: existing.install={:?}, new.install={:?}, should_replace={}",
                entry.name,
                existing.install,
                entry.install,
                should_replace
            );

            if should_replace {
                *existing = entry;
            }
            // Otherwise keep existing entry (deterministic: first-wins for same priority)
        } else {
            resources.push(entry);
        }
    }

    /// Removes stale lockfile entries that are no longer in the manifest.
    ///
    /// This method removes lockfile entries for direct manifest dependencies that have been
    /// commented out or removed from the manifest. This must be called BEFORE
    /// `remove_manifest_entries_for_update()` to ensure stale entries don't cause conflicts
    /// during resolution.
    ///
    /// A manifest-level entry is identified by:
    /// - `manifest_alias.is_none()` - Direct dependency with no pattern expansion
    /// - `manifest_alias.is_some()` - Pattern-expanded dependency (alias must be in manifest)
    ///
    /// For each stale entry found, this also removes its transitive children to maintain
    /// lockfile consistency.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to clean
    ///
    /// # Examples
    ///
    /// If a user comments out an agent in agpm.toml:
    /// ```toml
    /// # [agents]
    /// # example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
    /// ```
    ///
    /// This function will remove the "example" agent from the lockfile and all its transitive
    /// dependencies before the update process begins.
    pub fn remove_stale_manifest_entries(&self, lockfile: &mut LockFile) {
        // Collect all current manifest keys for each resource type
        let manifest_agents: HashSet<String> =
            self.manifest.agents.keys().map(|k| k.to_string()).collect();
        let manifest_snippets: HashSet<String> =
            self.manifest.snippets.keys().map(|k| k.to_string()).collect();
        let manifest_commands: HashSet<String> =
            self.manifest.commands.keys().map(|k| k.to_string()).collect();
        let manifest_scripts: HashSet<String> =
            self.manifest.scripts.keys().map(|k| k.to_string()).collect();
        let manifest_hooks: HashSet<String> =
            self.manifest.hooks.keys().map(|k| k.to_string()).collect();
        let manifest_mcp_servers: HashSet<String> =
            self.manifest.mcp_servers.keys().map(|k| k.to_string()).collect();
        let manifest_skills: HashSet<String> =
            self.manifest.skills.keys().map(|k| k.to_string()).collect();

        // Helper to get the right manifest keys for a resource type
        let get_manifest_keys = |resource_type: ResourceType| match resource_type {
            ResourceType::Agent => &manifest_agents,
            ResourceType::Snippet => &manifest_snippets,
            ResourceType::Command => &manifest_commands,
            ResourceType::Script => &manifest_scripts,
            ResourceType::Hook => &manifest_hooks,
            ResourceType::McpServer => &manifest_mcp_servers,
            ResourceType::Skill => &manifest_skills,
        };

        // Collect (name, source) pairs to remove
        let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();
        let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

        // Find all manifest-level entries that are no longer in the manifest
        for resource_type in ResourceType::all() {
            let manifest_keys = get_manifest_keys(*resource_type);
            let resources = lockfile.get_resources(resource_type);

            for entry in resources {
                // Determine if this is a stale manifest-level entry (no longer in manifest)
                let is_stale = if let Some(ref alias) = entry.manifest_alias {
                    // Pattern-expanded entry: stale if alias is NOT in manifest
                    !manifest_keys.contains(alias)
                } else {
                    // Direct entry: stale if name is NOT in manifest
                    !manifest_keys.contains(&entry.name)
                };

                if is_stale {
                    let key = (entry.name.clone(), entry.source.clone());
                    entries_to_remove.insert(key.clone());
                    direct_entries.push(key);
                }
            }
        }

        // For each stale entry, recursively collect its transitive children
        for (parent_name, parent_source) in direct_entries {
            for resource_type in ResourceType::all() {
                if let Some(parent_entry) = lockfile
                    .get_resources(resource_type)
                    .iter()
                    .find(|e| e.name == parent_name && e.source == parent_source)
                {
                    Self::collect_transitive_children(
                        lockfile,
                        parent_entry,
                        &mut entries_to_remove,
                    );
                }
            }
        }

        // Remove all marked entries
        let should_remove = |entry: &LockedResource| {
            entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
        };

        lockfile.agents.retain(|entry| !should_remove(entry));
        lockfile.snippets.retain(|entry| !should_remove(entry));
        lockfile.commands.retain(|entry| !should_remove(entry));
        lockfile.scripts.retain(|entry| !should_remove(entry));
        lockfile.hooks.retain(|entry| !should_remove(entry));
        lockfile.mcp_servers.retain(|entry| !should_remove(entry));
    }

    /// Removes lockfile entries for manifest dependencies that will be re-resolved.
    ///
    /// This method removes old entries for direct manifest dependencies before updating,
    /// which handles the case where a dependency's source or resource type changes.
    /// This prevents duplicate entries with the same name but different sources.
    ///
    /// Pattern-expanded and transitive dependencies are preserved because:
    /// - Pattern expansions will be re-added during resolution with (name, source) matching
    /// - Transitive dependencies aren't manifest keys and won't be removed
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to clean
    /// * `manifest_keys` - Set of manifest dependency keys being updated
    pub fn remove_manifest_entries_for_update(
        &self,
        lockfile: &mut LockFile,
        manifest_keys: &HashSet<String>,
    ) {
        // Collect (name, source) pairs to remove
        // We use (name, source) tuples to distinguish same-named resources from different sources
        let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();

        // Step 1: Find direct manifest entries and collect them for transitive traversal
        let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

        for resource_type in ResourceType::all() {
            let resources = lockfile.get_resources(resource_type);
            for entry in resources {
                // Check if this entry originates from a manifest key being updated
                if manifest_keys.contains(&entry.name)
                    || entry
                        .manifest_alias
                        .as_ref()
                        .is_some_and(|alias| manifest_keys.contains(alias))
                {
                    let key = (entry.name.clone(), entry.source.clone());
                    entries_to_remove.insert(key.clone());
                    direct_entries.push(key);
                }
            }
        }

        // Step 2: For each direct entry, recursively collect its transitive children
        // This ensures that when "agent-A" changes from repo1 to repo2, we also remove
        // all transitive dependencies that came from repo1 via agent-A
        for (parent_name, parent_source) in direct_entries {
            // Find the parent entry in the lockfile
            for resource_type in ResourceType::all() {
                if let Some(parent_entry) = lockfile
                    .get_resources(resource_type)
                    .iter()
                    .find(|e| e.name == parent_name && e.source == parent_source)
                {
                    // Walk its dependency tree
                    Self::collect_transitive_children(
                        lockfile,
                        parent_entry,
                        &mut entries_to_remove,
                    );
                }
            }
        }

        // Step 3: Remove all marked entries
        let should_remove = |entry: &LockedResource| {
            entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
        };

        lockfile.agents.retain(|entry| !should_remove(entry));
        lockfile.snippets.retain(|entry| !should_remove(entry));
        lockfile.commands.retain(|entry| !should_remove(entry));
        lockfile.scripts.retain(|entry| !should_remove(entry));
        lockfile.hooks.retain(|entry| !should_remove(entry));
        lockfile.mcp_servers.retain(|entry| !should_remove(entry));
    }

    /// Recursively collect all transitive children of a lockfile entry.
    ///
    /// This walks the dependency graph starting from `parent`, following the `dependencies`
    /// field to find all resources that transitively depend on the parent. Only dependencies
    /// with the same source as the parent are collected (to avoid removing unrelated resources).
    ///
    /// The `dependencies` field contains strings in the format:
    /// - `"resource_type/name"` for dependencies from the same source
    /// - `"source:resource_type/name:version"` for explicit source references
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to search for dependencies
    /// * `parent` - The parent entry whose children we want to collect
    /// * `entries_to_remove` - Set of (name, source) pairs to populate with found children
    fn collect_transitive_children(
        lockfile: &LockFile,
        parent: &LockedResource,
        entries_to_remove: &mut HashSet<(String, Option<String>)>,
    ) {
        // For each dependency declared by this parent
        for dep_ref in parent.parsed_dependencies() {
            let dep_path = &dep_ref.path;
            let resource_type = dep_ref.resource_type;

            // Extract the resource name from the path (filename without extension)
            let dep_name = dependency_helpers::extract_filename_from_path(dep_path)
                .unwrap_or_else(|| dep_path.to_string());

            // Determine the source: use explicit source from dep_ref if present, otherwise inherit from parent
            let dep_source = dep_ref.source.or_else(|| parent.source.clone());

            // Find the dependency entry with matching name and source
            if let Some(dep_entry) = lockfile
                .get_resources(&resource_type)
                .iter()
                .find(|e| e.name == dep_name && e.source == dep_source)
            {
                let key = (dep_entry.name.clone(), dep_entry.source.clone());

                // Add to removal set and recurse (if not already processed)
                if !entries_to_remove.contains(&key) {
                    entries_to_remove.insert(key);
                    // Recursively collect this dependency's children
                    Self::collect_transitive_children(lockfile, dep_entry, entries_to_remove);
                }
            }
        }
    }
}

/// Adds pattern-expanded entries to the lockfile with deterministic deduplication.
///
/// This function adds multiple resolved entries from a pattern dependency to the
/// appropriate resource type collection in the lockfile. When duplicates are found,
/// it uses the same deterministic merge strategy as `add_or_update_lockfile_entry`
/// to ensure consistent lockfile generation.
///
/// # Arguments
///
/// * `lockfile` - The mutable lockfile to update
/// * `entries` - Vector of resolved resources from pattern expansion
/// * `resource_type` - The type of resource being added
///
/// # Deduplication
///
/// Uses deterministic merge strategy:
/// 1. Prefer manifest dependencies over transitive dependencies
/// 2. Prefer install=true over install=false
/// 3. Otherwise keep existing entry
pub fn add_pattern_entries(
    lockfile: &mut LockFile,
    entries: Vec<LockedResource>,
    resource_type: ResourceType,
) {
    let resources = lockfile.get_resources_mut(&resource_type);

    for entry in entries {
        if let Some(existing) = resources.iter_mut().find(|e| is_duplicate_entry(e, &entry)) {
            // Use deterministic merge strategy to ensure consistent lockfile generation
            if should_replace_duplicate(existing, &entry) {
                *existing = entry;
            }
        } else {
            resources.push(entry);
        }
    }
}

/// Rewrites a dependency string to include version information.
///
/// This helper function transforms dependency strings by looking up version information
/// in the provided maps and updating the dependency reference accordingly.
///
/// # Arguments
///
/// * `dep` - The original dependency string (must be properly formatted)
/// * `lookup_map` - Map of (resource_type, path, source) -> name for resolving dependencies
/// * `resource_info_map` - Map of (resource_type, name, source) -> (source, version) for version info
/// * `parent_source` - The source of the parent resource (for inheritance)
///
/// # Returns
///
/// The updated dependency string with version information included, or the original
/// dependency string if it cannot be parsed or no version info is found
fn rewrite_dependency_string(
    dep: &str,
    lookup_map: &HashMap<(ResourceType, String, Option<String>), String>,
    resource_info_map: &HashMap<ResourceKey, ResourceInfo>,
    parent_source: Option<String>,
) -> String {
    // Parse dependency using DependencyReference - only support properly formatted dependencies
    if let Ok(existing_dep) = LockfileDependencyRef::from_str(dep) {
        // If it's already a properly formatted dependency, try to add version info if missing
        let dep_source = existing_dep.source.clone().or_else(|| parent_source.clone());
        let dep_resource_type = existing_dep.resource_type;
        let dep_path = existing_dep.path.clone();

        // Look up resource in same source
        if let Some(dep_name) = lookup_map.get(&(
            dep_resource_type,
            dependency_helpers::normalize_lookup_path(&dep_path),
            dep_source.clone(),
        )) {
            if let Some((_source, Some(ver))) =
                resource_info_map.get(&(dep_resource_type, dep_name.clone(), dep_source.clone()))
            {
                // Create updated dependency reference with version
                return LockfileDependencyRef::git(
                    dep_source.clone().unwrap_or_default(),
                    dep_resource_type,
                    dep_path,
                    Some(ver.clone()),
                )
                .to_string();
            }
        }

        // Return as-is if no version info found
        existing_dep.to_string()
    } else {
        // Return as-is if parsing fails
        dep.to_string()
    }
}

// ============================================================================
// Lockfile Helper Operations
// ============================================================================

/// Helper to generate a unique key for grouping dependencies.
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn group_key(source: &str, version: &str) -> String {
    format!("{source}::{version}")
}

/// Get patches for a specific resource from the manifest.
///
/// Looks up patches defined in `[patch.<resource_type>.<alias>]` sections
/// and returns them as a HashMap ready for inclusion in the lockfile.
///
/// For pattern-expanded resources, the manifest_alias should be provided to ensure
/// patches are looked up using the original pattern name rather than the concrete
/// resource name.
///
/// # Arguments
///
/// * `manifest` - Reference to the project manifest containing patches
/// * `resource_type` - Type of the resource (agent, snippet, command, etc.)
/// * `name` - Resource name to look up patches for
/// * `manifest_alias` - Optional manifest alias for pattern-expanded resources
///
/// # Returns
///
/// BTreeMap of patch key-value pairs, or empty BTreeMap if no patches defined
pub(super) fn get_patches_for_resource(
    manifest: &Manifest,
    resource_type: ResourceType,
    name: &str,
    manifest_alias: Option<&str>,
) -> BTreeMap<String, toml::Value> {
    // Use manifest_alias for pattern-expanded resources, name for regular resources
    let lookup_name = manifest_alias.unwrap_or(name);

    let patches = match resource_type {
        ResourceType::Agent => &manifest.patches.agents,
        ResourceType::Snippet => &manifest.patches.snippets,
        ResourceType::Command => &manifest.patches.commands,
        ResourceType::Script => &manifest.patches.scripts,
        ResourceType::Hook => &manifest.patches.hooks,
        ResourceType::McpServer => &manifest.patches.mcp_servers,
        ResourceType::Skill => &manifest.patches.skills,
    };

    patches.get(lookup_name).cloned().unwrap_or_default()
}

/// Build the complete merged template variable context for a dependency.
///
/// This creates the full variant_inputs that should be stored in the lockfile,
/// combining both the global project configuration and any dependency-specific
/// variant_inputs overrides.
///
/// This ensures lockfile entries contain the exact template context that was
/// used during dependency resolution, enabling reproducible builds.
///
/// # Arguments
///
/// * `manifest` - Reference to the project manifest containing global project config
/// * `dep` - The dependency to build variant_inputs for
///
/// # Returns
///
/// Complete merged variant_inputs (always returns a Value, empty if no variables)
pub(super) fn build_merged_variant_inputs(
    manifest: &Manifest,
    dep: &ResourceDependency,
) -> serde_json::Value {
    use crate::templating::deep_merge_json;

    // Start with dependency-level template_vars (if any)
    let dep_vars = dep.get_template_vars();

    tracing::debug!(
        "[DEBUG] build_merged_variant_inputs: dep_path='{}', has_dep_vars={}, dep_vars={:?}",
        dep.get_path(),
        dep_vars.is_some(),
        dep_vars
    );

    // Get global project config as JSON
    let global_project = manifest
        .project
        .as_ref()
        .map(|p| p.to_json_value())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    tracing::debug!("[DEBUG] build_merged_variant_inputs: global_project={:?}", global_project);

    // Build complete context
    let mut merged_map = serde_json::Map::new();

    // If dependency has template_vars, start with those
    if let Some(vars) = dep_vars {
        if let Some(obj) = vars.as_object() {
            merged_map.extend(obj.clone());
        }
    }

    // Extract project overrides from dependency template_vars (if present)
    let project_overrides = dep_vars
        .and_then(|v| v.get("project").cloned())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Deep merge global project config with dependency-specific overrides
    let merged_project = deep_merge_json(global_project, &project_overrides);

    // Add merged project config to the template_vars only if it's not empty
    if let Some(project_obj) = merged_project.as_object() {
        if !project_obj.is_empty() {
            merged_map.insert("project".to_string(), merged_project);
        }
    }

    // Always return a Value (empty object if nothing else)
    let result = serde_json::Value::Object(merged_map);

    tracing::debug!(
        "[DEBUG] build_merged_variant_inputs: dep_path='{}', result={:?}",
        dep.get_path(),
        result
    );

    result
}

/// Variant inputs with JSON value and computed hash.
///
/// This struct holds the variant inputs as a JSON value along with its
/// pre-computed SHA-256 hash for identity comparison. Computing the hash
/// once ensures consistency throughout the codebase.
///
/// Uses `#[serde(transparent)]` so it serializes as the JSON value directly,
/// which becomes a TOML table when serialized to TOML.
/// The hash is transient and recomputed after deserialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct VariantInputs {
    /// The JSON value
    json: serde_json::Value,
    /// SHA-256 hash of the serialized JSON (not serialized, computed on load)
    #[serde(skip)]
    hash: String,
}

impl PartialEq for VariantInputs {
    fn eq(&self, other: &Self) -> bool {
        // Compare by hash for performance (avoid deep JSON comparison)
        self.hash == other.hash
    }
}

impl Eq for VariantInputs {}

impl Default for VariantInputs {
    fn default() -> Self {
        Self::new(serde_json::Value::Object(serde_json::Map::new()))
    }
}

impl VariantInputs {
    /// Create a new VariantInputs from a JSON value, computing the hash once.
    pub fn new(json: serde_json::Value) -> Self {
        // Compute hash using centralized function
        let hash = crate::utils::compute_variant_inputs_hash(&json).unwrap_or_else(|_| {
            // Fallback to empty hash if serialization fails (shouldn't happen)
            tracing::error!("Failed to compute variant_inputs_hash, using empty hash");
            "sha256:".to_string()
        });

        Self {
            json,
            hash,
        }
    }

    /// Get the JSON value
    pub fn json(&self) -> &serde_json::Value {
        &self.json
    }

    /// Get the SHA-256 hash
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Recompute the hash from the JSON value.
    ///
    /// This is called after deserialization since the hash field is skipped.
    pub fn recompute_hash(&mut self) {
        self.hash = crate::utils::compute_variant_inputs_hash(&self.json).unwrap_or_else(|_| {
            tracing::error!("Failed to recompute variant_inputs_hash");
            "sha256:".to_string()
        });
    }
}

/// Adds or updates a resource entry in the lockfile based on resource type.
///
/// This helper method eliminates code duplication between the `resolve()` and `update()`
/// methods by centralizing lockfile entry management logic. It automatically determines
/// the resource type from the entry name and adds or updates the entry in the appropriate
/// collection within the lockfile.
///
/// The method performs upsert behavior - if an entry with matching name and source
/// already exists in the appropriate collection, it will be updated (including version);
/// otherwise, a new entry is added. This allows version updates (e.g., v1.0 → v2.0)
/// to replace the existing entry rather than creating duplicates.
///
/// # Arguments
///
/// * `lockfile` - Mutable reference to the lockfile to modify
/// * `entry` - The [`LockedResource`] entry to add or update
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn add_or_update_lockfile_entry(lockfile: &mut LockFile, entry: LockedResource) {
    let resources = lockfile.get_resources_mut(&entry.resource_type);

    if let Some(existing) = resources.iter_mut().find(|e| is_duplicate_entry(e, &entry)) {
        // Use deterministic merge strategy to ensure consistent lockfile generation
        if should_replace_duplicate(existing, &entry) {
            *existing = entry;
        }
    } else {
        resources.push(entry);
    }
}

/// Removes stale lockfile entries that are no longer in the manifest.
///
/// This method removes lockfile entries for direct manifest dependencies that have been
/// commented out or removed from the manifest. This must be called BEFORE
/// `remove_manifest_entries_for_update()` to ensure stale entries don't cause conflicts
/// during resolution.
///
/// A manifest-level entry is identified by:
/// - `manifest_alias.is_none()` - Direct dependency with no pattern expansion
/// - `manifest_alias.is_some()` - Pattern-expanded dependency (alias must be in manifest)
///
/// For each stale entry found, this also removes its transitive children to maintain
/// lockfile consistency.
///
/// # Arguments
///
/// * `manifest` - Reference to the current project manifest
/// * `lockfile` - The mutable lockfile to clean
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn remove_stale_manifest_entries(manifest: &Manifest, lockfile: &mut LockFile) {
    // Collect all current manifest keys for each resource type
    let manifest_agents: HashSet<String> = manifest.agents.keys().map(|k| k.to_string()).collect();
    let manifest_snippets: HashSet<String> =
        manifest.snippets.keys().map(|k| k.to_string()).collect();
    let manifest_commands: HashSet<String> =
        manifest.commands.keys().map(|k| k.to_string()).collect();
    let manifest_scripts: HashSet<String> =
        manifest.scripts.keys().map(|k| k.to_string()).collect();
    let manifest_hooks: HashSet<String> = manifest.hooks.keys().map(|k| k.to_string()).collect();
    let manifest_mcp_servers: HashSet<String> =
        manifest.mcp_servers.keys().map(|k| k.to_string()).collect();
    let manifest_skills: HashSet<String> = manifest.skills.keys().map(|k| k.to_string()).collect();

    // Helper to get the right manifest keys for a resource type
    let get_manifest_keys = |resource_type: ResourceType| match resource_type {
        ResourceType::Agent => &manifest_agents,
        ResourceType::Snippet => &manifest_snippets,
        ResourceType::Command => &manifest_commands,
        ResourceType::Script => &manifest_scripts,
        ResourceType::Hook => &manifest_hooks,
        ResourceType::McpServer => &manifest_mcp_servers,
        ResourceType::Skill => &manifest_skills,
    };

    // Collect (name, source) pairs to remove
    let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();
    let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

    // Find all manifest-level entries that are no longer in the manifest
    for resource_type in ResourceType::all() {
        let manifest_keys = get_manifest_keys(*resource_type);
        let resources = lockfile.get_resources(resource_type);

        for entry in resources {
            // Determine if this is a stale manifest-level entry (no longer in manifest)
            let is_stale = if let Some(ref alias) = entry.manifest_alias {
                // Pattern-expanded entry: stale if alias is NOT in manifest
                !manifest_keys.contains(alias)
            } else {
                // Direct entry: stale if name is NOT in manifest
                !manifest_keys.contains(&entry.name)
            };

            if is_stale {
                let key = (entry.name.clone(), entry.source.clone());
                entries_to_remove.insert(key.clone());
                direct_entries.push(key);
            }
        }
    }

    // For each stale entry, recursively collect its transitive children
    for (parent_name, parent_source) in direct_entries {
        for resource_type in ResourceType::all() {
            if let Some(parent_entry) = lockfile
                .get_resources(resource_type)
                .iter()
                .find(|e| e.name == parent_name && e.source == parent_source)
            {
                collect_transitive_children(lockfile, parent_entry, &mut entries_to_remove);
            }
        }
    }

    // Remove all marked entries
    let should_remove = |entry: &LockedResource| {
        entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
    };

    lockfile.agents.retain(|entry| !should_remove(entry));
    lockfile.snippets.retain(|entry| !should_remove(entry));
    lockfile.commands.retain(|entry| !should_remove(entry));
    lockfile.scripts.retain(|entry| !should_remove(entry));
    lockfile.hooks.retain(|entry| !should_remove(entry));
    lockfile.mcp_servers.retain(|entry| !should_remove(entry));
}

/// Removes lockfile entries for manifest dependencies that will be re-resolved.
///
/// This method removes old entries for direct manifest dependencies before updating,
/// which handles the case where a dependency's source or resource type changes.
/// This prevents duplicate entries with the same name but different sources.
///
/// Pattern-expanded and transitive dependencies are preserved because:
/// - Pattern expansions will be re-added during resolution with (name, source) matching
/// - Transitive dependencies aren't manifest keys and won't be removed
///
/// # Arguments
///
/// * `lockfile` - The mutable lockfile to clean
/// * `manifest_keys` - Set of manifest dependency keys being updated
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn remove_manifest_entries_for_update(
    lockfile: &mut LockFile,
    manifest_keys: &HashSet<String>,
) {
    // Collect (name, source) pairs to remove
    // We use (name, source) tuples to distinguish same-named resources from different sources
    let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();

    // Step 1: Find direct manifest entries and collect them for transitive traversal
    let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

    for resource_type in ResourceType::all() {
        let resources = lockfile.get_resources(resource_type);
        for entry in resources {
            // Check if this entry originates from a manifest key being updated
            if manifest_keys.contains(&entry.name)
                || entry.manifest_alias.as_ref().is_some_and(|alias| manifest_keys.contains(alias))
            {
                let key = (entry.name.clone(), entry.source.clone());
                entries_to_remove.insert(key.clone());
                direct_entries.push(key);
            }
        }
    }

    // Step 2: For each direct entry, recursively collect its transitive children
    // This ensures that when "agent-A" changes from repo1 to repo2, we also remove
    // all transitive dependencies that came from repo1 via agent-A
    for (parent_name, parent_source) in direct_entries {
        // Find the parent entry in the lockfile
        for resource_type in ResourceType::all() {
            if let Some(parent_entry) = lockfile
                .get_resources(resource_type)
                .iter()
                .find(|e| e.name == parent_name && e.source == parent_source)
            {
                // Walk its dependency tree
                collect_transitive_children(lockfile, parent_entry, &mut entries_to_remove);
            }
        }
    }

    // Step 3: Remove all marked entries
    let should_remove = |entry: &LockedResource| {
        entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
    };

    lockfile.agents.retain(|entry| !should_remove(entry));
    lockfile.snippets.retain(|entry| !should_remove(entry));
    lockfile.commands.retain(|entry| !should_remove(entry));
    lockfile.scripts.retain(|entry| !should_remove(entry));
    lockfile.hooks.retain(|entry| !should_remove(entry));
    lockfile.mcp_servers.retain(|entry| !should_remove(entry));
}

/// Recursively collect all transitive children of a lockfile entry.
///
/// This walks the dependency graph starting from `parent`, following the `dependencies`
/// field to find all resources that transitively depend on the parent. Only dependencies
/// with the same source as the parent are collected (to avoid removing unrelated resources).
///
/// The `dependencies` field contains strings in the format:
/// - `"resource_type/name"` for dependencies from the same source
/// - `"source:resource_type/name:version"` for explicit source references
///
/// # Arguments
///
/// * `lockfile` - The lockfile to search for dependencies
/// * `parent` - The parent entry whose children we want to collect
/// * `entries_to_remove` - Set of (name, source) pairs to populate with found children
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn collect_transitive_children(
    lockfile: &LockFile,
    parent: &LockedResource,
    entries_to_remove: &mut HashSet<(String, Option<String>)>,
) {
    // For each dependency declared by this parent
    for dep_ref in parent.parsed_dependencies() {
        let dep_path = &dep_ref.path;
        let resource_type = dep_ref.resource_type;

        // Extract the resource name from the path (filename without extension)
        let dep_name = dependency_helpers::extract_filename_from_path(dep_path)
            .unwrap_or_else(|| dep_path.to_string());

        // Determine the source: use explicit source from dep_ref if present, otherwise inherit from parent
        let dep_source = dep_ref.source.or_else(|| parent.source.clone());

        // Find the dependency entry with matching name and source
        if let Some(dep_entry) = lockfile
            .get_resources(&resource_type)
            .iter()
            .find(|e| e.name == dep_name && e.source == dep_source)
        {
            let key = (dep_entry.name.clone(), dep_entry.source.clone());

            // Add to removal set and recurse (if not already processed)
            if !entries_to_remove.contains(&key) {
                entries_to_remove.insert(key);
                // Recursively collect this dependency's children
                collect_transitive_children(lockfile, dep_entry, entries_to_remove);
            }
        }
    }
}

/// Detects conflicts where multiple dependencies resolve to the same installation path.
///
/// This method validates that no two dependencies will overwrite each other during
/// installation. It builds a map of all resolved `installed_at` paths and checks for
/// collisions across all resource types.
///
/// # Arguments
///
/// * `lockfile` - The lockfile containing all resolved dependencies
///
/// # Returns
///
/// Returns `Ok(())` if no conflicts are detected, or an error describing the conflicts.
///
/// # Errors
///
/// Returns an error if:
/// - Two or more dependencies resolve to the same `installed_at` path
/// - The error message lists all conflicting dependency names and the shared path
pub(super) fn detect_target_conflicts(lockfile: &LockFile) -> Result<()> {
    // Map of (installed_at path, resolved_commit) -> list of dependency names
    // Two dependencies with the same path AND same commit are NOT a conflict
    let mut path_map: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();

    // Collect all resources from lockfile
    // Note: Hooks and MCP servers are excluded because they're configuration-only
    // resources that are designed to share config files (.claude/settings.local.json
    // for hooks, .mcp.json for MCP servers), not individual files that would conflict.
    // Also skip resources with install=false since they don't create files.
    let all_resources: Vec<(&str, &LockedResource)> = lockfile
        .agents
        .iter()
        .filter(|r| r.install != Some(false))
        .map(|r| (r.name.as_str(), r))
        .chain(
            lockfile
                .snippets
                .iter()
                .filter(|r| r.install != Some(false))
                .map(|r| (r.name.as_str(), r)),
        )
        .chain(
            lockfile
                .commands
                .iter()
                .filter(|r| r.install != Some(false))
                .map(|r| (r.name.as_str(), r)),
        )
        .chain(
            lockfile
                .scripts
                .iter()
                .filter(|r| r.install != Some(false))
                .map(|r| (r.name.as_str(), r)),
        )
        // Hooks and MCP servers intentionally omitted - they share config files
        .collect();

    // Build the path map with commit information
    for (name, resource) in &all_resources {
        let key = (resource.installed_at.clone(), resource.resolved_commit.clone());
        path_map.entry(key).or_default().push((*name).to_string());
    }

    // Now check for actual conflicts: same path but DIFFERENT commits
    // Group by path only to find potential conflicts
    let mut path_only_map: HashMap<String, Vec<(&str, &LockedResource)>> = HashMap::new();
    for (name, resource) in &all_resources {
        path_only_map.entry(resource.installed_at.clone()).or_default().push((name, resource));
    }

    // Find conflicts (same path with different commits OR local deps with same path)
    let mut conflicts: Vec<(String, Vec<String>)> = Vec::new();
    for (path, resources) in path_only_map {
        if resources.len() > 1 {
            // Check if they have different commits
            let commits: HashSet<_> = resources.iter().map(|(_, r)| &r.resolved_commit).collect();

            // Conflict if:
            // 1. Different commits (different content from Git)
            // 2. All are local dependencies (resolved_commit = None) - can't overwrite same path
            let all_local = commits.len() == 1 && commits.contains(&None);

            if commits.len() > 1 || all_local {
                let names: Vec<String> = resources.iter().map(|(n, _)| (*n).to_string()).collect();
                conflicts.push((path, names));
            }
        }
    }

    if !conflicts.is_empty() {
        // Build a detailed error message
        let mut error_msg = String::from(
            "Target path conflicts detected:\n\n\
             Multiple dependencies resolve to the same installation path with different content.\n\
             This would cause files to overwrite each other.\n\n",
        );

        for (path, names) in &conflicts {
            error_msg.push_str(&format!("  Path: {}\n  Conflicts: {}\n\n", path, names.join(", ")));
        }

        error_msg.push_str(
            "To resolve this conflict:\n\
             1. Use custom 'target' field to specify different installation paths:\n\
                Example: target = \"custom/subdir/file.md\"\n\n\
             2. Use custom 'filename' field to specify different filenames:\n\
                Example: filename = \"utils-v2.md\"\n\n\
             3. For transitive dependencies, add them as direct dependencies with custom target/filename\n\n\
             4. Ensure pattern dependencies don't overlap with single-file dependencies\n\n\
             Note: This often occurs when different dependencies have transitive dependencies\n\
             with the same name but from different sources.",
        );

        return Err(anyhow::anyhow!(error_msg));
    }

    Ok(())
}

/// Add version information to dependency references in all lockfile entries.
///
/// This post-processing step updates the `dependencies` field of each locked resource
/// to include version information (e.g., converting "agent/helper" to "agent/helper@v1.0.0").
///
/// # Arguments
///
/// * `lockfile` - The mutable lockfile to update
pub(super) fn add_version_to_all_dependencies(lockfile: &mut LockFile) {
    use crate::resolver::types as dependency_helpers;

    // Build lookup map: (resource_type, normalized_path, source) -> name
    let mut lookup_map: HashMap<(ResourceType, String, Option<String>), String> = HashMap::new();

    // Build lookup from all lockfile entries
    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(resource_type) {
            let normalized_path = dependency_helpers::normalize_lookup_path(&entry.path);
            lookup_map.insert(
                (*resource_type, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );

            // Also store by filename for backward compatibility
            if let Some(filename) = dependency_helpers::extract_filename_from_path(&entry.path) {
                lookup_map
                    .insert((*resource_type, filename, entry.source.clone()), entry.name.clone());
            }

            // Also store by type-stripped path
            if let Some(stripped) =
                dependency_helpers::strip_resource_type_directory(&normalized_path)
            {
                lookup_map
                    .insert((*resource_type, stripped, entry.source.clone()), entry.name.clone());
            }
        }
    }

    // Build resource info map: (resource_type, name, source) -> (source, version)
    let mut resource_info_map: HashMap<ResourceKey, ResourceInfo> = HashMap::new();

    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(resource_type) {
            resource_info_map.insert(
                (*resource_type, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
    }

    // Update dependencies in all resources
    for resource_type in ResourceType::all() {
        let resources = lockfile.get_resources_mut(resource_type);
        for entry in resources {
            let parent_source = entry.source.clone();

            let updated_deps: Vec<String> = entry
                .dependencies
                .iter()
                .map(|dep| {
                    rewrite_dependency_string(
                        dep,
                        &lookup_map,
                        &resource_info_map,
                        parent_source.clone(),
                    )
                })
                .collect();

            entry.dependencies = updated_deps;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::lockfile::LockedResource;
    use crate::manifest::ResourceDependency;

    fn create_test_manifest() -> Manifest {
        let mut manifest = Manifest::default();
        manifest.agents.insert(
            "test-agent".to_string(),
            ResourceDependency::Simple("agents/test-agent.md".to_string()),
        );
        manifest.snippets.insert(
            "test-snippet".to_string(),
            ResourceDependency::Simple("snippets/test-snippet.md".to_string()),
        );
        manifest
    }

    fn create_test_lockfile() -> LockFile {
        let mut lockfile = LockFile::default();

        // Add some test entries
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/agents/test-agent.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });

        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "snippets/test-snippet.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:test2".to_string(),
            installed_at: ".claude/snippets/test-snippet.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Snippet,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        });

        lockfile
    }

    #[test]
    fn test_add_or_update_lockfile_entry_new() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = LockFile::default();

        let entry = LockedResource {
            resource_type: ResourceType::Agent,
            name: "new-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/new-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            installed_at: ".claude/agents/new-agent.md".to_string(),
            resolved_commit: Some("xyz789".to_string()),
            checksum: "sha256:new".to_string(),
            dependencies: vec![],
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        };

        builder.add_or_update_lockfile_entry(&mut lockfile, "new-agent", entry);

        assert_eq!(lockfile.agents.len(), 1);
        assert_eq!(lockfile.agents[0].name, "new-agent");
    }

    #[test]
    fn test_add_or_update_lockfile_entry_replace() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        let updated_entry = LockedResource {
            resource_type: ResourceType::Agent,
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("test-agent".to_string()), // Manifest dependency being updated
            context_checksum: None,
            installed_at: ".claude/agents/test-agent.md".to_string(),
            resolved_commit: Some("updated123".to_string()), // Updated commit
            checksum: "sha256:updated".to_string(),          // Updated checksum
            dependencies: vec![],
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        };

        builder.add_or_update_lockfile_entry(&mut lockfile, "test-agent", updated_entry);

        assert_eq!(lockfile.agents.len(), 1);
        assert_eq!(lockfile.agents[0].resolved_commit, Some("updated123".to_string()));
        assert_eq!(lockfile.agents[0].checksum, "sha256:updated");
    }

    #[test]
    fn test_remove_stale_manifest_entries() {
        let mut manifest = create_test_manifest();
        // Remove one agent from manifest to make it stale
        manifest.agents.remove("test-agent");

        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        builder.remove_stale_manifest_entries(&mut lockfile);

        // test-agent should be removed, test-snippet should remain
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 1);
        assert_eq!(lockfile.snippets[0].name, "test-snippet");
    }

    #[test]
    fn test_remove_manifest_entries_for_update() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        let mut manifest_keys = HashSet::new();
        manifest_keys.insert("test-agent".to_string());

        builder.remove_manifest_entries_for_update(&mut lockfile, &manifest_keys);

        // test-agent should be removed, test-snippet should remain
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 1);
        assert_eq!(lockfile.snippets[0].name, "test-snippet");
    }

    #[test]
    fn test_collect_transitive_children() {
        let lockfile = create_test_lockfile();
        let mut entries_to_remove = HashSet::new();

        // Create a parent with dependencies
        let parent = LockedResource {
            resource_type: ResourceType::Agent,
            name: "parent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/parent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            installed_at: ".claude/agents/parent.md".to_string(),
            resolved_commit: Some("parent123".to_string()),
            checksum: "sha256:parent".to_string(),
            dependencies: vec!["agent:agents/test-agent".to_string()], // Reference to test-agent (new format)
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            files: None,
        };

        LockfileBuilder::collect_transitive_children(&lockfile, &parent, &mut entries_to_remove);

        // Should find the test-agent dependency
        assert!(
            entries_to_remove.contains(&("test-agent".to_string(), Some("community".to_string())))
        );
    }

    #[test]
    fn test_build_merged_variant_inputs_preserves_all_keys() {
        use crate::manifest::DetailedDependency;
        use serde_json::json;

        // Create a manifest with no global project config
        let manifest_toml = r#"
[sources]
test-repo = "https://example.com/repo.git"
        "#;

        let manifest: Manifest = toml::from_str(manifest_toml).unwrap();

        // Create a dependency with template_vars containing both project and config
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test-repo".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(json!({
                "project": { "name": "Production" },
                "config": { "model": "claude-3-opus", "temperature": 0.5 }
            })),
        }));

        // Call build_merged_variant_inputs
        let result = build_merged_variant_inputs(&manifest, &dep);

        // Print the result for debugging
        println!(
            "Result: {}",
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
        );

        // Verify both project and config are present
        assert!(result.get("project").is_some(), "project should be present in variant_inputs");
        assert!(result.get("config").is_some(), "config should be present in variant_inputs");

        let config = result.get("config").unwrap();
        assert_eq!(config.get("model").unwrap().as_str().unwrap(), "claude-3-opus");
        assert_eq!(config.get("temperature").unwrap().as_f64().unwrap(), 0.5);
    }

    #[test]
    fn test_direct_vs_transitive_with_different_template_vars_should_not_deduplicate() {
        use serde_json::json;

        // Create direct dependency with template_vars = {lang: "rust"}
        let direct = LockedResource {
            name: "agents/generic".to_string(),
            manifest_alias: Some("generic-rust".to_string()), // Direct from manifest
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/generic.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:direct".to_string(),
            installed_at: ".claude/agents/generic-rust.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::new(json!({"lang": "rust"})),
            files: None,
        };

        // Create transitive dependency with template_vars = {lang: "python"}
        let transitive = LockedResource {
            name: "agents/generic".to_string(),
            manifest_alias: None, // Transitive dependency
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/generic.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:transitive".to_string(),
            installed_at: ".claude/agents/generic.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::new(json!({"lang": "python"})),
            files: None,
        };

        // According to the CRITICAL note in the code:
        // "template_vars are part of the resource identity! Resources with
        // different template_vars are DISTINCT resources that must all exist in the lockfile."
        //
        // Therefore, these should NOT be considered duplicates even though
        // one is direct and one is transitive.
        let is_dup = is_duplicate_entry(&direct, &transitive);

        assert!(
            !is_dup,
            "Direct and transitive dependencies with different template_vars should NOT be duplicates. \
             They represent distinct resources that both need to exist in the lockfile."
        );
    }
}
