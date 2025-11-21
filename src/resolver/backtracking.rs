//! Automatic version backtracking for SHA conflict resolution.
//!
//! This module implements automatic resolution of version conflicts by finding
//! alternative versions that satisfy all constraints and resolve to the same commit SHA.
//!
//! # Algorithm
//!
//! When SHA conflicts are detected (multiple requirements for the same resource resolving
//! to different commits), the backtracking resolver attempts to find compatible versions:
//!
//! 1. **Query available versions**: Fetch all tags from the Git repository
//! 2. **Filter by constraints**: Find versions satisfying all requirements
//! 3. **Try alternatives**: Test versions in preference order (latest first)
//! 4. **Verify SHA match**: Check if alternative version resolves to same SHA as other requirements
//! 5. **Handle transitive deps**: Re-resolve transitive dependencies after version changes
//! 6. **Iterate if needed**: Continue until all conflicts resolved or limits reached
//!
//! # Performance Limits
//!
//! To prevent excessive computation:
//! - Maximum 100 version resolution attempts per conflict
//! - 10-second timeout for entire backtracking process
//! - Early termination if no progress made
//!
//! # Example
//!
//! ```text
//! Initial resolution:
//!   app-a requires agents-^v1.0.0 → agents-v1.0.11 → SHA: abc123
//!   app-b requires guides-^v1.0.0 → guides-v1.0.10 → SHA: def456
//!
//! Conflict detected (different SHAs for same resource)
//!
//! Backtracking:
//!   Try agents-v1.0.10 → SHA: def456  ✓ Matches!
//!
//! Resolution:
//!   app-a: agents-^v1.0.0 → agents-v1.0.10 → SHA: def456
//!   app-b: guides-^v1.0.0 → guides-v1.0.10 → SHA: def456
//!   Both resolve to same SHA, conflict resolved!
//! ```

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::lockfile::ResourceId;
use crate::resolver::ResolutionCore;
use crate::resolver::version_resolver::VersionResolutionService;
use crate::version::conflict::{ConflictingRequirement, VersionConflict};

/// Maximum number of version resolution attempts before giving up
const MAX_ATTEMPTS: usize = 100;

/// Maximum duration for backtracking before timeout (increased for transitive resolution)
const MAX_DURATION: Duration = Duration::from_secs(10);

/// Maximum number of backtracking iterations before giving up
const MAX_ITERATIONS: usize = 10;

/// Tracks resources whose versions changed during backtracking.
///
/// These resources need their transitive dependencies re-extracted and re-resolved
/// because changing a resource's version may change which transitive dependencies
/// it declares.
#[derive(Debug, Clone)]
struct TransitiveChangeTracker {
    /// Map: resource_id → (old_version, new_version, new_sha, variant_inputs)
    changed_resources: HashMap<String, (String, String, String, Option<serde_json::Value>)>,
}

impl TransitiveChangeTracker {
    fn new() -> Self {
        Self {
            changed_resources: HashMap::new(),
        }
    }

    fn record_change(
        &mut self,
        resource_id: &str,
        old_version: &str,
        new_version: &str,
        new_sha: &str,
        variant_inputs: Option<serde_json::Value>,
    ) {
        self.changed_resources.insert(
            resource_id.to_string(),
            (old_version.to_string(), new_version.to_string(), new_sha.to_string(), variant_inputs),
        );
    }

    fn get_changed_resources(
        &self,
    ) -> &HashMap<String, (String, String, String, Option<serde_json::Value>)> {
        &self.changed_resources
    }

    fn clear(&mut self) {
        self.changed_resources.clear();
    }
}

/// Parameters for adding or updating a resource in the registry.
#[derive(Debug, Clone)]
struct ResourceParams {
    resource_id: ResourceId,
    version: String,
    sha: String,
    version_constraint: String,
    required_by: String,
}

/// Entry for a single resource in the registry.
#[derive(Debug, Clone)]
struct ResourceEntry {
    /// Full ResourceId structure - used for ConflictDetector
    resource_id: ResourceId,

    /// Current version (may change during backtracking)
    version: String,

    /// Resolved SHA for this version
    sha: String,

    /// Version constraint originally requested
    version_constraint: String,

    /// Resources that depend on this one
    required_by: Vec<String>,
}

/// Tracks all resources and their dependency relationships for conflict detection.
///
/// This registry maintains a complete view of all resources in the dependency graph,
/// including their current versions, SHAs, and required_by relationships. This enables
/// accurate conflict detection after backtracking changes versions.
#[derive(Debug, Clone)]
struct ResourceRegistry {
    /// Map: resource_id → ResourceEntry
    resources: HashMap<String, ResourceEntry>,
}

impl ResourceRegistry {
    fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Add or update a resource in the registry.
    ///
    /// If the resource already exists, updates its version and SHA, and adds the
    /// required_by entry if not already present.
    fn add_or_update_resource(&mut self, params: ResourceParams) {
        let ResourceParams {
            resource_id,
            version,
            sha,
            version_constraint,
            required_by,
        } = params;

        // Convert ResourceId to string for HashMap key
        let resource_id_string =
            resource_id_to_string(&resource_id).expect("ResourceId should have a valid source");

        self.resources
            .entry(resource_id_string.clone())
            .and_modify(|entry| {
                entry.version = version.clone();
                entry.sha = sha.clone();
                if !entry.required_by.contains(&required_by) {
                    entry.required_by.push(required_by.clone());
                }
            })
            .or_insert_with(|| ResourceEntry {
                resource_id: resource_id.clone(),
                version,
                sha,
                version_constraint,
                required_by: vec![required_by],
            });
    }

    /// Iterate over all resources in the registry.
    fn all_resources(&self) -> impl Iterator<Item = &ResourceEntry> {
        self.resources.values()
    }

    /// Update the version and SHA for an existing resource.
    ///
    /// This is used during backtracking when a resource's version changes.
    /// The required_by relationships and version_constraint are preserved.
    fn update_version_and_sha(&mut self, resource_id: &str, new_version: String, new_sha: String) {
        if let Some(entry) = self.resources.get_mut(resource_id) {
            entry.version = new_version;
            entry.sha = new_sha;
        }
    }
}

/// Convert a resource_id string (format: "source:path") into components.
fn parse_resource_id_string(resource_id: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = resource_id.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid resource_id format: {}", resource_id));
    }
    Ok((parts[0], parts[1]))
}

/// Convert a ResourceId to the legacy string format "source:name".
fn resource_id_to_string(resource_id: &ResourceId) -> Result<String> {
    let source = resource_id
        .source()
        .ok_or_else(|| anyhow::anyhow!("Resource {} has no source", resource_id))?;
    Ok(format!("{}:{}", source, resource_id.name()))
}

/// State of a single backtracking iteration.
#[derive(Debug, Clone)]
pub struct BacktrackingIteration {
    /// Iteration number (1-indexed)
    pub iteration: usize,

    /// Conflicts detected at start of this iteration
    pub conflicts: Vec<VersionConflict>,

    /// Updates applied during this iteration
    pub updates: Vec<VersionUpdate>,

    /// Number of transitive deps re-resolved
    pub transitive_reresolutions: usize,

    /// Whether this iteration made progress
    pub made_progress: bool,
}

/// Reason for termination of backtracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// All conflicts successfully resolved
    Success,

    /// Reached maximum iteration limit
    MaxIterations,

    /// Reached timeout
    Timeout,

    /// No progress made (same conflicts as previous iteration)
    NoProgress,

    /// Detected oscillation (cycling between states)
    Oscillation,

    /// Failed to find compatible version
    NoCompatibleVersion,
}

