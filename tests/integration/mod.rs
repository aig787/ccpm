//! Integration test suite for AGPM
//!
//! This test suite contains comprehensive end-to-end integration tests that verify
//! the complete functionality of AGPM commands and workflows. These tests run relatively
//! quickly and are executed in CI on every commit.
//!
//! # Running Integration Tests
//!
//! ```bash
//! cargo test --test integration
//! cargo nextest run --test integration
//! ```
//!
//! # Test Organization
//!
//! Tests are organized into logical subdirectories by functionality area:
//!
//! ## Transitive Dependencies (`transitive/`)
//! - **basic**: Basic transitive dependency resolution, diamond patterns, cycles
//! - **complex**: Complex dependency graphs and scenarios
//! - **cross_type**: Cross-type and cross-source transitive dependencies
//! - **local**: Local file transitive dependency resolution
//! - **merged**: Dependency merging and deduplication
//! - **overrides**: Direct manifest dependencies overriding transitive ones
//! - **patterns**: Pattern expansion in transitive dependencies
//! - **versions**: Version conflict and metadata resolution
//!
//! ## Lockfile Management (`lockfile/`)
//! - **checksums**: Checksum computation and validation
//! - **determinism**: Deterministic lockfile generation
//! - **migration**: Migration from older lockfile formats
//! - **stability**: Lockfile stability across operations
//! - **staleness**: Lockfile staleness detection
//!
//! ## Installation Workflows (`install/`)
//! - **basic**: Basic installation workflows (formerly deploy.rs)
//! - **cleanup**: Artifact cleanup and removal
//! - **incremental_add**: Incremental dependency addition
//! - **install_field**: Install field and content embedding functionality
//! - **multi_artifact**: Multiple artifact types
//! - **multi_resource**: Multiple resource management
//!
//! ## Template Rendering (`templating/`)
//! - **basic**: Basic template rendering
//! - **content_filter**: Content filter (`{{ 'path' | content }}`) functionality
//! - **project_vars**: Project-level template variables in transitive dependencies
//! - **resource_vars**: Resource-specific template variables with transitive dependencies
//!
//! ## Version Management (`versioning/`)
//! - **basic**: Version constraint handling
//! - **outdated**: Outdated dependency detection
//! - **prefixed**: Prefixed version tags (monorepo-style)
//! - **progress**: Update progress reporting
//!
//! ## CLI Commands (`commands/`)
//! - **config**: Configuration management (test_config)
//! - **list**: List command functionality
//! - **tree**: Dependency tree visualization
//! - **upgrade**: Self-upgrade functionality
//! - **validate**: Validation command
//!
//! ## Pattern Matching (`patterns/`)
//! - **basic**: Basic pattern matching and expansion
//! - **refresh**: Dependency refresh and update logic
//!
//! ## Configuration (`config/`)
//! - **conflicts**: Version conflict detection
//! - **hooks**: Claude Code hooks integration
//! - **patches**: Patch/override functionality
//! - **tools**: Tool enable/disable management
//!
//! ## System Infrastructure (`system/`)
//! - **cache**: Cache and worktree management
//! - **cross_platform**: Cross-platform compatibility (Windows, macOS, Linux)
//! - **errors**: Error handling and edge cases
//! - **file_url**: file:// URL support
//! - **gitignore**: .gitignore management
//! - **parallelism**: --max-parallel flag behavior

// Shared test utilities (from parent tests/ directory)
#[path = "../common/mod.rs"]
mod common;
#[path = "../fixtures/mod.rs"]
mod fixtures;

// Test configuration (used by versioning tests)
mod test_config;

mod install;
mod skills;
mod templating;
mod transitive;
mod versioning;
