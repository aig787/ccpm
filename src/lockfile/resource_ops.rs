//! Resource management operations for lockfiles.
//!
//! This module provides methods for adding, retrieving, and managing locked
//! resources (agents, snippets, commands, scripts, hooks, MCP servers) within
//! the lockfile.

use super::{LockFile, LockedResource, LockedSource, ResourceId};

impl LockFile {
    /// Add or update source repository, setting fetched_at to current UTC time.
    ///
    /// Replaces existing source with same name.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique source identifier (matches manifest `[sources]` keys)
    /// * `url` - Full Git repository URL
    /// * `commit` - Resolved 40-character commit hash
    ///
    /// # Behavior
    ///
    /// If a source with the same name already exists, it will be replaced with
    /// the new information. This ensures that each source name appears exactly
    /// once in the lockfile.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// let mut lockfile = LockFile::new();
    /// lockfile.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/example/community.git".to_string(),
    ///     "a1b2c3d4e5f6789abcdef0123456789abcdef012".to_string()
    /// );
    ///
    /// assert_eq!(lockfile.sources.len(), 1);
    /// assert_eq!(lockfile.sources[0].name, "community");
    /// ```
    ///
    /// # Time Zone
    ///
    /// The `fetched_at` timestamp is always recorded in UTC to ensure consistency
    /// across different time zones and systems.
    pub fn add_source(&mut self, name: String, url: String, _commit: String) {
        // Remove existing entry if present
        self.sources.retain(|s| s.name != name);

        self.sources.push(LockedSource {
            name,
            url,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Find source repository by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Source name to search for (matches manifest `[sources]` keys)
    ///
    /// # Returns
    ///
    /// * `Some(&LockedSource)` - Reference to the found source
    /// * `None` - No source with that name exists
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// if let Some(source) = lockfile.get_source("community") {
    ///     println!("Source URL: {}", source.url);
    ///     println!("Fetched at: {}", source.fetched_at);
    /// }
    /// ```
    #[must_use]
    pub fn get_source(&self, name: &str) -> Option<&LockedSource> {
        self.sources.iter().find(|s| s.name == name)
    }

    /// Add or update resource (agents or snippets only).
    ///
    /// Replaces existing resource with same name.
    ///
    /// **Note**: Backward compatibility only. Use `add_typed_resource` for all types.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique resource identifier within its type
    /// * `resource` - Complete [`LockedResource`] with all resolved information
    /// * `is_agent` - `true` for agents, `false` for snippets
    ///
    /// # Behavior
    ///
    /// If a resource with the same name already exists in the same type category,
    /// it will be replaced. Resources are categorized separately (agents vs snippets),
    /// so an agent named "helper" and a snippet named "helper" can coexist.
    ///
    /// # Examples
    ///
    /// Adding an agent:
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::{LockFile, LockedResourceBuilder};
    /// use agpm_cli::core::ResourceType;
    ///
    /// let mut lockfile = LockFile::new();
    /// let resource = LockedResourceBuilder::new(
    ///     "example-agent".to_string(),
    ///     "agents/example.md".to_string(),
    ///     "sha256:abcdef...".to_string(),
    ///     "agents/example-agent.md".to_string(),
    ///     ResourceType::Agent,
    /// )
    /// .source(Some("community".to_string()))
    /// .url(Some("https://github.com/example/repo.git".to_string()))
    /// .version(Some("^1.0".to_string()))
    /// .resolved_commit(Some("a1b2c3d...".to_string()))
    /// .tool(Some("claude-code".to_string()))
    /// .dependencies(Vec::new())
    /// .applied_patches(std::collections::BTreeMap::new())
    /// .build();
    ///
    /// lockfile.add_resource("example-agent".to_string(), resource, true);
    /// assert_eq!(lockfile.agents.len(), 1);
    /// ```
    ///
    /// Adding a snippet:
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, LockedResourceBuilder};
    /// # use agpm_cli::core::ResourceType;
    /// # let mut lockfile = LockFile::new();
    /// let snippet = LockedResourceBuilder::new(
    ///     "util-snippet".to_string(),
    ///     "../local/utils.md".to_string(),
    ///     "sha256:fedcba...".to_string(),
    ///     "snippets/util-snippet.md".to_string(),
    ///     ResourceType::Snippet,
    /// )
    /// .tool(Some("claude-code".to_string()))
    /// .dependencies(Vec::new())
    /// .applied_patches(std::collections::BTreeMap::new())
    /// .build();
    ///
    /// lockfile.add_resource("util-snippet".to_string(), snippet, false);
    /// assert_eq!(lockfile.snippets.len(), 1);
    /// ```
    pub fn add_resource(&mut self, name: String, resource: LockedResource, is_agent: bool) {
        let resources = if is_agent {
            &mut self.agents
        } else {
            &mut self.snippets
        };

        // Remove existing entry if present
        resources.retain(|r| r.name != name);
        resources.push(resource);
    }

    /// Add or update resource with explicit type support.
    ///
    /// Preferred method - supports all resource types.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique resource identifier within its type
    /// * `resource` - Complete [`LockedResource`] with all resolved information
    /// * `resource_type` - The type of resource (Agent, Snippet, or Command)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::{LockFile, LockedResourceBuilder};
    /// use agpm_cli::core::ResourceType;
    ///
    /// let mut lockfile = LockFile::new();
    /// let command = LockedResourceBuilder::new(
    ///     "build-command".to_string(),
    ///     "commands/build.md".to_string(),
    ///     "sha256:abcdef...".to_string(),
    ///     ".claude/commands/build-command.md".to_string(),
    ///     ResourceType::Command,
    /// )
    /// .source(Some("community".to_string()))
    /// .url(Some("https://github.com/example/repo.git".to_string()))
    /// .version(Some("v1.0.0".to_string()))
    /// .resolved_commit(Some("a1b2c3d...".to_string()))
    /// .tool(Some("claude-code".to_string()))
    /// .dependencies(Vec::new())
    /// .applied_patches(std::collections::BTreeMap::new())
    /// .build();
    ///
    /// lockfile.add_typed_resource("build-command".to_string(), command, ResourceType::Command);
    /// assert_eq!(lockfile.commands.len(), 1);
    /// ```
    pub fn add_typed_resource(
        &mut self,
        name: String,
        resource: LockedResource,
        resource_type: crate::core::ResourceType,
    ) {
        let resources = match resource_type {
            crate::core::ResourceType::Agent => &mut self.agents,
            crate::core::ResourceType::Snippet => &mut self.snippets,
            crate::core::ResourceType::Command => &mut self.commands,
            crate::core::ResourceType::McpServer => {
                // MCP servers are handled differently - they don't use LockedResource
                // This shouldn't be called for MCP servers
                return;
            }
            crate::core::ResourceType::Script => &mut self.scripts,
            crate::core::ResourceType::Hook => &mut self.hooks,
            crate::core::ResourceType::Skill => &mut self.skills,
        };

        // Remove existing entry if present
        resources.retain(|r| r.name != name);
        resources.push(resource);
    }

    /// Check if resource exists by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to check
    ///
    /// # Returns
    ///
    /// * `true` - Resource exists in the lockfile
    /// * `false` - Resource does not exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// if lockfile.has_resource("example-agent") {
    ///     println!("Agent is already locked");
    /// } else {
    ///     println!("Agent needs to be resolved and installed");
    /// }
    /// ```
    ///
    /// This is equivalent to calling `lockfile.get_resource(name).is_some()`.
    #[must_use]
    pub fn has_resource(&self, name: &str) -> bool {
        self.get_resource(name).is_some()
    }

    /// Internal name-based lookup across all types.
    ///
    /// Returns first match. External callers should use `find_resource_by_id` for proper lookup.
    #[must_use]
    pub(crate) fn get_resource(&self, name: &str) -> Option<&LockedResource> {
        // Simple name matching - may return first of multiple resources with same name
        // For precise matching when duplicates exist, use find_resource_by_id()
        // Matches both canonical name and manifest_alias for backward compatibility
        let matches =
            |r: &&LockedResource| r.name == name || r.manifest_alias.as_deref() == Some(name);

        self.agents
            .iter()
            .find(matches)
            .or_else(|| self.snippets.iter().find(matches))
            .or_else(|| self.commands.iter().find(matches))
            .or_else(|| self.scripts.iter().find(matches))
            .or_else(|| self.hooks.iter().find(matches))
            .or_else(|| self.mcp_servers.iter().find(matches))
    }

    /// Get resources by type as slice.
    pub fn get_resources(&self, resource_type: &crate::core::ResourceType) -> &[LockedResource] {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &self.agents,
            ResourceType::Snippet => &self.snippets,
            ResourceType::Command => &self.commands,
            ResourceType::Script => &self.scripts,
            ResourceType::Hook => &self.hooks,
            ResourceType::McpServer => &self.mcp_servers,
            ResourceType::Skill => &self.skills,
        }
    }

    /// Get mutable resources by type.
    pub const fn get_resources_mut(
        &mut self,
        resource_type: &crate::core::ResourceType,
    ) -> &mut Vec<LockedResource> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &mut self.agents,
            ResourceType::Snippet => &mut self.snippets,
            ResourceType::Command => &mut self.commands,
            ResourceType::Script => &mut self.scripts,
            ResourceType::Hook => &mut self.hooks,
            ResourceType::McpServer => &mut self.mcp_servers,
            ResourceType::Skill => &mut self.skills,
        }
    }

    /// Collect all resources across all types.
    ///
    /// Useful for operations processing resources uniformly:
    /// - Installation reports
    /// - Checksum validation
    /// - Bulk operations
    ///
    /// # Returns
    ///
    /// A vector containing references to all [`LockedResource`] entries in the lockfile.
    /// The order matches the resource type order defined in [`crate::core::ResourceType::all()`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// let all_resources = lockfile.all_resources();
    /// println!("Total locked resources: {}", all_resources.len());
    ///
    /// for resource in all_resources {
    ///     println!("- {}: {}", resource.name, resource.installed_at);
    /// }
    /// ```
    #[must_use]
    pub fn all_resources(&self) -> Vec<&LockedResource> {
        let mut resources = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            resources.extend(self.get_resources(resource_type));
        }

        resources
    }

    /// Clear all entries, returning lockfile to empty state.
    ///
    /// Format version unchanged.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// let mut lockfile = LockFile::new();
    /// // ... add sources and resources ...
    ///
    /// lockfile.clear();
    /// assert!(lockfile.sources.is_empty());
    /// assert!(lockfile.agents.is_empty());
    /// assert!(lockfile.snippets.is_empty());
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Preparing for complete lockfile regeneration
    /// - Implementing `agpm clean` functionality
    /// - Resetting lockfile state during testing
    /// - Handling lockfile corruption recovery
    pub fn clear(&mut self) {
        self.sources.clear();

        // Use ResourceType::all() to clear all resource types
        for resource_type in crate::core::ResourceType::all() {
            self.get_resources_mut(resource_type).clear();
        }
    }

    /// Find resource by name within specific type.
    ///
    /// More precise than `get_resource` when type is known.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to search for
    /// * `resource_type` - The type of resource to search within
    ///
    /// # Returns
    ///
    /// * `Some(&LockedResource)` - Reference to the found resource
    /// * `None` - No resource with that name exists in the specified type
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::core::ResourceType;
    /// # let lockfile = LockFile::new();
    /// // Find a specific agent
    /// if let Some(agent) = lockfile.find_resource("helper", &ResourceType::Agent) {
    ///     println!("Found agent: {}", agent.installed_at);
    /// }
    ///
    /// // Find a specific snippet
    /// if let Some(snippet) = lockfile.find_resource("utils", &ResourceType::Snippet) {
    ///     println!("Found snippet: {}", snippet.installed_at);
    /// }
    /// ```
    ///
    /// **Note**: External callers should prefer `find_resource_by_id(&ResourceId)` for ResourceId-based lookup.
    #[must_use]
    pub fn find_resource(
        &self,
        name: &str,
        resource_type: &crate::core::ResourceType,
    ) -> Option<&LockedResource> {
        self.get_resources(resource_type).iter().find(|r| r.name == name)
    }

    /// Find resource by complete ResourceId (canonical lookup method).
    ///
    /// Checks all identity fields: name, source, tool, template_vars.
    ///
    /// # Arguments
    ///
    /// * `id` - The complete ResourceId to search for
    ///
    /// # Returns
    ///
    /// * `Some(&LockedResource)` - Reference to the matching resource
    /// * `None` - No resource with that exact ID exists
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, ResourceId};
    /// # use agpm_cli::core::ResourceType;
    /// # use agpm_cli::utils::compute_variant_inputs_hash;
    /// # use serde_json::json;
    /// # let lockfile = LockFile::new();
    /// // Find resource with specific template_vars
    /// let template_vars = json!({"project": {"language": "python"}});
    /// let variant_hash = compute_variant_inputs_hash(&template_vars).unwrap_or_default();
    /// let id = ResourceId::new(
    ///     "backend-engineer",
    ///     Some("community"),
    ///     Some("claude-code"),
    ///     ResourceType::Agent,
    ///     variant_hash
    /// );
    ///
    /// if let Some(resource) = lockfile.find_resource_by_id(&id) {
    ///     println!("Found: {}", resource.installed_at);
    /// }
    /// ```
    #[must_use]
    pub fn find_resource_by_id(&self, id: &ResourceId) -> Option<&LockedResource> {
        // Search all resource types for exact ResourceId match
        self.agents
            .iter()
            .find(|r| r.matches_id(id))
            .or_else(|| self.snippets.iter().find(|r| r.matches_id(id)))
            .or_else(|| self.commands.iter().find(|r| r.matches_id(id)))
            .or_else(|| self.scripts.iter().find(|r| r.matches_id(id)))
            .or_else(|| self.hooks.iter().find(|r| r.matches_id(id)))
            .or_else(|| self.mcp_servers.iter().find(|r| r.matches_id(id)))
    }

    /// Find mutable resource by ResourceId.
    ///
    /// Use for modifications (checksums, patches, etc.).
    ///
    /// # Arguments
    ///
    /// * `id` - The complete ResourceId to search for
    ///
    /// # Returns
    ///
    /// * `Some(&mut LockedResource)` - Mutable reference to the matching resource
    /// * `None` - No resource with that exact ID exists
    #[must_use]
    pub fn find_resource_by_id_mut(&mut self, id: &ResourceId) -> Option<&mut LockedResource> {
        // Search all resource types for exact ResourceId match (mutable)
        if let Some(r) = self.agents.iter_mut().find(|r| r.matches_id(id)) {
            return Some(r);
        }
        if let Some(r) = self.snippets.iter_mut().find(|r| r.matches_id(id)) {
            return Some(r);
        }
        if let Some(r) = self.commands.iter_mut().find(|r| r.matches_id(id)) {
            return Some(r);
        }
        if let Some(r) = self.scripts.iter_mut().find(|r| r.matches_id(id)) {
            return Some(r);
        }
        if let Some(r) = self.hooks.iter_mut().find(|r| r.matches_id(id)) {
            return Some(r);
        }
        self.mcp_servers.iter_mut().find(|r| r.matches_id(id))
    }

    /// Get all resources by type for templating.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The type of resources to retrieve
    ///
    /// # Returns
    ///
    /// A slice of all resources of the specified type.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::core::ResourceType;
    /// # let lockfile = LockFile::new();
    /// // Get all agents for templating
    /// let agents = lockfile.get_resources_by_type(&ResourceType::Agent);
    /// for agent in agents {
    ///     println!("Agent: {} -> {}", agent.name, agent.installed_at);
    /// }
    ///
    /// // Get all snippets for templating
    /// let snippets = lockfile.get_resources_by_type(&ResourceType::Snippet);
    /// println!("Found {} snippets", snippets.len());
    /// ```
    ///
    /// # See Also
    ///
    /// * [`get_resources`](Self::get_resources) - Get resources by type (same method)
    /// * [`all_resources`](Self::all_resources) - Get all resources across all types
    #[must_use]
    pub fn get_resources_by_type(
        &self,
        resource_type: &crate::core::ResourceType,
    ) -> &[LockedResource] {
        self.get_resources(resource_type)
    }
}