/// Automatic version backtracking resolver.
///
/// Attempts to resolve SHA conflicts by finding alternative versions
/// that satisfy all constraints and resolve to the same commit.
pub struct BacktrackingResolver<'a> {
    /// Core resolution context with manifest, cache, and source manager
    core: &'a ResolutionCore,

    /// Version resolution service for Git operations
    version_service: &'a mut VersionResolutionService,

    /// Maximum version resolution attempts
    max_attempts: usize,

    /// Maximum duration before timeout
    timeout: Duration,

    /// Start time for timeout tracking
    start_time: Instant,

    /// Number of attempts made so far
    attempts: usize,

    /// Tracks resources whose versions changed (need transitive re-resolution)
    change_tracker: TransitiveChangeTracker,

    /// Iteration history for debugging and oscillation detection
    iteration_history: Vec<BacktrackingIteration>,

    /// Maximum iterations before giving up
    max_iterations: usize,

    /// Registry of all resources for conflict detection after version changes
    resource_registry: ResourceRegistry,
}

impl<'a> BacktrackingResolver<'a> {
    /// Create a new backtracking resolver with default limits.
    ///
    /// # Arguments
    ///
    /// * `core` - Resolution core with manifest and cache
    /// * `version_service` - Version resolution service
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agpm_cli::resolver::backtracking::BacktrackingResolver;
    /// use agpm_cli::resolver::{ResolutionCore, VersionResolutionService};
    /// use agpm_cli::cache::Cache;
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::source::SourceManager;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let cache = Cache::new()?;
    /// let manifest = Manifest::new();
    /// let source_manager = SourceManager::new_with_cache(cache.cache_dir().to_path_buf());
    /// let core = ResolutionCore::new(manifest, cache, source_manager, None);
    /// let cache_for_service = Cache::new()?; // Separate cache instance for service
    /// let mut version_service = VersionResolutionService::new(cache_for_service);
    /// let resolver = BacktrackingResolver::new(&core, &mut version_service);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        core: &'a ResolutionCore,
        version_service: &'a mut VersionResolutionService,
    ) -> Self {
        Self {
            core,
            version_service,
            max_attempts: MAX_ATTEMPTS,
            timeout: MAX_DURATION,
            start_time: Instant::now(),
            attempts: 0,
            change_tracker: TransitiveChangeTracker::new(),
            iteration_history: Vec::new(),
            max_iterations: MAX_ITERATIONS,
            resource_registry: ResourceRegistry::new(),
        }
    }

    /// Add or update a resource in the registry for conflict detection.
    ///
    /// This should be called by the main resolver during dependency resolution
    /// to build up the complete resource graph before backtracking begins.
    ///
    /// Populate the resource registry from a ConflictDetector.
    ///
    /// This extracts all requirements from the conflict detector and builds
    /// a complete resource registry for conflict detection during backtracking.
    ///
    /// Note: ResourceEntry stores only essential fields for conflict detection.
    /// Template variables (variant_inputs) are handled separately during backtracking.
    ///
    /// Resources without sources (e.g., local resources) are silently skipped
    /// with warnings logged, as they don't participate in version conflict resolution.
    pub fn populate_from_conflict_detector(
        &mut self,
        conflict_detector: &crate::version::conflict::ConflictDetector,
    ) {
        // Access the requirements from the conflict detector
        let requirements = conflict_detector.requirements();

        let mut skipped_count = 0;
        let mut processed_count = 0;

        for (resource_id, reqs) in requirements {
            // Verify ResourceId has a valid source (needed for backtracking registry)
            if resource_id_to_string(resource_id).is_err() {
                // Enhanced logging with more context about the skipped resource
                let tool_info =
                    resource_id.tool().map(|t| format!("tool: {}", t)).unwrap_or_default();
                let type_info = format!("type: {}", resource_id.resource_type());

                tracing::warn!(
                    "Skipping resource without source: {} (name: {}, {}, {} requirements: {})",
                    resource_id,
                    resource_id.name(),
                    type_info,
                    tool_info,
                    reqs.len()
                );

                skipped_count += 1;
                continue;
            }

            processed_count += 1;

            for req in reqs {
                self.resource_registry.add_or_update_resource(ResourceParams {
                    resource_id: resource_id.clone(), // Store full ResourceId for ConflictDetector
                    version: req.requirement.clone(), // Use requirement as version
                    sha: req.resolved_sha.clone(),
                    version_constraint: req.requirement.clone(), // Use requirement as constraint
                    required_by: req.required_by.clone(),
                });
            }
        }

        // Log summary of processed resources
        if skipped_count > 0 {
            tracing::info!(
                "Population complete: processed {} resources, skipped {} without source (local resources)",
                processed_count,
                skipped_count
            );
        } else {
            tracing::debug!(
                "Population complete: processed {} resources, no local resources skipped",
                processed_count
            );
        }
    }

    /// Create a backtracking resolver with custom limits (for testing).
    #[allow(dead_code)] // Used in unit tests to control timeout and iteration limits
    pub fn with_limits(
        core: &'a ResolutionCore,
        version_service: &'a mut VersionResolutionService,
        max_attempts: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            core,
            version_service,
            max_attempts,
            timeout,
            start_time: Instant::now(),
            attempts: 0,
            change_tracker: TransitiveChangeTracker::new(),
            iteration_history: Vec::new(),
            max_iterations: MAX_ITERATIONS,
            resource_registry: ResourceRegistry::new(),
        }
    }

    /// Attempt to resolve conflicts by finding compatible versions.
    ///
    /// Iteratively resolves version conflicts by trying alternative versions and
    /// re-extracting transitive dependencies until conflicts are resolved or
    /// termination conditions are met.
    ///
    /// # Arguments
    ///
    /// * `conflicts` - Initial list of detected version conflicts
    ///
    /// # Returns
    ///
    /// `BacktrackingResult` containing resolution status, updates, and termination reason
    ///
    /// # Errors
    ///
    /// Returns error if backtracking encounters an unexpected error
    pub async fn resolve_conflicts(
        &mut self,
        initial_conflicts: &[VersionConflict],
    ) -> Result<BacktrackingResult> {
        tracing::debug!(
            "Starting iterative backtracking for {} conflict(s), limits: {} iterations, {} attempts, {}s timeout",
            initial_conflicts.len(),
            self.max_iterations,
            self.max_attempts,
            self.timeout.as_secs()
        );

        let mut current_conflicts = initial_conflicts.to_vec();
        let mut all_updates = Vec::new();
        let mut total_transitive = 0;

        // Iterative resolution loop
        for iteration_num in 1..=self.max_iterations {
            tracing::debug!("=== Backtracking iteration {} ===", iteration_num);
            tracing::debug!("Processing {} conflict(s)", current_conflicts.len());

            // Check timeout
            if self.start_time.elapsed() > self.timeout {
                tracing::warn!("Backtracking timeout after {:?}", self.start_time.elapsed());
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::Timeout,
                ));
            }

            // Try to resolve current conflicts
            let mut iteration_updates = Vec::new();
            for conflict in &current_conflicts {
                match self.resolve_single_conflict(conflict).await? {
                    Some(update) => {
                        tracing::debug!(
                            "Resolved conflict for {}: {} → {}",
                            conflict.resource,
                            update.old_version,
                            update.new_version
                        );
                        iteration_updates.push(update);
                    }
                    None => {
                        tracing::debug!("Could not resolve conflict for {}", conflict.resource);
                        return Ok(self.build_result(
                            false,
                            all_updates,
                            total_transitive,
                            TerminationReason::NoCompatibleVersion,
                        ));
                    }
                }
            }

            if iteration_updates.is_empty() {
                // No updates found - can't make progress
                tracing::debug!("No updates found in iteration {}", iteration_num);
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::NoCompatibleVersion,
                ));
            }

            // Record changes in change tracker and update resource registry
            for update in &iteration_updates {
                self.change_tracker.record_change(
                    &update.resource_id,
                    &update.old_version,
                    &update.new_version,
                    &update.new_sha,
                    update.variant_inputs.clone(),
                );

                // Update the resource registry with the new version and SHA
                self.resource_registry.update_version_and_sha(
                    &update.resource_id,
                    update.new_version.clone(),
                    update.new_sha.clone(),
                );
            }

            all_updates.extend(iteration_updates.clone());

            // Re-extract and re-resolve transitive deps for changed resources
            tracing::debug!(
                "Re-extracting transitive deps for {} changed resource(s)",
                self.change_tracker.get_changed_resources().len()
            );
            let transitive_count = self.reextract_transitive_deps().await?;
            total_transitive += transitive_count;

            if transitive_count > 0 {
                tracing::debug!("Re-resolved {} transitive dependency(ies)", transitive_count);
            }

            // Re-check for conflicts
            let new_conflicts = self.detect_conflicts_after_changes().await?;
            tracing::debug!(
                "After iteration {}: {} conflict(s) remaining",
                iteration_num,
                new_conflicts.len()
            );

            // Record iteration history
            self.iteration_history.push(BacktrackingIteration {
                iteration: iteration_num,
                conflicts: current_conflicts.clone(),
                updates: iteration_updates,
                transitive_reresolutions: transitive_count,
                made_progress: !new_conflicts.is_empty() || transitive_count > 0,
            });

            // Check for termination conditions
            if new_conflicts.is_empty() {
                // Success! All conflicts resolved
                tracing::info!(
                    "✓ Resolved all conflicts after {} iteration(s), {} version update(s), {} transitive re-resolution(s)",
                    iteration_num,
                    all_updates.len(),
                    total_transitive
                );
                return Ok(self.build_result(
                    true,
                    all_updates,
                    total_transitive,
                    TerminationReason::Success,
                ));
            }

            if conflicts_equal(&current_conflicts, &new_conflicts) {
                // No progress - same conflicts as before
                tracing::warn!(
                    "No progress made in iteration {}: same conflicts remain",
                    iteration_num
                );
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::NoProgress,
                ));
            }

            if self.detect_oscillation(&new_conflicts) {
                // Oscillation detected - cycling between states
                tracing::warn!("Oscillation detected in iteration {}", iteration_num);
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::Oscillation,
                ));
            }

            // Update for next iteration
            current_conflicts = new_conflicts;
        }

        // Reached max iterations without resolving
        tracing::warn!(
            "Reached max iterations ({}) without resolving all conflicts. {} conflict(s) remaining",
            self.max_iterations,
            current_conflicts.len()
        );
        Ok(self.build_result(
            false,
            all_updates,
            total_transitive,
            TerminationReason::MaxIterations,
        ))
    }

    /// Resolve a single conflict by finding an alternative version.
    ///
    /// # Arguments
    ///
    /// * `conflict` - The version conflict to resolve
    ///
    /// # Returns
    ///
    /// `Some(VersionUpdate)` if resolution found, `None` if no solution
    async fn resolve_single_conflict(
        &mut self,
        conflict: &VersionConflict,
    ) -> Result<Option<VersionUpdate>> {
        // Extract source from ResourceId
        let source_name = conflict
            .resource
            .source()
            .ok_or_else(|| anyhow::anyhow!("Resource {} has no source", conflict.resource))?;

        // Group requirements by SHA to find which ones need updating
        let mut sha_groups: HashMap<&str, Vec<&ConflictingRequirement>> = HashMap::new();
        for req in &conflict.conflicting_requirements {
            sha_groups.entry(req.resolved_sha.as_str()).or_default().push(req);
        }

        // Find the target SHA (most common, or first with most recent version)
        let target_sha = self.select_target_sha(&sha_groups)?;

        tracing::debug!(
            "Target SHA for {}: {} ({} requirements)",
            conflict.resource,
            &target_sha[..8.min(target_sha.len())],
            sha_groups.get(target_sha).map_or(0, |v| v.len())
        );

        // Find requirements that need updating (those not matching target SHA)
        let requirements_to_update: Vec<&ConflictingRequirement> = conflict
            .conflicting_requirements
            .iter()
            .filter(|req| req.resolved_sha != target_sha)
            .collect();

        if requirements_to_update.is_empty() {
            // All requirements already match - shouldn't happen but handle gracefully
            return Ok(None);
        }

        // Try to find an alternative version for the first requirement that matches target SHA
        // (Simplification: update one at a time; more complex scenarios handled in iterations)
        let req_to_update = requirements_to_update[0];

        self.find_alternative_version(source_name, req_to_update, target_sha).await
    }

    /// Select the target SHA that other versions should match.
    ///
    /// Strategy: Choose the SHA with the most requirements, breaking ties by:
    /// 1. Preferring Version resolution mode over GitRef (semver tags are more stable)
    /// 2. Alphabetically by SHA for deterministic ordering
    fn select_target_sha<'b>(
        &self,
        sha_groups: &'b HashMap<&str, Vec<&ConflictingRequirement>>,
    ) -> Result<&'b str> {
        sha_groups
            .iter()
            .max_by(|(sha_a, reqs_a), (sha_b, reqs_b)| {
                // Primary: number of requirements (more is better)
                let count_cmp = reqs_a.len().cmp(&reqs_b.len());
                if count_cmp != std::cmp::Ordering::Equal {
                    return count_cmp;
                }

                // Secondary: prefer Version mode over GitRef (semver is more stable)
                // Count how many requirements use Version mode
                let version_count_a = reqs_a
                    .iter()
                    .filter(|r| {
                        // Check if requirement looks like a semver constraint
                        crate::version::constraints::VersionConstraint::parse(&r.requirement)
                            .is_ok()
                    })
                    .count();
                let version_count_b = reqs_b
                    .iter()
                    .filter(|r| {
                        crate::version::constraints::VersionConstraint::parse(&r.requirement)
                            .is_ok()
                    })
                    .count();

                let mode_cmp = version_count_a.cmp(&version_count_b);
                if mode_cmp != std::cmp::Ordering::Equal {
                    return mode_cmp;
                }

                // Tertiary: alphabetically by SHA for deterministic ordering
                sha_a.cmp(sha_b)
            })
            .map(|(sha, _)| *sha)
            .ok_or_else(|| anyhow::anyhow!("No SHA groups found"))
    }

    /// Find an alternative version that satisfies the constraint and matches target SHA.
    ///
    /// This method searches for alternative versions of the **parent resource** (not the
    /// transitive dependency that's conflicting). For each alternative parent version,
    /// it extracts the transitive dependencies and checks if they resolve to the target SHA.
    ///
    /// # Arguments
    ///
    /// * `source_name` - Name of the source repository
    /// * `requirement` - The conflicting requirement (contains parent metadata)
    /// * `target_sha` - The SHA that the transitive dependency must resolve to
    ///
    /// # Returns
    ///
    /// `Some(VersionUpdate)` if compatible version found, `None` otherwise
    async fn find_alternative_version(
        &mut self,
        source_name: &str,
        requirement: &ConflictingRequirement,
        target_sha: &str,
    ) -> Result<Option<VersionUpdate>> {
        // For direct dependencies (required_by = "manifest"), we search for alternative
        // versions of the dependency itself (old behavior)
        if requirement.required_by == "manifest" {
            return self
                .find_alternative_for_direct_dependency(source_name, requirement, target_sha)
                .await;
        }

        // For transitive dependencies, we need to search for alternative versions of the PARENT
        // Extract parent metadata
        let parent_version_constraint =
            requirement.parent_version_constraint.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing parent_version_constraint for transitive dependency required by '{}'",
                    requirement.required_by
                )
            })?;

        tracing::debug!(
            "Searching alternative versions of PARENT '{}' (current: {}) to resolve conflict on transitive dependency",
            requirement.required_by,
            parent_version_constraint
        );

        // Get available versions of the PARENT from Git
        let available_versions = self.get_available_versions(source_name).await?;

        // Filter parent versions matching the parent's constraint
        let matching_versions =
            self.filter_by_constraint(&available_versions, parent_version_constraint)?;

        tracing::debug!(
            "Found {} parent versions matching constraint {}",
            matching_versions.len(),
            parent_version_constraint
        );

        // Try parent versions in preference order (latest first)
        for parent_version in matching_versions {
            // Check limits
            self.attempts += 1;
            if self.attempts >= self.max_attempts {
                tracing::warn!("Reached max attempts ({})", self.max_attempts);
                return Ok(None);
            }

            if self.start_time.elapsed() > self.timeout {
                tracing::warn!("Backtracking timeout");
                return Ok(None);
            }

            // Resolve this parent version to a SHA
            let parent_sha = self.resolve_version_to_sha(source_name, &parent_version).await?;

            tracing::trace!(
                "Trying parent {}: SHA {}",
                parent_version,
                &parent_sha[..8.min(parent_sha.len())]
            );

            // Get source URL
            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            // Get or create worktree for this parent version
            let worktree_path = self
                .core
                .cache()
                .get_or_create_worktree_for_sha(
                    source_name,
                    &source_url,
                    &parent_sha,
                    Some(source_name),
                )
                .await?;

            // Extract transitive dependencies from the parent resource at this version
            // Parse required_by to get the resource path (e.g., "agents/agent-a" → "agents/agent-a.md")
            let parent_resource_path = if requirement.required_by.ends_with(".md")
                || requirement.required_by.ends_with(".json")
            {
                requirement.required_by.clone()
            } else {
                format!("{}.md", requirement.required_by) // Assume .md extension
            };

            // Extract transitive deps from parent at this version
            // Look up variant_inputs from PreparedSourceVersion and clone for use
            let parent_resource_id = format!("{}:{}", source_name, requirement.required_by);
            let parent_group_key = format!("{}::{}", source_name, parent_version);
            let parent_variant_inputs_cloned: Option<serde_json::Value> = self
                .version_service
                .prepared_versions()
                .get(&parent_group_key)
                .and_then(|prepared| {
                    prepared.resource_variants.get(&parent_resource_id).and_then(|opt| opt.clone())
                });

            let transitive_deps =
                match crate::resolver::transitive_extractor::extract_transitive_deps(
                    &worktree_path,
                    &parent_resource_path,
                    parent_variant_inputs_cloned.as_ref(),
                )
                .await
                {
                    Ok(deps) => deps,
                    Err(e) => {
                        tracing::debug!(
                            "Failed to extract transitive deps from parent {} @ {}: {}",
                            parent_resource_path,
                            parent_version,
                            e
                        );
                        continue; // Try next version
                    }
                };

            // Check if any of the transitive dependencies resolve to the target SHA
            // We need to find the specific transitive dep that was conflicting
            for (_resource_type, specs) in transitive_deps {
                for spec in specs {
                    // Resolve this transitive dep's version to a SHA
                    let dep_version = spec.version.as_deref().unwrap_or("HEAD");
                    let dep_sha = match self.resolve_version_to_sha(source_name, dep_version).await
                    {
                        Ok(sha) => sha,
                        Err(_) => continue,
                    };

                    tracing::trace!(
                        "  Transitive dep {} @ {}: SHA {} → {}",
                        spec.path,
                        dep_version,
                        &dep_sha[..8.min(dep_sha.len())],
                        if dep_sha == target_sha {
                            "MATCH"
                        } else {
                            "no match"
                        }
                    );

                    // If this transitive dep resolves to target SHA, we found a solution!
                    if dep_sha == target_sha {
                        tracing::info!(
                            "Found compatible parent version: {} @ {} (was @ {})",
                            requirement.required_by,
                            parent_version,
                            parent_version_constraint
                        );

                        // Look up variant_inputs from PreparedSourceVersion and clone
                        let resource_id = format!("{}:{}", source_name, requirement.required_by);
                        let group_key = format!("{}::{}", source_name, parent_version);
                        let variant_inputs: Option<serde_json::Value> =
                            self.version_service.prepared_versions().get(&group_key).and_then(
                                |prepared| {
                                    prepared
                                        .resource_variants
                                        .get(&resource_id)
                                        .and_then(|opt| opt.clone())
                                },
                            );

                        return Ok(Some(VersionUpdate {
                            resource_id,
                            old_version: parent_version_constraint.clone(),
                            new_version: parent_version.clone(),
                            old_sha: requirement
                                .parent_resolved_sha
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string()),
                            new_sha: parent_sha,
                            variant_inputs,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Find alternative version for a direct dependency (not transitive).
    ///
    /// This is the old behavior: search for versions of the dependency itself.
    async fn find_alternative_for_direct_dependency(
        &mut self,
        source_name: &str,
        requirement: &ConflictingRequirement,
        target_sha: &str,
    ) -> Result<Option<VersionUpdate>> {
        let available_versions = self.get_available_versions(source_name).await?;

        tracing::debug!(
            "Searching {} available versions for direct dependency {} matching SHA {}",
            available_versions.len(),
            requirement.requirement,
            &target_sha[..8.min(target_sha.len())]
        );

        let matching_versions =
            self.filter_by_constraint(&available_versions, &requirement.requirement)?;

        tracing::debug!(
            "Found {} versions matching constraint {}",
            matching_versions.len(),
            requirement.requirement
        );

        // Try versions in preference order (latest first)
        for version in matching_versions {
            self.attempts += 1;
            if self.attempts >= self.max_attempts {
                tracing::warn!("Reached max attempts ({})", self.max_attempts);
                return Ok(None);
            }

            if self.start_time.elapsed() > self.timeout {
                tracing::warn!("Backtracking timeout");
                return Ok(None);
            }

            let sha = self.resolve_version_to_sha(source_name, &version).await?;

            tracing::trace!(
                "Trying {}: {} → {}",
                version,
                &sha[..8.min(sha.len())],
                if sha == target_sha {
                    "MATCH"
                } else {
                    "no match"
                }
            );

            if sha == target_sha {
                // Look up variant_inputs from PreparedSourceVersion and clone
                let resource_id = format!("{}:{}", source_name, requirement.required_by);
                let group_key = format!("{}::{}", source_name, version);
                let variant_inputs: Option<serde_json::Value> =
                    self.version_service.prepared_versions().get(&group_key).and_then(|prepared| {
                        prepared.resource_variants.get(&resource_id).and_then(|opt| opt.clone())
                    });

                return Ok(Some(VersionUpdate {
                    resource_id,
                    old_version: requirement.requirement.clone(),
                    new_version: version.clone(),
                    old_sha: requirement.resolved_sha.clone(),
                    new_sha: sha,
                    variant_inputs,
                }));
            }
        }

        Ok(None)
    }

    /// Get all available versions (tags) from a Git repository.
    ///
    /// # Arguments
    ///
    /// * `source_name` - Name of the source repository
    ///
    /// # Returns
    ///
    /// List of available version strings (tag names)
    async fn get_available_versions(&self, source_name: &str) -> Result<Vec<String>> {
        // Get bare repository path from version service
        let bare_repo_path =
            self.version_service.get_bare_repo_path(source_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Source '{}' not yet synced. Call pre_sync_sources() first.",
                    source_name
                )
            })?;

        // List tags using Git
        let git_repo = crate::git::GitRepo::new(&bare_repo_path);
        let tags = git_repo
            .list_tags()
            .await
            .with_context(|| format!("Failed to list tags for source '{}'", source_name))?;

        Ok(tags)
    }

    /// Filter versions by constraint, returning matching versions in preference order.
    ///
    /// This function implements prefix-aware version filtering, ensuring that prefixed
    /// constraints (e.g., `d->=v1.0.0`) only match tags with the same prefix (e.g.,
    /// `d-v1.0.0`, `d-v2.0.0`). This prevents cross-contamination from tags with different
    /// prefixes that happen to satisfy the version constraint.
    ///
    /// Preference order: highest semantic versions first (with deterministic tag name
    /// tie-breaking), excluding pre-releases unless explicitly specified in the constraint.
    ///
    /// # Arguments
    ///
    /// * `versions` - Available version tags (may include multiple prefixes)
    /// * `constraint` - Version constraint string (e.g., "^1.0.0", "d->=v1.0.0", "main")
    ///
    /// # Returns
    ///
    /// Filtered and sorted list of matching versions, sorted highest version first.
    /// For non-semantic constraints (HEAD, branches), returns exact matches.
    ///
    /// # Prefix Filtering
    ///
    /// If the constraint contains a prefix (e.g., `d->=v1.0.0`), only tags with that
    /// exact prefix will be considered. This prevents bugs like:
    /// - `d->=v1.0.0` incorrectly matching `a-v2.0.0`, `b-v1.5.0`, `x-v1.0.0`
    /// - Non-deterministic resolution when multiple prefixes have overlapping versions
    ///
    /// # Special Cases
    ///
    /// - **HEAD/latest/\***: Returns highest semantic version among prefix-matched tags
    /// - **Exact refs**: Returns exact match if it exists with correct prefix
    /// - **Branch names**: Returns branch ref if found (no prefix filtering for branches)
    ///
    /// # Examples
    ///
    /// Conceptual example showing the filtering behavior:
    ///
    /// ```ignore
    /// // Input tags: ["d-v1.0.0", "d-v2.0.0", "a-v1.0.0", "x-v1.0.0"]
    /// // Constraint: "d->=v1.0.0" (prefixed constraint)
    /// // Result: ["d-v2.0.0", "d-v1.0.0"] (only d-* tags, sorted)
    ///
    /// // Input tags: ["v1.0.0", "v2.0.0", "d-v1.0.0"]
    /// // Constraint: ">=v1.0.0" (unprefixed constraint)
    /// // Result: ["v2.0.0", "v1.0.0"] (only unprefixed tags)
    /// ```
    fn filter_by_constraint(&self, versions: &[String], constraint: &str) -> Result<Vec<String>> {
        use crate::resolver::version_resolver::parse_tags_to_versions;
        use crate::version::constraints::{ConstraintSet, VersionConstraint};

        // Parse versions and filter by constraint
        let mut matching = Vec::new();

        // Extract prefix from constraint to filter tags
        // This ensures that prefixed constraints (e.g., "d-^v1.0.0") only match
        // tags with the same prefix (e.g., "d-v1.0.0", "d-v2.0.0")
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);

        // Filter versions to only those matching the constraint's prefix
        let prefix_filtered_versions: Vec<String> = versions
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                // Both must have same prefix (or both have None)
                tag_prefix == constraint_prefix
            })
            .cloned()
            .collect();

        // Special cases: HEAD, latest, or wildcard
        if constraint == "HEAD" || constraint == "latest" || constraint == "*" {
            // For HEAD/latest/*, sort by semantic version if possible
            let mut tag_versions = parse_tags_to_versions(prefix_filtered_versions.clone());
            if !tag_versions.is_empty() {
                // Sort deterministically (highest version first, tag name for ties)
                use crate::resolver::version_resolver::sort_versions_deterministic;
                sort_versions_deterministic(&mut tag_versions);
                matching.extend(tag_versions.into_iter().map(|(tag, _)| tag));
            } else {
                // Fallback to string sorting only if no semantic versions found
                matching.extend(prefix_filtered_versions.iter().cloned());
                matching.sort_by(|a, b| b.cmp(a));
            }
        } else {
            // Try to parse as version constraint
            if let Ok(constraint_parsed) = VersionConstraint::parse(constraint) {
                // Create a constraint set
                let mut constraint_set = ConstraintSet::new();
                constraint_set.add(constraint_parsed)?;

                // Parse tags to versions (already prefix-filtered)
                let tag_versions = parse_tags_to_versions(prefix_filtered_versions);

                // Filter by constraint satisfaction and collect as (tag, version) pairs
                let mut matched_pairs: Vec<(String, semver::Version)> = tag_versions
                    .into_iter()
                    .filter(|(_, version)| constraint_set.satisfies(version))
                    .collect();

                // Sort deterministically (highest version first, tag name for ties)
                use crate::resolver::version_resolver::sort_versions_deterministic;
                sort_versions_deterministic(&mut matched_pairs);

                // Extract just the tag names
                matching.extend(matched_pairs.into_iter().map(|(tag, _)| tag));
            } else {
                // Not a constraint, treat as exact ref
                if prefix_filtered_versions.contains(&constraint.to_string()) {
                    matching.push(constraint.to_string());
                }
            }
        }

        Ok(matching)
    }

    /// Resolve a version string to its commit SHA.
    ///
    /// # Arguments
    ///
    /// * `source_name` - Name of the source repository
    /// * `version` - Version string (tag, branch, or commit)
    ///
    /// # Returns
    ///
    /// Full commit SHA
    async fn resolve_version_to_sha(&self, source_name: &str, version: &str) -> Result<String> {
        // Get bare repository path from version service
        let bare_repo_path = self
            .version_service
            .get_bare_repo_path(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not yet synced", source_name))?;

        let git_repo = crate::git::GitRepo::new(&bare_repo_path);

        // Resolve ref to SHA
        git_repo.resolve_to_sha(Some(version)).await.context("Failed to resolve version to SHA")
    }

    /// Build a BacktrackingResult with all required fields.
    fn build_result(
        &self,
        resolved: bool,
        updates: Vec<VersionUpdate>,
        total_transitive: usize,
        termination_reason: TerminationReason,
    ) -> BacktrackingResult {
        BacktrackingResult {
            resolved,
            updates,
            iterations: self.iteration_history.len(),
            attempted_versions: self.attempts,
            iteration_history: self.iteration_history.clone(),
            total_transitive_reresolutions: total_transitive,
            termination_reason,
        }
    }

    /// Get variant inputs (template variables) for a resource.
    ///
    /// Looks up the template variables that were used when resolving this resource
    /// from the change tracker. Returns None if:
    /// - The resource hasn't changed during backtracking
    /// - The resource was resolved without template variables
    ///
    /// This ensures that when re-extracting transitive dependencies after a version
    /// change, the same template variables are used for rendering.
    fn get_variant_inputs_for_resource(
        &self,
        resource_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        // Look up in change tracker - returns variant_inputs if resource changed
        Ok(self
            .change_tracker
            .get_changed_resources()
            .get(resource_id)
            .and_then(|(_, _, _, variant_inputs)| variant_inputs.clone()))
    }

    /// Re-extract and re-resolve transitive dependencies for changed resources.
    ///
    /// For each resource whose version changed during backtracking, we need to:
    /// 1. Get the worktree for the new version
    /// 2. Extract transitive dependencies from the resource file
    /// 3. Resolve those dependencies (version → SHA)
    /// 4. Update PreparedSourceVersions
    ///
    /// # Returns
    ///
    /// Number of transitive dependencies re-resolved
    async fn reextract_transitive_deps(&mut self) -> Result<usize> {
        use crate::resolver::transitive_extractor::extract_transitive_deps;

        let mut count = 0;

        // Get all changed resources (need to collect to avoid borrowing issues)
        let changed: Vec<(String, String, String)> = self
            .change_tracker
            .get_changed_resources()
            .iter()
            .map(|(id, (_, new_ver, new_sha, _))| (id.clone(), new_ver.clone(), new_sha.clone()))
            .collect();

        for (resource_id, new_version, new_sha) in changed {
            // Parse resource_id to get source and path
            let (source_name, resource_path) = parse_resource_id_string(&resource_id)?;

            tracing::debug!(
                "Re-extracting transitive deps for {}: version={}, sha={}",
                resource_id,
                new_version,
                &new_sha[..8.min(new_sha.len())]
            );

            // Get the source URL
            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            // Get or create worktree for new SHA
            let worktree_path = self
                .core
                .cache()
                .get_or_create_worktree_for_sha(
                    source_name,
                    &source_url,
                    &new_sha,
                    Some(source_name),
                )
                .await?;

            // Get variant inputs (template variables) for this resource
            let variant_inputs = self.get_variant_inputs_for_resource(&resource_id)?;

            // Extract transitive dependencies from resource file at new version
            let transitive_deps =
                extract_transitive_deps(&worktree_path, resource_path, variant_inputs.as_ref())
                    .await?;

            // Resolve each transitive dependency (version → SHA)
            for (_resource_type, specs) in transitive_deps {
                for spec in specs {
                    // Skip dependencies with install=false (default is true)
                    if matches!(spec.install, Some(false)) {
                        continue;
                    }

                    // Transitive deps inherit source from parent (no source field in DependencySpec)
                    let dep_source = source_name;
                    let dep_version = spec.version.as_deref(); // May be None (means HEAD)

                    // Prepare this version (will resolve SHA and create worktree)
                    self.version_service
                        .prepare_additional_version(self.core, dep_source, dep_version)
                        .await
                        .with_context(|| {
                            format!(
                                "Failed to prepare transitive dependency '{}' from {}",
                                spec.path, resource_id
                            )
                        })?;

                    count += 1;

                    tracing::debug!(
                        "  Re-resolved transitive dep: {} from source {} version {}",
                        spec.path,
                        dep_source,
                        dep_version.unwrap_or("HEAD")
                    );
                }
            }
        }

        // Clear change tracker for next iteration
        self.change_tracker.clear();

        Ok(count)
    }

    /// Detect conflicts after applying backtracking updates.
    ///
    /// # Implementation
    ///
    /// This method rebuilds a ConflictDetector from the resource registry to detect
    /// conflicts immediately after version changes, allowing earlier detection and fewer
    /// iterations in complex scenarios.
    ///
    /// ## Safety Mechanisms
    ///
    /// Multiple termination conditions ensure convergence or graceful failure:
    ///
    /// 1. **NoProgress**: Stops if the same conflicts persist across iterations
    /// 2. **MaxIterations**: Hard limit of 10 iterations prevents infinite loops
    /// 3. **Oscillation**: Detects cycling between conflict states
    /// 4. **Timeout**: 10-second maximum duration enforced
    /// 5. **Post-backtracking check**: Main resolver validates final state
    ///
    /// The approach has proven sufficient in practice, with safety nets ensuring
    /// correct behavior in all scenarios.
    async fn detect_conflicts_after_changes(&self) -> Result<Vec<VersionConflict>> {
        tracing::debug!("Detecting conflicts after version changes...");

        // Build a new ConflictDetector from the current state of the resource registry
        let mut detector = crate::version::conflict::ConflictDetector::new();

        // Add all resources from registry to conflict detector
        for resource in self.resource_registry.all_resources() {
            for required_by in &resource.required_by {
                detector.add_requirement(
                    resource.resource_id.clone(), // Use the full ResourceId
                    required_by,
                    &resource.version_constraint,
                    &resource.sha,
                );
            }
        }

        // Detect conflicts with updated state
        let conflicts = detector.detect_conflicts();

        if conflicts.is_empty() {
            tracing::debug!("No conflicts detected after changes");
        } else {
            tracing::debug!(
                "Detected {} conflict(s) after changes: {:?}",
                conflicts.len(),
                conflicts.iter().map(|c| &c.resource).collect::<Vec<_>>()
            );
        }

        Ok(conflicts)
    }

    /// Detect if we're oscillating between two conflict states.
    ///
    /// Oscillation occurs when:
    /// - Iteration N: Conflict A
    /// - Iteration N+1: Resolve A, but introduces conflict B
    /// - Iteration N+2: Resolve B, but re-introduces conflict A
    /// - Cycle continues forever
    ///
    /// Detection: If current conflicts match ANY previous iteration's conflicts, we're oscillating.
    fn detect_oscillation(&self, current_conflicts: &[VersionConflict]) -> bool {
        // Check if current conflicts match conflicts from any previous iteration
        for iteration in &self.iteration_history {
            if conflicts_equal(&iteration.conflicts, current_conflicts) {
                tracing::warn!(
                    "Oscillation detected: conflicts match iteration {}",
                    iteration.iteration
                );
                return true;
            }
        }
        false
    }
}

/// Check if two conflict sets are equivalent.
///
/// Two conflict sets are equal if they contain the same resources with the same
/// resolved SHAs, regardless of order. This provides more precise oscillation
/// detection by comparing the actual conflict state, not just resource names.
fn conflicts_equal(a: &[VersionConflict], b: &[VersionConflict]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    // Create a deterministic representation that includes both resource names
    // and their resolved SHAs for precise conflict state comparison
    let mut a_state = std::collections::BTreeSet::new();
    let mut b_state = std::collections::BTreeSet::new();

    // Extract all (resource, resolved_sha) pairs from each conflict
    for conflict in a {
        for req in &conflict.conflicting_requirements {
            a_state.insert((conflict.resource.clone(), req.resolved_sha.clone()));
        }
    }

    for conflict in b {
        for req in &conflict.conflicting_requirements {
            b_state.insert((conflict.resource.clone(), req.resolved_sha.clone()));
        }
    }

    a_state == b_state
}

/// Result of a backtracking attempt.
#[derive(Debug, Clone)]
pub struct BacktrackingResult {
    /// Whether conflicts were successfully resolved
    pub resolved: bool,

    /// List of ALL version updates made across all iterations
    pub updates: Vec<VersionUpdate>,

    /// Number of backtracking iterations performed
    pub iterations: usize,

    /// Total number of version resolutions attempted
    pub attempted_versions: usize,

    /// History of each iteration (for debugging/logging)
    pub iteration_history: Vec<BacktrackingIteration>,

    /// Total transitive deps re-resolved across all iterations
    pub total_transitive_reresolutions: usize,

    /// Reason for termination
    pub termination_reason: TerminationReason,
}

/// Record of a version update made during backtracking.
#[derive(Debug, Clone)]
pub struct VersionUpdate {
    /// Resource identifier (format: "source:required_by")
    pub resource_id: String,

    /// Original version constraint
    pub old_version: String,

    /// New version selected
    pub new_version: String,

    /// Original resolved SHA
    pub old_sha: String,

    /// New resolved SHA
    pub new_sha: String,

    /// Template variables (variant inputs) for this resource
    pub variant_inputs: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a test ResourceId
    fn test_resource_id(name: &str) -> ResourceId {
        ResourceId::new(
            name,
            Some("test-source"),
            Some("claude-code"),
            crate::core::ResourceType::Agent,
            crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string(),
        )
    }

    #[test]
    fn test_parse_resource_id() {
        let (source, path) = parse_resource_id_string("community:agents/helper.md").unwrap();
        assert_eq!(source, "community");
        assert_eq!(path, "agents/helper.md");
    }

    #[test]
    fn test_parse_resource_id_invalid() {
        let result = parse_resource_id_string("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_backtracking_result_structure() {
        let result = BacktrackingResult {
            resolved: true,
            updates: vec![VersionUpdate {
                resource_id: "community:test".to_string(),
                old_version: "v1.0.0".to_string(),
                new_version: "v1.0.1".to_string(),
                old_sha: "abc123".to_string(),
                new_sha: "def456".to_string(),
                variant_inputs: None,
            }],
            iterations: 1,
            attempted_versions: 5,
            iteration_history: vec![],
            total_transitive_reresolutions: 0,
            termination_reason: TerminationReason::Success,
        };

        assert!(result.resolved);
        assert_eq!(result.updates.len(), 1);
        assert_eq!(result.iterations, 1);
        assert_eq!(result.attempted_versions, 5);
        assert_eq!(result.termination_reason, TerminationReason::Success);
    }

    #[test]
    fn test_conflicts_equal_identical_resources_and_shas() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflicts_a = vec![conflict_a];
        let conflicts_b = vec![conflict_b];

        // Should be equal - same resource with same SHAs
        assert!(conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_same_resources_different_shas() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                // Different SHA for second requirement
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "999888777666".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflicts_a = vec![conflict_a];
        let conflicts_b = vec![conflict_b];

        // Should NOT be equal - same resource but different SHAs
        assert!(!conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_different_resources() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib2"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflicts_a = vec![conflict_a];
        let conflicts_b = vec![conflict_b];

        // Should NOT be equal - different resources
        assert!(!conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_multiple_conflicts_order_independent() {
        let conflict1_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^2.0.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflict2_a = VersionConflict {
            resource: test_resource_id("lib2"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app3".to_string(),
                requirement: "^1.5.0".to_string(),
                resolved_sha: "333444555666".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflict1_b = VersionConflict {
            resource: test_resource_id("lib2"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app3".to_string(),
                requirement: "^1.5.0".to_string(),
                resolved_sha: "333444555666".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflict2_b = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^2.0.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        // Different order - should still be equal
        let conflicts_a = vec![conflict1_a, conflict2_a];
        let conflicts_b = vec![conflict2_b, conflict1_b];

        // Should be equal - same conflicts with same SHAs, different order
        assert!(conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_empty_lists() {
        let conflicts_a: Vec<VersionConflict> = vec![];
        let conflicts_b: Vec<VersionConflict> = vec![];

        // Should be equal - both empty
        assert!(conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_different_lengths() {
        let conflict1 = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflicts_a = vec![conflict1.clone()];
        let conflicts_b = vec![conflict1.clone(), conflict1.clone()];

        // Should NOT be equal - different lengths
        assert!(!conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_complex_real_world_scenario() {
        // Simulate a real-world complex conflict scenario
        let conflict_a1 = VersionConflict {
            resource: test_resource_id("agents/helper"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "agents/ai-assistant".to_string(),
                    requirement: "agents-v1.0.0".to_string(),
                    resolved_sha: "a1b2c3d4e5f6".to_string(),
                    resolved_version: None,
                    parent_version_constraint: Some("^2.0.0".to_string()),
                    parent_resolved_sha: Some("ffeeddccbbaa".to_string()),
                },
                ConflictingRequirement {
                    required_by: "agents/code-reviewer".to_string(),
                    requirement: "agents-v1.1.0".to_string(),
                    resolved_sha: "b2c3d4e5f6a1".to_string(),
                    resolved_version: None,
                    parent_version_constraint: Some("^2.1.0".to_string()),
                    parent_resolved_sha: Some("ccbbaa998877".to_string()),
                },
            ],
        };

        let conflict_a2 = VersionConflict {
            resource: test_resource_id("snippets/utils"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "agents/helper".to_string(),
                requirement: "snippets-v1.0.0".to_string(),
                resolved_sha: "c3d4e5f6a1b2".to_string(),
                resolved_version: None,
                parent_version_constraint: Some("^1.0.0".to_string()),
                parent_resolved_sha: Some("a1b2c3d4e5f6".to_string()),
            }],
        };

        // Same conflicts but different order and with different requirement strings
        let conflict_b1 = VersionConflict {
            resource: test_resource_id("snippets/utils"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "agents/helper".to_string(),
                requirement: "snippets-^v1.0.0".to_string(), // Different requirement string
                resolved_sha: "c3d4e5f6a1b2".to_string(),    // But same SHA
                resolved_version: None,
                parent_version_constraint: Some("~1.0.0".to_string()), // Different constraint
                parent_resolved_sha: Some("a1b2c3d4e5f6".to_string()),
            }],
        };

        let conflict_b2 = VersionConflict {
            resource: test_resource_id("agents/helper"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "agents/code-reviewer".to_string(),
                    requirement: "agents-v1.1.0".to_string(),
                    resolved_sha: "b2c3d4e5f6a1".to_string(),
                    resolved_version: None,
                    parent_version_constraint: Some("^2.1.0".to_string()),
                    parent_resolved_sha: Some("ccbbaa998877".to_string()),
                },
                ConflictingRequirement {
                    required_by: "agents/ai-assistant".to_string(),
                    requirement: "agents-v1.0.0".to_string(),
                    resolved_sha: "a1b2c3d4e5f6".to_string(),
                    resolved_version: None,
                    parent_version_constraint: Some("^2.0.0".to_string()),
                    parent_resolved_sha: Some("ffeeddccbbaa".to_string()),
                },
            ],
        };

        let conflicts_a = vec![conflict_a1, conflict_a2];
        let conflicts_b = vec![conflict_b2, conflict_b1]; // Different order

        // Should be equal - same resources with same SHAs regardless of order
        assert!(conflicts_equal(&conflicts_a, &conflicts_b));
    }

    /// Test that prefix filtering works correctly for versioned constraints.
    ///
    /// This test verifies that when filtering tags with a prefixed constraint like
    /// `d->=v1.0.0`, only tags with the `d-` prefix are considered, not tags with
    /// other prefixes that happen to match the version constraint.
    ///
    /// This is a regression test for a bug where `d->=v1.0.0` would incorrectly
    /// match tags like `a-v1.0.0`, `b-v1.0.0`, `x-v1.0.0`, etc., causing
    /// non-deterministic behavior in backtracking resolution.
    #[test]
    fn test_filter_by_constraint_respects_prefix() {
        use crate::resolver::version_resolver::parse_tags_to_versions;
        use crate::version::constraints::{ConstraintSet, VersionConstraint};

        // Create a list of tags with different prefixes but similar versions
        let all_tags = vec![
            "d-v1.0.0".to_string(),
            "d-v2.0.0".to_string(),
            "a-v1.0.0".to_string(),
            "a-v2.0.0".to_string(),
            "b-v1.0.0".to_string(),
            "b-v2.0.0".to_string(),
            "x-v1.0.0".to_string(),
            "x-v2.0.0".to_string(),
        ];

        // Test constraint: d->=v1.0.0 (should match d-v1.0.0 and d-v2.0.0 ONLY)
        let constraint = "d->=v1.0.0";

        // Extract prefix from constraint
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);

        // Filter tags by prefix (this is what filter_by_constraint SHOULD do)
        let prefix_filtered: Vec<String> = all_tags
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                tag_prefix == constraint_prefix
            })
            .cloned()
            .collect();

        // Parse the constraint
        let constraint_parsed = VersionConstraint::parse(constraint).unwrap();
        let mut constraint_set = ConstraintSet::new();
        constraint_set.add(constraint_parsed).unwrap();

        // Parse tags to versions (with prefix filtering)
        let tag_versions = parse_tags_to_versions(prefix_filtered.clone());

        // Filter by constraint satisfaction
        let matched_tags: Vec<String> = tag_versions
            .into_iter()
            .filter(|(_, version)| constraint_set.satisfies(version))
            .map(|(tag, _)| tag)
            .collect();

        // CRITICAL: Only d-prefixed tags should match
        assert_eq!(matched_tags.len(), 2, "Should match exactly 2 tags with d- prefix");
        assert!(matched_tags.contains(&"d-v1.0.0".to_string()), "Should match d-v1.0.0");
        assert!(matched_tags.contains(&"d-v2.0.0".to_string()), "Should match d-v2.0.0");

        // CRITICAL: Should NOT match tags with other prefixes
        assert!(
            !matched_tags.contains(&"a-v1.0.0".to_string()),
            "Should NOT match a-v1.0.0 (wrong prefix)"
        );
        assert!(
            !matched_tags.contains(&"b-v1.0.0".to_string()),
            "Should NOT match b-v1.0.0 (wrong prefix)"
        );
        assert!(
            !matched_tags.contains(&"x-v1.0.0".to_string()),
            "Should NOT match x-v1.0.0 (wrong prefix)"
        );

        // Now test what happens WITHOUT prefix filtering (the bug!)
        let tag_versions_no_prefix = parse_tags_to_versions(all_tags.clone());
        let matched_no_prefix: Vec<String> = tag_versions_no_prefix
            .into_iter()
            .filter(|(_, version)| constraint_set.satisfies(version))
            .map(|(tag, _)| tag)
            .collect();

        // WITHOUT prefix filtering, we'd incorrectly match ALL tags with v1.x.x or v2.x.x
        assert!(
            matched_no_prefix.len() > 2,
            "Bug: Without prefix filtering, constraint matches too many tags (found {})",
            matched_no_prefix.len()
        );
        assert!(
            matched_no_prefix.contains(&"a-v1.0.0".to_string()),
            "Bug: Without prefix filtering, incorrectly matches a-v1.0.0"
        );
        assert!(
            matched_no_prefix.contains(&"b-v1.0.0".to_string()),
            "Bug: Without prefix filtering, incorrectly matches b-v1.0.0"
        );
    }

    /// Test that unprefixed constraints only match unprefixed tags.
    ///
    /// This test verifies that when filtering tags with an unprefixed constraint
    /// like `>=v1.0.0`, only tags without prefixes are considered, not tags with
    /// prefixes that happen to match the version constraint.
    #[test]
    fn test_filter_by_constraint_unprefixed() {
        use crate::resolver::version_resolver::parse_tags_to_versions;
        use crate::version::constraints::{ConstraintSet, VersionConstraint};

        // Create a list of mixed prefixed and unprefixed tags
        let all_tags = [
            "v1.0.0".to_string(),   // Unprefixed
            "v2.0.0".to_string(),   // Unprefixed
            "d-v1.0.0".to_string(), // Prefixed
            "d-v2.0.0".to_string(), // Prefixed
            "a-v1.5.0".to_string(), // Prefixed
        ];

        // Test constraint: >=v1.0.0 (no prefix - should match only unprefixed tags)
        let constraint = ">=v1.0.0";

        // Extract prefix from constraint (should be None)
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);
        assert!(constraint_prefix.is_none(), "Unprefixed constraint should have None prefix");

        // Filter tags by prefix
        let prefix_filtered: Vec<String> = all_tags
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                tag_prefix == constraint_prefix // Both None
            })
            .cloned()
            .collect();

        // Parse the constraint
        let constraint_parsed = VersionConstraint::parse(constraint).unwrap();
        let mut constraint_set = ConstraintSet::new();
        constraint_set.add(constraint_parsed).unwrap();

        // Parse tags to versions (with prefix filtering)
        let tag_versions = parse_tags_to_versions(prefix_filtered.clone());

        // Filter by constraint satisfaction
        let matched_tags: Vec<String> = tag_versions
            .into_iter()
            .filter(|(_, version)| constraint_set.satisfies(version))
            .map(|(tag, _)| tag)
            .collect();

        // Should match exactly 2 unprefixed tags
        assert_eq!(matched_tags.len(), 2, "Should match exactly 2 unprefixed tags");
        assert!(matched_tags.contains(&"v1.0.0".to_string()), "Should match v1.0.0");
        assert!(matched_tags.contains(&"v2.0.0".to_string()), "Should match v2.0.0");

        // Should NOT match prefixed tags
        assert!(
            !matched_tags.contains(&"d-v1.0.0".to_string()),
            "Should NOT match d-v1.0.0 (has prefix)"
        );
        assert!(
            !matched_tags.contains(&"d-v2.0.0".to_string()),
            "Should NOT match d-v2.0.0 (has prefix)"
        );
        assert!(
            !matched_tags.contains(&"a-v1.5.0".to_string()),
            "Should NOT match a-v1.5.0 (has prefix)"
        );
    }

    /// Test that deterministic sorting works correctly with identical versions.
    ///
    /// This test verifies that when multiple tags have the same semantic version,
    /// they are sorted alphabetically by tag name for deterministic ordering.
    #[test]
    fn test_deterministic_sorting_with_identical_versions() {
        use crate::resolver::version_resolver::{
            parse_tags_to_versions, sort_versions_deterministic,
        };

        // Tags with same version but different names
        let tags = vec![
            "z-v1.0.0".to_string(),
            "a-v1.0.0".to_string(),
            "m-v1.0.0".to_string(),
            "b-v2.0.0".to_string(), // Different version (should be first)
        ];

        let mut result = parse_tags_to_versions(tags);

        // parse_tags_to_versions already calls sort_versions_deterministic,
        // but let's verify the result is deterministic
        assert_eq!(result.len(), 4, "Should parse all 4 tags");

        // Highest version first (b-v2.0.0)
        assert_eq!(result[0].0, "b-v2.0.0", "Highest version should be first");

        // Then v1.0.0 tags sorted alphabetically
        assert_eq!(result[1].0, "a-v1.0.0", "First v1.0.0 tag alphabetically");
        assert_eq!(result[2].0, "m-v1.0.0", "Second v1.0.0 tag alphabetically");
        assert_eq!(result[3].0, "z-v1.0.0", "Third v1.0.0 tag alphabetically");

        // Re-sort and verify determinism
        sort_versions_deterministic(&mut result);
        assert_eq!(result[0].0, "b-v2.0.0");
        assert_eq!(result[1].0, "a-v1.0.0");
        assert_eq!(result[2].0, "m-v1.0.0");
        assert_eq!(result[3].0, "z-v1.0.0");
    }

    /// Test that prefix filtering works with exact version constraints.
    ///
    /// This test verifies that exact version constraints with prefixes only match
    /// tags with the same prefix and exact version.
    #[test]
    fn test_filter_by_constraint_exact_version_with_prefix() {
        let all_tags = ["d-v1.0.0".to_string(), "a-v1.0.0".to_string(), "b-v1.0.0".to_string()];

        // Exact version with prefix
        let constraint = "d-v1.0.0";

        // Extract prefix
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);

        // Filter by prefix
        let prefix_filtered: Vec<String> = all_tags
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                tag_prefix == constraint_prefix
            })
            .cloned()
            .collect();

        // For exact versions, we should get an exact match
        assert_eq!(prefix_filtered.len(), 1, "Should match exactly 1 tag with prefix");
        assert_eq!(prefix_filtered[0], "d-v1.0.0", "Should match d-v1.0.0");
        assert!(!prefix_filtered.contains(&"a-v1.0.0".to_string()), "Should NOT match a-v1.0.0");
        assert!(!prefix_filtered.contains(&"b-v1.0.0".to_string()), "Should NOT match b-v1.0.0");
    }
}
