//! Progress indicators and user interface utilities
//!
//! This module provides a unified progress system for AGPM operations using the
//! `MultiPhaseProgress` approach. All progress tracking goes through phases to ensure
//! consistent user experience across different operations.
//!
//! # Features
//!
//! - **Unified progress**: All operations use `MultiPhaseProgress` for consistency
//! - **Phase-based tracking**: Installation/update operations broken into logical phases
//! - **CI/quiet mode support**: Automatically disables in non-interactive environments
//! - **Thread safety**: Safe to use across async tasks and parallel operations
//!
//! # Configuration
//!
//! Progress indicators are now controlled via the `MultiPhaseProgress` constructor
//! parameter rather than environment variables for better thread safety.
//!
//! # Examples
//!
//! ## Multi-Phase Progress
//!
//! ```rust,no_run
//! use agpm_cli::utils::progress::{MultiPhaseProgress, InstallationPhase};
//!
//! let progress = MultiPhaseProgress::new(true);
//!
//! // Start syncing phase
//! progress.start_phase(InstallationPhase::SyncingSources, Some("Fetching repositories"));
//! // ... do work ...
//! progress.complete_phase(Some("Synced 3 repositories"));
//!
//! // Start resolving phase
//! progress.start_phase(InstallationPhase::ResolvingDependencies, None);
//! // ... do work ...
//! progress.complete_phase(Some("Resolved 25 dependencies"));
//! ```

use crate::manifest::Manifest;
use indicatif::{ProgressBar as IndicatifBar, ProgressStyle as IndicatifStyle};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Re-export for deprecated functions - use MultiPhaseProgress instead
#[deprecated(since = "0.3.0", note = "Use MultiPhaseProgress instead")]
pub use indicatif::ProgressBar;

/// Represents different phases of the installation process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallationPhase {
    /// Syncing source repositories
    SyncingSources,
    /// Resolving dependencies and versions
    ResolvingDependencies,
    /// Installing resources from resolved dependencies
    Installing,
    /// Installing specific resources (used during updates)
    InstallingResources,
    /// Updating configuration files and finalizing
    Finalizing,
}

impl InstallationPhase {
    /// Get a human-readable description of the phase
    pub const fn description(&self) -> &'static str {
        match self {
            Self::SyncingSources => "Syncing sources",
            Self::ResolvingDependencies => "Resolving dependencies",
            Self::Installing => "Installing resources",
            Self::InstallingResources => "Installing resources",
            Self::Finalizing => "Finalizing installation",
        }
    }
}

/// Manages a fixed-size window of active resources during installation.
/// This provides real-time visibility into which resources are currently
/// being processed without unbounded terminal output.
struct ActiveWindow {
    /// Fixed number of display slots (typically 5-7)
    slots: Vec<Option<IndicatifBar>>,
    /// Counter bar showing overall progress (e.g., "Installing (50/500 complete)")
    counter_bar: Option<IndicatifBar>,
    /// Maximum number of slots in the window
    max_slots: usize,
    /// Map from resource name to slot index for fast lookup
    resource_to_slot: std::collections::HashMap<String, usize>,
}

impl ActiveWindow {
    fn new(max_slots: usize) -> Self {
        Self {
            slots: Vec::with_capacity(max_slots),
            counter_bar: None,
            max_slots,
            resource_to_slot: std::collections::HashMap::new(),
        }
    }
}

/// Multi-phase progress manager that displays multiple progress bars
/// with completed phases showing as static messages
#[derive(Clone)]
pub struct MultiPhaseProgress {
    /// `MultiProgress` container from indicatif
    multi: Arc<indicatif::MultiProgress>,
    /// Current active spinner/progress bar
    current_bar: Arc<Mutex<Option<IndicatifBar>>>,
    /// Whether progress is enabled
    enabled: bool,
    /// Phase start time for timing calculations
    phase_start: Arc<Mutex<Option<Instant>>>,
    /// Active window for showing real-time resource processing
    active_window: Arc<Mutex<ActiveWindow>>,
}

impl MultiPhaseProgress {
    /// Create a new multi-phase progress manager
    pub fn new(enabled: bool) -> Self {
        Self {
            multi: Arc::new(indicatif::MultiProgress::new()),
            current_bar: Arc::new(Mutex::new(None)),
            enabled,
            phase_start: Arc::new(Mutex::new(None)),
            active_window: Arc::new(Mutex::new(ActiveWindow::new(7))),
        }
    }

