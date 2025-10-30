//! Version conflict detection and reporting.
//!
//! This module handles detection and reporting of version conflicts that can occur
//! when multiple dependencies require incompatible versions of the same resource.
//! It provides detailed conflict information to help users resolve dependency issues.

use anyhow::Result;
use semver::{Op, Version, VersionReq};
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::core::AgpmError;
use pubgrub::Ranges;

/// Represents a version conflict between dependencies
#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub resource: String,
    pub conflicting_requirements: Vec<ConflictingRequirement>,
}

#[derive(Debug, Clone)]
pub struct ConflictingRequirement {
    pub required_by: String,
    pub requirement: String,
    pub resolved_version: Option<Version>,
}

impl fmt::Display for VersionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Version conflict for '{}':", self.resource)?;
        for req in &self.conflicting_requirements {
            writeln!(f, "  - {} requires {}", req.required_by, req.requirement)?;
            if let Some(v) = &req.resolved_version {
                writeln!(f, "    (resolved to {v})")?;
            }
        }
        Ok(())
    }
}

/// Detects and resolves version conflicts
pub struct ConflictDetector {
    requirements: HashMap<String, Vec<(String, String)>>, // resource -> [(requirer, requirement)]
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ConflictDetector {
    pub fn new() -> Self {
        Self {
            requirements: HashMap::new(),
        }
    }

    /// Add a dependency requirement
    pub fn add_requirement(&mut self, resource: &str, required_by: &str, requirement: &str) {
        self.requirements
            .entry(resource.to_string())
            .or_default()
            .push((required_by.to_string(), requirement.to_string()));
    }

    /// Detect conflicts in the current requirements
    pub fn detect_conflicts(&self) -> Vec<VersionConflict> {
        let mut conflicts = Vec::new();

        for (resource, requirements) in &self.requirements {
            if requirements.len() <= 1 {
                continue; // No conflict possible with single requirement
            }

            // Check if requirements are compatible
            let compatible = self.are_requirements_compatible(requirements);
            if !compatible {
                let conflict = VersionConflict {
                    resource: resource.clone(),
                    conflicting_requirements: requirements
                        .iter()
                        .map(|(requirer, req)| ConflictingRequirement {
                            required_by: requirer.clone(),
                            requirement: req.clone(),
                            resolved_version: None,
                        })
                        .collect(),
                };
                conflicts.push(conflict);
            }
        }

        conflicts
    }

    /// Check if a set of requirements are compatible
    fn are_requirements_compatible(&self, requirements: &[(String, String)]) -> bool {
        // Check for HEAD (unspecified version) mixed with specific versions
        let has_head = requirements.iter().any(|(_, req)| req == "HEAD");
        let has_specific = requirements.iter().any(|(_, req)| req != "HEAD");

        if has_head && has_specific {
            // HEAD mixed with specific versions is a conflict
            return false;
        }

        // Parse all requirements (with v-prefix normalization)
        let parsed_reqs: Vec<_> = requirements
            .iter()
            .filter_map(|(_, req)| {
                if req == "*" {
                    Some(VersionReq::parse("*").unwrap())
                } else if req == "HEAD" {
                    // HEAD is handled above
                    None
                } else {
                    crate::version::parse_version_req(req).ok()
                }
            })
            .collect();

        if parsed_reqs.len() != requirements.len() {
            // Some requirements couldn't be parsed as semver
            // Check if we have BOTH semver and git refs - that's a conflict
            let has_semver = !parsed_reqs.is_empty();
            let has_git_refs = parsed_reqs.len() < requirements.len();

            if has_semver && has_git_refs {
                // Mixed semver and git refs - incompatible!
                return false;
            }

            // All git refs - check if they're compatible
            return self.check_git_ref_compatibility(requirements);
        }

        // All semver - check if ranges intersect
        self.can_satisfy_all(&parsed_reqs)
    }

    /// Check if git references are compatible
    ///
    /// Normalizes refs to lowercase to handle case-insensitive filesystems (Windows, macOS)
    /// where "main" and "Main" refer to the same branch.
    fn check_git_ref_compatibility(&self, requirements: &[(String, String)]) -> bool {
        let refs: HashSet<_> = requirements
            .iter()
            .filter_map(|(_, req)| {
                if !req.starts_with('^')
                    && !req.starts_with('~')
                    && !req.starts_with('>')
                    && !req.starts_with('<')
                    && !req.starts_with('=')
                    && req != "HEAD"
                    && req != "*"
                {
                    // Normalize to lowercase for case-insensitive comparison
                    // (handles "main" vs "Main" on case-insensitive filesystems)
                    Some(req.to_lowercase())
                } else {
                    None
                }
            })
            .collect();

        // All git refs must be the same (after normalization)
        refs.len() <= 1
    }

    /// Check if all requirements can be satisfied by some version
    ///
    /// Uses pubgrub's Ranges type for proper range intersection, avoiding heuristics.
    fn can_satisfy_all(&self, requirements: &[VersionReq]) -> bool {
        if requirements.is_empty() {
            return true;
        }

        // Convert all VersionReq to Ranges and compute intersection
        let mut intersection: Option<Ranges<Version>> = None;

        for req in requirements {
            let range = self.version_req_to_ranges(req);

            intersection = match intersection {
                None => Some(range),
                Some(current) => Some(current.intersection(&range)),
            };

            // Early exit if intersection becomes empty
            if let Some(ref i) = intersection
                && i.is_empty()
            {
                return false;
            }
        }

        // If we have a non-empty intersection, requirements are compatible
        intersection.is_none_or(|i| !i.is_empty())
    }

    /// Convert a `semver::VersionReq` to `pubgrub::Ranges<Version>`
    ///
    /// In semver, multiple comparators in a single requirement are `ANDed` together (intersection),
    /// not `ORed` (union). For example, ">=5.0.0, <6.0.0" means [5.0.0, 6.0.0), not the entire number line.
    fn version_req_to_ranges(&self, req: &VersionReq) -> Ranges<Version> {
        let comparators = &req.comparators;

        // Wildcard: matches all versions
        if comparators.is_empty() {
            return Ranges::full();
        }

        // Start with full range and intersect each comparator (AND semantics)
        let mut ranges = Ranges::full();

        for comp in comparators {
            // Build Version with prerelease and build metadata
            let base_version = if comp.pre.is_empty() {
                Version::new(comp.major, comp.minor.unwrap_or(0), comp.patch.unwrap_or(0))
            } else {
                Version {
                    major: comp.major,
                    minor: comp.minor.unwrap_or(0),
                    patch: comp.patch.unwrap_or(0),
                    pre: comp.pre.clone(),
                    build: Default::default(),
                }
            };

            let comp_range = match comp.op {
                Op::Exact => {
                    // =x.y.z → exactly that version
                    Ranges::singleton(base_version)
                }
                Op::Greater => {
                    // >x.y.z → strictly higher than x.y.z
                    Ranges::strictly_higher_than(base_version)
                }
                Op::GreaterEq => {
                    // >=x.y.z → x.y.z or higher
                    Ranges::higher_than(base_version)
                }
                Op::Less => {
                    // <x.y.z → strictly lower than x.y.z
                    Ranges::strictly_lower_than(base_version)
                }
                Op::LessEq => {
                    // <=x.y.z → x.y.z or lower
                    Ranges::lower_than(base_version)
                }
                Op::Tilde => {
                    // Tilde operator: allows patch updates
                    // ~1.2.3 → >=1.2.3, <1.3.0
                    // ~1.2 → >=1.2.0, <1.3.0
                    // ~1 → >=1.0.0, <2.0.0 (allows minor and patch updates)
                    let upper = if comp.minor.is_none() {
                        // ~1 → [1.0.0, 2.0.0)
                        Version::new(comp.major + 1, 0, 0)
                    } else {
                        // ~1.2 or ~1.2.3 → [1.2.x, 1.3.0)
                        Version::new(comp.major, comp.minor.unwrap() + 1, 0)
                    };
                    Ranges::between(base_version, upper)
                }
                Op::Caret => {
                    // Caret operator: compatible updates (no breaking changes)
                    // ^1.2.3 → >=1.2.3, <2.0.0 (major != 0: allow minor and patch)
                    // ^0.2.3 → >=0.2.3, <0.3.0 (major == 0: allow only patch)
                    // ^0.0.3 → >=0.0.3, <0.0.4 (major == 0 && minor == 0: allow only exact patch)
                    // ^0.0 → >=0.0.0, <0.1.0
                    // ^0 → >=0.0.0, <1.0.0

                    if base_version.major > 0 {
                        // ^x.y.z (x>0) → [x.y.z, (x+1).0.0)
                        let upper = Version::new(base_version.major + 1, 0, 0);
                        Ranges::between(base_version, upper)
                    } else if base_version.minor > 0 {
                        // ^0.y.z (y>0) → [0.y.z, 0.(y+1).0)
                        let upper = Version::new(0, base_version.minor + 1, 0);
                        Ranges::between(base_version, upper)
                    } else if comp.patch.is_some() && base_version.patch > 0 {
                        // ^0.0.z (z>0) → [0.0.z, 0.0.(z+1))
                        let upper = Version::new(0, 0, base_version.patch + 1);
                        Ranges::between(base_version, upper)
                    } else if comp.patch.is_none() && comp.minor.is_some() {
                        // ^0.0 → [0.0.0, 0.1.0)
                        let upper = Version::new(0, 1, 0);
                        Ranges::between(base_version, upper)
                    } else if comp.minor.is_none() {
                        // ^0 → [0.0.0, 1.0.0)
                        let upper = Version::new(1, 0, 0);
                        Ranges::between(base_version, upper)
                    } else {
                        // ^0.0.0 → [0.0.0, 0.0.1)
                        let upper = Version::new(0, 0, 1);
                        Ranges::between(base_version, upper)
                    }
                }
                Op::Wildcard => {
                    // x.* or x.y.* → all versions in that major/minor
                    if comp.minor.is_none() {
                        // x.* → >=x.0.0, <(x+1).0.0
                        let lower = Version::new(comp.major, 0, 0);
                        let upper = Version::new(comp.major + 1, 0, 0);
                        Ranges::between(lower, upper)
                    } else if comp.patch.is_none() {
                        // x.y.* → >=x.y.0, <x.(y+1).0
                        let lower = Version::new(comp.major, comp.minor.unwrap(), 0);
                        let upper = Version::new(comp.major, comp.minor.unwrap() + 1, 0);
                        Ranges::between(lower, upper)
                    } else {
                        // Full version specified - shouldn't happen with Wildcard op
                        Ranges::singleton(base_version)
                    }
                }
                _ => {
                    // Unknown operator - treat as full range (overly permissive)
                    Ranges::full()
                }
            };

            // Intersect with accumulated range (AND semantics)
            ranges = ranges.intersection(&comp_range);
        }

        ranges
    }

    /// Try to resolve conflicts by finding compatible versions
    pub fn resolve_conflicts(
        &self,
        available_versions: &HashMap<String, Vec<Version>>,
    ) -> Result<HashMap<String, Version>> {
        let mut resolved = HashMap::new();
        let conflicts = self.detect_conflicts();

        if !conflicts.is_empty() {
            let conflict_messages: Vec<String> =
                conflicts.iter().map(std::string::ToString::to_string).collect();

            return Err(AgpmError::Other {
                message: format!(
                    "Unable to resolve version conflicts:\n{}",
                    conflict_messages.join("\n")
                ),
            }
            .into());
        }

        // Resolve each resource to its best version
        for (resource, requirements) in &self.requirements {
            let versions = available_versions.get(resource).ok_or_else(|| AgpmError::Other {
                message: format!("No versions available for resource: {resource}"),
            })?;

            let best_version = self.find_best_version(versions, requirements)?;
            resolved.insert(resource.clone(), best_version);
        }

        Ok(resolved)
    }

    /// Find the best version that satisfies all requirements
    fn find_best_version(
        &self,
        available: &[Version],
        requirements: &[(String, String)],
    ) -> Result<Version> {
        let mut candidates = available.to_vec();

        // Filter by each requirement
        for (_, req_str) in requirements {
            if req_str == "latest" || req_str == "*" {
                continue; // These match everything
            }

            if let Ok(req) = crate::version::parse_version_req(req_str) {
                candidates.retain(|v| req.matches(v));
            }
        }

        if candidates.is_empty() {
            return Err(AgpmError::Other {
                message: format!("No version satisfies all requirements: {requirements:?}"),
            }
            .into());
        }

        // Sort and return the highest version
        candidates.sort_by(|a, b| b.cmp(a));
        Ok(candidates[0].clone())
    }
}

/// Analyzes dependency graphs for circular dependencies
pub struct CircularDependencyDetector {
    graph: HashMap<String, HashSet<String>>,
}

impl Default for CircularDependencyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl CircularDependencyDetector {
    pub fn new() -> Self {
        Self {
            graph: HashMap::new(),
        }
    }