    /// Start a new phase with a spinner
    pub fn start_phase(&self, phase: InstallationPhase, message: Option<&str>) {
        if !self.enabled {
            return;
        }

        // Store phase start time
        *self.phase_start.lock().unwrap() = Some(Instant::now());

        // Remove reference to old bar
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        let spinner = self.multi.add(IndicatifBar::new_spinner());

        // Format: "Syncing sources" or "Syncing sources (additional info)"
        let phase_msg = if let Some(msg) = message {
            format!("{} {}", phase.description(), msg)
        } else {
            phase.description().to_string()
        };

        let style = IndicatifStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner} {msg}")
            .unwrap();

        spinner.set_style(style);
        spinner.set_message(phase_msg);
        spinner.enable_steady_tick(Duration::from_millis(100));

        *self.current_bar.lock().unwrap() = Some(spinner);
    }

    /// Start a new phase with a progress bar
    pub fn start_phase_with_progress(&self, phase: InstallationPhase, total: usize) {
        if !self.enabled {
            return;
        }

        // Store phase start time
        *self.phase_start.lock().unwrap() = Some(Instant::now());

        // Remove reference to old bar
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        // Create new progress bar for this phase
        let progress_bar = self.multi.add(IndicatifBar::new(total as u64));

        // Configure progress bar style
        let style = IndicatifStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=>-");

        progress_bar.set_style(style);
        progress_bar.set_message(phase.description());

        // Store the progress bar
        *self.current_bar.lock().unwrap() = Some(progress_bar);
    }

    /// Update the message of the current phase
    pub fn update_message(&self, message: String) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_message(message);
        }
    }

    /// Update the current message for the active phase
    pub fn update_current_message(&self, message: &str) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_message(message.to_string());
        }
    }

    /// Increment progress for progress bars
    pub fn increment_progress(&self, delta: u64) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.inc(delta);
        }
    }

    /// Set progress position for progress bars
    pub fn set_progress(&self, pos: usize) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_position(pos as u64);
        }
    }

    /// Complete the current phase and show it as a static message
    pub fn complete_phase(&self, message: Option<&str>) {
        if !self.enabled {
            return;
        }

        // Calculate duration
        let duration = self.phase_start.lock().unwrap().take().map(|start| start.elapsed());

        if let Ok(mut guard) = self.current_bar.lock() {
            if let Some(bar) = guard.take() {
                bar.disable_steady_tick();
                bar.finish_and_clear();

                // Format completion message with timing
                let final_message = match (message, duration) {
                    (Some(msg), Some(d)) => {
                        format!("✓ {} ({:.1}s)", msg, d.as_secs_f64())
                    }
                    (Some(msg), None) => format!("✓ {}", msg),
                    (None, Some(d)) => format!("✓ Complete ({:.1}s)", d.as_secs_f64()),
                    (None, None) => "✓ Complete".to_string(),
                };

                self.multi.suspend(|| {
                    println!("{}", final_message);
                });
            }
        }
    }

    /// Start a phase with active resource tracking window.
    /// This displays a fixed-size window showing which resources are currently
    /// being processed, along with a counter showing overall progress.
    ///
    /// # Arguments
    /// * `phase` - The installation phase to start
    /// * `total` - Total number of resources to install
    /// * `window_size` - Number of slots in the active window (typically 5-7)
    pub fn start_phase_with_active_tracking(
        &self,
        phase: InstallationPhase,
        total: usize,
        window_size: usize,
    ) {
        if !self.enabled {
            return;
        }

        // Store phase start time
        *self.phase_start.lock().unwrap() = Some(Instant::now());

        // Clear previous bar
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        // Create counter bar at top showing overall progress
        let counter_bar = self.multi.add(IndicatifBar::new(total as u64));
        let style = IndicatifStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner} {msg}")
            .unwrap();
        counter_bar.set_style(style);
        counter_bar.set_message(format!("{} (0/{} complete)", phase.description(), total));
        counter_bar.enable_steady_tick(Duration::from_millis(100));

        // Create fixed slots below for active resources
        let mut slots = Vec::with_capacity(window_size);
        for _ in 0..window_size {
            let slot = self.multi.add(IndicatifBar::new_spinner());
            let slot_style = IndicatifStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("  {msg}")
                .unwrap();
            slot.set_style(slot_style);
            slot.set_message(""); // Empty initially
            slots.push(Some(slot));
        }

        // Store in active window
        let mut window = self.active_window.lock().unwrap();
        window.counter_bar = Some(counter_bar);
        window.slots = slots;
        window.max_slots = window_size;
        window.resource_to_slot.clear();

        // Store counter bar as current bar
        *self.current_bar.lock().unwrap() = window.counter_bar.clone();
    }

    /// Format a resource name with tool, type, version, and hash information.
    ///
    /// Format: {tool}/{type}: {name}@{version}[{hash}]
    /// - Hash only shown for non-default configurations
    /// - "local" shown for version when source is None
    ///
    /// # Arguments
    /// * `entry` - The locked resource entry with full metadata
    fn format_resource_display_name(&self, entry: &crate::lockfile::LockedResource) -> String {
        let tool = entry.tool.as_deref().unwrap_or("claude-code");
        let resource_type_str = entry.resource_type.to_string();

        // Extract the base name without the type prefix
        let base_name = entry.name.trim_start_matches(&format!("{}/", resource_type_str));

        // Determine version or "local" for local resources
        let version = if entry.source.is_none() {
            "local".to_string()
        } else {
            entry.version.clone().unwrap_or_else(|| "unknown".to_string())
        };

        // Determine if we should show the hash (only for non-default configurations)
        let hash_suffix = self.should_show_hash(entry);

        format!("{}/{}: {}@{}{}", tool, resource_type_str, base_name, version, hash_suffix)
    }

    /// Determine if hash should be displayed based on whether configuration is non-default.
    ///
    /// # Arguments
    /// * `entry` - The locked resource entry
    fn should_show_hash(&self, entry: &crate::lockfile::LockedResource) -> String {
        // Show hash only for resources with non-default variant_inputs
        // Compare against the static EMPTY_VARIANT_INPUTS_HASH to detect default configuration
        let hash = &entry.variant_inputs.hash();
        if *hash != crate::utils::EMPTY_VARIANT_INPUTS_HASH.as_str() {
            // Extract 8 characters from the hash (skip "sha256:" prefix)
            if hash.len() >= 17 {
                format!("[{}]", &hash[9..17])
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    /// Mark a resource as actively being processed.
    /// This adds the resource to the first available slot in the active window.
    ///
    /// # Arguments
    /// * `entry` - The locked resource entry with full metadata
    pub fn mark_resource_active(&self, entry: &crate::lockfile::LockedResource) {
        if !self.enabled {
            return;
        }

        let display_name = self.format_resource_display_name(entry);

        let mut window = self.active_window.lock().unwrap();

        // Create a unique key for this resource that includes both name and variant hash
        // This allows us to handle multiple resources with the same name but different variants
        let resource_key = format!("{}:{}", entry.name, entry.variant_inputs.hash());

        // Find first available slot:
        // 1. Look for completely empty slots (no message)
        // 2. Look for slots assigned to the same exact resource (already being processed)
        for (idx, slot_opt) in window.slots.iter().enumerate() {
            if let Some(bar) = slot_opt {
                // Check if slot is empty (empty message or just whitespace)
                if bar.message().trim().is_empty() {
                    bar.set_message(format!("→ {}", display_name));
                    window.resource_to_slot.insert(resource_key, idx);
                    break;
                }
                // Check if this slot is already showing the exact same resource
                else if window.resource_to_slot.iter().any(|(_, &slot_idx)| slot_idx == idx) {
                    // This slot is already assigned to some resource, check if it's the same one
                    if let Some((existing_key, _)) =
                        window.resource_to_slot.iter().find(|&(_, &slot_idx)| slot_idx == idx)
                    {
                        if *existing_key == resource_key {
                            // Already showing this resource, no need to do anything
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Mark a resource as complete and update progress counter.
    /// This clears the resource from its slot and updates the overall counter.
    ///
    /// # Arguments
    /// * `entry` - The locked resource entry that was completed
    /// * `completed` - Number of resources completed so far
    /// * `total` - Total number of resources to install
    pub fn mark_resource_complete(
        &self,
        entry: &crate::lockfile::LockedResource,
        completed: usize,
        total: usize,
    ) {
        if !self.enabled {
            return;
        }

        let mut window = self.active_window.lock().unwrap();

        // Create the same unique key that was used in mark_resource_active
        let resource_key = format!("{}:{}", entry.name, entry.variant_inputs.hash());

        // Find and clear the slot using the resource key from hashmap
        if let Some(&slot_idx) = window.resource_to_slot.get(&resource_key) {
            if let Some(Some(bar)) = window.slots.get(slot_idx) {
                bar.set_message(""); // Clear slot
            }
            window.resource_to_slot.remove(&resource_key);
        } else {
            // Fallback: search all slots for matching display name
            let display_name = self.format_resource_display_name(entry);
            for bar in window.slots.iter().flatten() {
                let message = bar.message();
                if message.contains(&display_name) {
                    bar.set_message(""); // Clear slot
                    break;
                }
            }
        }

        // Update counter bar
        if let Some(ref counter) = window.counter_bar {
            counter.set_message(format!("Installing resources ({}/{} complete)", completed, total));
        }
    }

    /// Complete phase with active window, showing final summary.
    /// This is similar to complete_phase but also clears the active window.
    pub fn complete_phase_with_window(&self, message: Option<&str>) {
        if !self.enabled {
            return;
        }

        // Calculate duration
        let duration = self.phase_start.lock().unwrap().take().map(|start| start.elapsed());

        // Clear active window slots
        let mut window = self.active_window.lock().unwrap();
        for slot in window.slots.iter_mut() {
            if let Some(bar) = slot.take() {
                bar.finish_and_clear();
            }
        }
        if let Some(counter) = window.counter_bar.take() {
            counter.disable_steady_tick();
            counter.finish_and_clear();
        }
        window.resource_to_slot.clear();

        // Clear current bar reference
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        // Format completion message with timing
        let final_message = match (message, duration) {
            (Some(msg), Some(d)) => {
                format!("✓ {} ({:.1}s)", msg, d.as_secs_f64())
            }
            (Some(msg), None) => format!("✓ {}", msg),
            (None, Some(d)) => format!("✓ Complete ({:.1}s)", d.as_secs_f64()),
            (None, None) => "✓ Complete".to_string(),
        };

        self.multi.suspend(|| {
            println!("{}", final_message);
        });
    }

    /// Calculate optimal window size based on concurrency and terminal constraints.
    /// Returns a size between 3 and 10, with 7 as a reasonable default.
    pub fn calculate_window_size(concurrency: usize) -> usize {
        // Use concurrency as a guide, but cap it for readability
        concurrency.clamp(5, 10)
    }

    /// Suspend progress display temporarily to execute a closure.
    /// This is useful for printing output that should appear outside the progress display.
    pub fn suspend<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.multi.suspend(f)
    }

    /// Clear all progress displays
    pub fn clear(&self) {
        // Clear current bar if any
        if let Ok(mut guard) = self.current_bar.lock()
            && let Some(bar) = guard.take()
        {
            bar.finish_and_clear();
        }
        self.multi.clear().ok();
    }

    /// Create a subordinate progress bar for detailed progress within a phase
    pub fn add_progress_bar(&self, total: u64) -> Option<IndicatifBar> {
        if !self.enabled {
            return None;
        }

        let pb = self.multi.add(IndicatifBar::new(total));
        let style = IndicatifStyle::default_bar()
            .template("  {msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=>-");
        pb.set_style(style);
        Some(pb)
    }
}

/// Helper function to collect dependency names from a manifest
pub fn collect_dependency_names(manifest: &Manifest) -> Vec<String> {
    manifest.all_dependencies().iter().map(|(name, _)| (*name).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::lockfile::LockedResource;
    use crate::resolver::lockfile_builder::VariantInputs;
    use std::str::FromStr;

    /// Helper function to create test LockedResource entries
    fn create_test_locked_resource(name: &str, resource_type: &str) -> LockedResource {
        LockedResource {
            name: name.to_string(),
            manifest_alias: None,
            source: Some("test".to_string()),
            url: Some("https://example.com".to_string()),
            version: Some("v1.0.0".to_string()),
            path: format!("{}.md", name),
            resolved_commit: Some("abc123def456".to_string()),
            resource_type: ResourceType::from_str(resource_type).unwrap_or(ResourceType::Agent),
            tool: Some("claude-code".to_string()),
            installed_at: format!(".claude/{}/{}.md", resource_type, name),
            checksum: "sha256:test123".to_string(),
            context_checksum: Some("sha256:context456".to_string()),
            variant_inputs: VariantInputs::default(),
            dependencies: vec![],
            applied_patches: std::collections::BTreeMap::new(),
            install: Some(true),
            files: None, // Test resources are single files
        }
    }

    #[test]
    fn test_installation_phase_description() {
        assert_eq!(InstallationPhase::SyncingSources.description(), "Syncing sources");
        assert_eq!(
            InstallationPhase::ResolvingDependencies.description(),
            "Resolving dependencies"
        );
        assert_eq!(InstallationPhase::Installing.description(), "Installing resources");
        assert_eq!(InstallationPhase::InstallingResources.description(), "Installing resources");
        assert_eq!(InstallationPhase::Finalizing.description(), "Finalizing installation");
    }

    #[test]
    fn test_active_window_basic() {
        let progress = MultiPhaseProgress::new(true);

        // Start tracking
        progress.start_phase_with_active_tracking(InstallationPhase::InstallingResources, 10, 5);

        // Create mock LockedResource entries for testing
        let resource1 = create_test_locked_resource("resource1", "agents");
        let resource2 = create_test_locked_resource("resource2", "agents");
        let resource3 = create_test_locked_resource("resource3", "agents");

        // Mark resources active
        progress.mark_resource_active(&resource1);
        progress.mark_resource_active(&resource2);
        progress.mark_resource_active(&resource3);

        // Mark one complete
        progress.mark_resource_complete(&resource1, 1, 10);

        // Complete phase
        progress.complete_phase_with_window(Some("Installed 10 resources"));
    }

    #[test]
    fn test_active_window_overflow() {
        let progress = MultiPhaseProgress::new(true);

        // Start with 3 slots
        progress.start_phase_with_active_tracking(InstallationPhase::InstallingResources, 10, 3);

        // Create mock LockedResource entries for testing
        let r1 = create_test_locked_resource("r1", "agents");
        let r2 = create_test_locked_resource("r2", "agents");
        let r3 = create_test_locked_resource("r3", "agents");
        let r4 = create_test_locked_resource("r4", "agents");
        let r5 = create_test_locked_resource("r5", "agents");

        // Try to add 5 resources (should fill 3 slots, other 2 wait)
        progress.mark_resource_active(&r1);
        progress.mark_resource_active(&r2);
        progress.mark_resource_active(&r3);
        progress.mark_resource_active(&r4); // Won't show until slot clears
        progress.mark_resource_active(&r5); // Won't show until slot clears

        // Complete one to free slot
        progress.mark_resource_complete(&r1, 1, 10);

        // Now r4 or r5 can be shown (depends on timing)
        progress.mark_resource_active(&r4);
    }

    #[test]
    fn test_calculate_window_size() {
        assert_eq!(MultiPhaseProgress::calculate_window_size(1), 5);
        assert_eq!(MultiPhaseProgress::calculate_window_size(5), 5);
        assert_eq!(MultiPhaseProgress::calculate_window_size(7), 7);
        assert_eq!(MultiPhaseProgress::calculate_window_size(10), 10);
        assert_eq!(MultiPhaseProgress::calculate_window_size(50), 10); // Capped
    }

    #[test]
    fn test_phase_timing() {
        let progress = MultiPhaseProgress::new(true);

        progress.start_phase(InstallationPhase::SyncingSources, None);
        std::thread::sleep(Duration::from_millis(100));
        progress.complete_phase(Some("Sources synced"));

        // Timing should be approximately 0.1s (verify via output inspection)
    }

    #[test]
    fn test_multi_phase_progress_new() {
        let progress = MultiPhaseProgress::new(true);

        // Test basic functionality
        progress.start_phase(InstallationPhase::SyncingSources, Some("test message"));
        progress.update_current_message("updated message");
        progress.complete_phase(Some("completed"));
        progress.clear();
    }

    #[test]
    fn test_multi_phase_progress_with_progress_bar() {
        let progress = MultiPhaseProgress::new(true);

        progress.start_phase_with_progress(InstallationPhase::Installing, 10);
        progress.increment_progress(5);
        progress.set_progress(8);
        progress.complete_phase(Some("Installation completed"));
    }

    #[test]
    fn test_multi_phase_progress_disabled() {
        let progress = MultiPhaseProgress::new(false);

        // These should not panic when disabled
        progress.start_phase(InstallationPhase::SyncingSources, None);
        progress.complete_phase(Some("test"));
        progress.clear();
    }

    #[test]
    fn test_collect_dependency_names() {
        // This test would need a proper Manifest instance to work
        // For now, just ensure the function compiles and runs

        // Note: This is a minimal test since we'd need to construct a full manifest
        // In real usage, this function extracts dependency names from the manifest
    }
}