    /// Add a dependency edge
    pub fn add_dependency(&mut self, from: &str, to: &str) {
        self.graph.entry(from.to_string()).or_default().insert(to.to_string());
    }

    /// Detect circular dependencies
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node in self.graph.keys() {
            if !visited.contains(node) {
                self.dfs_detect_cycle(node, &mut visited, &mut rec_stack, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_detect_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = self.graph.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    self.dfs_detect_cycle(neighbor, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(neighbor) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap();
                    let cycle = path[cycle_start..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_detection() {
        let mut detector = ConflictDetector::new();

        // Add compatible requirements
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^1.2.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0); // These are compatible

        // Add incompatible requirements
        detector.add_requirement("lib2", "app1", "^1.0.0");
        detector.add_requirement("lib2", "app2", "^2.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resource, "lib2");
    }

    #[test]
    fn test_git_ref_compatibility() {
        let mut detector = ConflictDetector::new();

        // Same git ref - compatible
        detector.add_requirement("lib1", "app1", "main");
        detector.add_requirement("lib1", "app2", "main");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0);

        // Different git refs - incompatible
        detector.add_requirement("lib2", "app1", "main");
        detector.add_requirement("lib2", "app2", "develop");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_git_ref_case_insensitive() {
        let mut detector = ConflictDetector::new();

        // Git refs differing only by case should be treated as the same
        // (important for case-insensitive filesystems like Windows and macOS)
        detector.add_requirement("lib1", "app1", "main");
        detector.add_requirement("lib1", "app2", "Main");
        detector.add_requirement("lib1", "app3", "MAIN");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Git refs differing only by case should be compatible (case-insensitive filesystems)"
        );

        // Mixed case with different branch names should still conflict
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement("lib2", "app1", "Main");
        detector2.add_requirement("lib2", "app2", "Develop");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(
            conflicts2.len(),
            1,
            "Different branch names should conflict regardless of case"
        );
    }

    #[test]
    fn test_resolve_conflicts() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^1.2.0");

        let mut available = HashMap::new();
        available.insert(
            "lib1".to_string(),
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.2.0").unwrap(),
                Version::parse("1.5.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ],
        );

        let resolved = detector.resolve_conflicts(&available).unwrap();
        assert_eq!(resolved.get("lib1"), Some(&Version::parse("1.5.0").unwrap()));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut detector = CircularDependencyDetector::new();

        // Create a cycle: A -> B -> C -> A
        detector.add_dependency("A", "B");
        detector.add_dependency("B", "C");
        detector.add_dependency("C", "A");

        let cycles = detector.detect_cycles();
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].contains(&"A".to_string()));
        assert!(cycles[0].contains(&"B".to_string()));
        assert!(cycles[0].contains(&"C".to_string()));
    }

    #[test]
    fn test_no_circular_dependencies() {
        let mut detector = CircularDependencyDetector::new();

        // Create a DAG: A -> B -> C
        detector.add_dependency("A", "B");
        detector.add_dependency("B", "C");
        detector.add_dependency("A", "C");

        let cycles = detector.detect_cycles();
        assert_eq!(cycles.len(), 0);
    }

    #[test]
    fn test_conflict_display() {
        let conflict = VersionConflict {
            resource: "test-lib".to_string(),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_version: Some(Version::parse("1.5.0").unwrap()),
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^2.0.0".to_string(),
                    resolved_version: None,
                },
            ],
        };

        let display = format!("{}", conflict);
        assert!(display.contains("test-lib"));
        assert!(display.contains("app1"));
        assert!(display.contains("^1.0.0"));
        assert!(display.contains("1.5.0"));
    }

    #[test]
    fn test_head_with_specific_version_conflict() {
        let mut detector = ConflictDetector::new();

        // HEAD (unspecified) mixed with specific version should conflict
        detector.add_requirement("lib1", "app1", "HEAD");
        detector.add_requirement("lib1", "app2", "^1.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "HEAD mixed with specific version should conflict");

        // "*" with any specific range is compatible (intersection is non-empty)
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement("lib2", "app1", "*");
        detector2.add_requirement("lib2", "app2", "^1.0.0");

        let conflicts = detector2.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "* should be compatible with ^1.0.0 (intersection is [1.0.0, 2.0.0))"
        );

        // "*" with ~2.1.0 is also compatible (intersection is [2.1.0, 2.2.0))
        let mut detector3 = ConflictDetector::new();
        detector3.add_requirement("lib3", "app1", "*");
        detector3.add_requirement("lib3", "app2", "~2.1.0");

        let conflicts = detector3.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "* should be compatible with ~2.1.0 (intersection is [2.1.0, 2.2.0))"
        );
    }

    #[test]
    fn test_mixed_semver_and_git_refs() {
        let mut detector = ConflictDetector::new();

        // Mix of semver and git branch - should be incompatible
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "main");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "Mixed semver and git ref should be detected as conflict");

        // Test with exact version and git tag
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement("lib2", "app1", "v1.0.0");
        detector2.add_requirement("lib2", "app2", "develop");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "Exact version with git branch should conflict");
    }

    #[test]
    fn test_duplicate_requirements_same_version() {
        let mut detector = ConflictDetector::new();

        // Multiple resources requiring the same exact version
        detector.add_requirement("lib1", "app1", "v1.0.0");
        detector.add_requirement("lib1", "app2", "v1.0.0");
        detector.add_requirement("lib1", "app3", "v1.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0, "Same version requirements should not conflict");
    }

    #[test]
    fn test_exact_version_conflicts() {
        let mut detector = ConflictDetector::new();

        // Different exact versions - definitely incompatible
        detector.add_requirement("lib1", "app1", "v1.0.0");
        detector.add_requirement("lib1", "app2", "v2.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "Different exact versions must conflict");
        assert_eq!(conflicts[0].conflicting_requirements.len(), 2);
    }

    #[test]
    fn test_resolve_conflicts_missing_resource() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^1.0.0");

        let available = HashMap::new(); // Empty - missing lib1

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when resource not in available versions");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No versions available"), "Error should mention missing versions");
    }

    #[test]
    fn test_resolve_conflicts_with_incompatible_ranges() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^2.0.0");

        let mut available = HashMap::new();
        available.insert(
            "lib1".to_string(),
            vec![Version::parse("1.5.0").unwrap(), Version::parse("2.3.0").unwrap()],
        );

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when requirements are incompatible");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unable to resolve version conflicts"),
            "Error should mention conflict resolution failure"
        );
    }

    #[test]
    fn test_resolve_conflicts_no_matching_version() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^3.0.0"); // Requires 3.x

        let mut available = HashMap::new();
        available.insert(
            "lib1".to_string(),
            vec![Version::parse("1.0.0").unwrap(), Version::parse("2.0.0").unwrap()],
        );

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when no version satisfies requirement");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No version satisfies"),
            "Error should mention no matching version: {}",
            err_msg
        );
    }

    #[test]
    fn test_conflict_aggregated_error_message() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^2.0.0");
        detector.add_requirement("lib2", "app1", "main");
        detector.add_requirement("lib2", "app3", "develop");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 2, "Should detect both conflicts");

        // Verify the conflicts contain proper information
        let lib1_conflict = conflicts.iter().find(|c| c.resource == "lib1");
        assert!(lib1_conflict.is_some(), "Should have lib1 conflict");
        assert_eq!(
            lib1_conflict.unwrap().conflicting_requirements.len(),
            2,
            "lib1 should have 2 conflicting requirements"
        );

        let lib2_conflict = conflicts.iter().find(|c| c.resource == "lib2");
        assert!(lib2_conflict.is_some(), "Should have lib2 conflict");
        assert_eq!(
            lib2_conflict.unwrap().conflicting_requirements.len(),
            2,
            "lib2 should have 2 conflicting requirements"
        );
    }

    #[test]
    fn test_multi_comparator_compatible() {
        let mut detector = ConflictDetector::new();

        // ">=5.0.0, <6.0.0" should be compatible with ">=5.5.0"
        // Intersection is [5.5.0, 6.0.0)
        detector.add_requirement("lib1", "app1", ">=5.0.0, <6.0.0");
        detector.add_requirement("lib1", "app2", ">=5.5.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Multi-comparator ranges with non-empty intersection should be compatible"
        );
    }

    #[test]
    fn test_multi_comparator_incompatible() {
        let mut detector = ConflictDetector::new();

        // ">=5.0.0, <6.0.0" should conflict with ">=7.0.0"
        // Intersection is empty
        detector.add_requirement("lib1", "app1", ">=5.0.0, <6.0.0");
        detector.add_requirement("lib1", "app2", ">=7.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            1,
            "Multi-comparator ranges with empty intersection should conflict"
        );
    }

    #[test]
    fn test_tilde_operator_variants() {
        let mut detector1 = ConflictDetector::new();

        // ~1 means [1.0.0, 2.0.0) - should be compatible with ^1.5.0 [1.5.0, 2.0.0)
        detector1.add_requirement("lib1", "app1", "~1");
        detector1.add_requirement("lib1", "app2", "^1.5.0");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "~1 should be compatible with ^1.5.0 (intersection is [1.5.0, 2.0.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // ~1.2 means [1.2.0, 1.3.0) - should conflict with ^1.5.0 [1.5.0, 2.0.0)
        detector2.add_requirement("lib2", "app1", "~1.2");
        detector2.add_requirement("lib2", "app2", "^1.5.0");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "~1.2 should conflict with ^1.5.0 (disjoint ranges)");

        let mut detector3 = ConflictDetector::new();

        // ~1.2.3 means [1.2.3, 1.3.0) - should be compatible with >=1.2.0
        detector3.add_requirement("lib3", "app1", "~1.2.3");
        detector3.add_requirement("lib3", "app2", ">=1.2.0");

        let conflicts3 = detector3.detect_conflicts();
        assert_eq!(conflicts3.len(), 0, "~1.2.3 should be compatible with >=1.2.0");
    }

    #[test]
    fn test_caret_zero_zero_patch() {
        let mut detector1 = ConflictDetector::new();

        // ^0.0.3 means [0.0.3, 0.0.4) - should be compatible with >=0.0.3, <0.0.5
        detector1.add_requirement("lib1", "app1", "^0.0.3");
        detector1.add_requirement("lib1", "app2", ">=0.0.3, <0.0.5");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "^0.0.3 should be compatible with >=0.0.3, <0.0.5 (intersection is [0.0.3, 0.0.4))"
        );

        let mut detector2 = ConflictDetector::new();

        // ^0.0.3 means [0.0.3, 0.0.4) - should conflict with ^0.0.5 [0.0.5, 0.0.6)
        detector2.add_requirement("lib2", "app1", "^0.0.3");
        detector2.add_requirement("lib2", "app2", "^0.0.5");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "^0.0.3 should conflict with ^0.0.5 (disjoint ranges)");
    }

    #[test]
    fn test_caret_zero_variants() {
        let mut detector1 = ConflictDetector::new();

        // ^0 means [0.0.0, 1.0.0) - should be compatible with ^0.5.0 [0.5.0, 0.6.0)
        detector1.add_requirement("lib1", "app1", "^0");
        detector1.add_requirement("lib1", "app2", "^0.5.0");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "^0 should be compatible with ^0.5.0 (intersection is [0.5.0, 0.6.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // ^0.0 means [0.0.0, 0.1.0) - should conflict with ^0.5.0 [0.5.0, 0.6.0)
        detector2.add_requirement("lib2", "app1", "^0.0");
        detector2.add_requirement("lib2", "app2", "^0.5.0");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "^0.0 should conflict with ^0.5.0 (disjoint ranges)");
    }

    #[test]
    fn test_prerelease_versions() {
        let mut detector1 = ConflictDetector::new();

        // =1.0.0-beta.1 should conflict with =1.0.0 (different versions)
        detector1.add_requirement("lib1", "app1", "=1.0.0-beta.1");
        detector1.add_requirement("lib1", "app2", "=1.0.0");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            1,
            "=1.0.0-beta.1 should conflict with =1.0.0 (different prerelease)"
        );

        let mut detector2 = ConflictDetector::new();

        // =1.0.0-beta.1 should be compatible with itself
        detector2.add_requirement("lib2", "app1", "=1.0.0-beta.1");
        detector2.add_requirement("lib2", "app2", "=1.0.0-beta.1");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 0, "Same prerelease version should be compatible");

        let mut detector3 = ConflictDetector::new();

        // >=1.0.0-beta should be compatible with >=1.0.0-alpha (intersection exists)
        detector3.add_requirement("lib3", "app1", ">=1.0.0-beta");
        detector3.add_requirement("lib3", "app2", ">=1.0.0-alpha");

        let conflicts3 = detector3.detect_conflicts();
        assert_eq!(conflicts3.len(), 0, ">=1.0.0-beta should be compatible with >=1.0.0-alpha");
    }

    #[test]
    fn test_high_version_ranges() {
        let mut detector = ConflictDetector::new();

        // Test ranges well above typical test versions (>3.0.0)
        detector.add_requirement("lib1", "app1", ">=5.0.0, <10.0.0");
        detector.add_requirement("lib1", "app2", "^7.5.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "High version ranges should work correctly (intersection is [7.5.0, 8.0.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // Test conflicting high version ranges
        detector2.add_requirement("lib2", "app1", ">=100.0.0");
        detector2.add_requirement("lib2", "app2", "<50.0.0");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "Disjoint high version ranges should conflict");
    }
}
