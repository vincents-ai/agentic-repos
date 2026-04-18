//! Conflict resolution strategies for multi-region replication.

use crate::replication::models::ConflictResolution;
use crate::replication::models::ReplicationConflict;
use std::collections::HashMap;
use uuid::Uuid;

/// Result of applying a conflict resolution strategy.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// The winning version hash.
    pub winning_version: String,
    /// The region whose version wins.
    pub winning_region: String,
    /// The resolution strategy applied.
    pub resolution: ConflictResolution,
}

/// Resolve a conflict using a given strategy.
/// Logs the resolution decision for audit trail.
pub fn resolve(conflict: &ReplicationConflict, strategy: ConflictResolution) -> ResolutionResult {
    let result = match strategy {
        ConflictResolution::SourceWins => ResolutionResult {
            winning_version: conflict.source_version.clone(),
            winning_region: conflict.source_region.clone(),
            resolution: ConflictResolution::SourceWins,
        },
        ConflictResolution::TargetWins => ResolutionResult {
            winning_version: conflict.target_version.clone(),
            winning_region: conflict.target_region.clone(),
            resolution: ConflictResolution::TargetWins,
        },
        ConflictResolution::LastWriteWins => {
            // Compare modification timestamps — the later write wins.
            // If timestamps are unavailable, fall back to source wins
            // and log a warning.
            let source_wins = match (conflict.source_modified_at, conflict.target_modified_at) {
                (Some(src), Some(tgt)) => src >= tgt,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => {
                    tracing::warn!(
                        conflict_id = %conflict.id,
                        "LastWriteWins: no timestamps available, defaulting to source"
                    );
                    true
                }
            };

            if source_wins {
                ResolutionResult {
                    winning_version: conflict.source_version.clone(),
                    winning_region: conflict.source_region.clone(),
                    resolution: ConflictResolution::LastWriteWins,
                }
            } else {
                ResolutionResult {
                    winning_version: conflict.target_version.clone(),
                    winning_region: conflict.target_region.clone(),
                    resolution: ConflictResolution::LastWriteWins,
                }
            }
        }
        ConflictResolution::Merged => ResolutionResult {
            winning_version: format!(
                "merged:{}+{}",
                conflict.source_version, conflict.target_version
            ),
            winning_region: conflict.source_region.clone(),
            resolution: ConflictResolution::Merged,
        },
        ConflictResolution::Manual | ConflictResolution::Pending => ResolutionResult {
            winning_version: String::new(),
            winning_region: String::new(),
            resolution: strategy,
        },
    };

    // Audit log for conflict resolution
    if !result.winning_version.is_empty() {
        tracing::info!(
            conflict_id = %conflict.id,
            repository = %conflict.repository_id,
            object_key = %conflict.object_key,
            strategy = ?strategy,
            winning_version = %result.winning_version,
            winning_region = %result.winning_region,
            "Replication conflict resolved"
        );
    }

    result
}

/// Automatic conflict resolver — applies a policy to a batch of conflicts.
pub struct AutoResolver {
    /// Per-repository default resolution strategy.
    repo_defaults: HashMap<String, ConflictResolution>,
    /// Global default strategy.
    global_default: ConflictResolution,
}

impl AutoResolver {
    pub fn new(global_default: ConflictResolution) -> Self {
        Self {
            repo_defaults: HashMap::new(),
            global_default,
        }
    }

    pub fn with_repo_default(
        mut self,
        repo: impl Into<String>,
        strategy: ConflictResolution,
    ) -> Self {
        self.repo_defaults.insert(repo.into(), strategy);
        self
    }

    /// Determine the resolution strategy for a given conflict.
    pub fn strategy_for(&self, conflict: &ReplicationConflict) -> ConflictResolution {
        self.repo_defaults
            .get(&conflict.repository_id)
            .copied()
            .unwrap_or(self.global_default)
    }

    /// Resolve a batch of conflicts, returning resolution results.
    pub fn resolve_batch(
        &self,
        conflicts: &[ReplicationConflict],
    ) -> Vec<(Uuid, ResolutionResult)> {
        conflicts
            .iter()
            .map(|c| {
                let strategy = self.strategy_for(c);
                (c.id, resolve(c, strategy))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conflict() -> ReplicationConflict {
        ReplicationConflict::new(
            "org/repo",
            "refs/heads/main",
            "abc",
            "def",
            "us-east-1",
            "eu-west-1",
        )
    }

    #[test]
    fn test_source_wins() {
        let conflict = make_conflict();
        let result = resolve(&conflict, ConflictResolution::SourceWins);
        assert_eq!(result.winning_version, "abc");
        assert_eq!(result.winning_region, "us-east-1");
        assert_eq!(result.resolution, ConflictResolution::SourceWins);
    }

    #[test]
    fn test_target_wins() {
        let conflict = make_conflict();
        let result = resolve(&conflict, ConflictResolution::TargetWins);
        assert_eq!(result.winning_version, "def");
        assert_eq!(result.winning_region, "eu-west-1");
    }

    #[test]
    fn test_merged() {
        let conflict = make_conflict();
        let result = resolve(&conflict, ConflictResolution::Merged);
        assert!(result.winning_version.contains("abc"));
        assert!(result.winning_version.contains("def"));
        assert_eq!(result.resolution, ConflictResolution::Merged);
    }

    #[test]
    fn test_pending_returns_empty() {
        let conflict = make_conflict();
        let result = resolve(&conflict, ConflictResolution::Pending);
        assert!(result.winning_version.is_empty());
        assert_eq!(result.resolution, ConflictResolution::Pending);
    }

    #[test]
    fn test_auto_resolver_global_default() {
        let resolver = AutoResolver::new(ConflictResolution::SourceWins);
        let conflict = make_conflict();
        assert_eq!(
            resolver.strategy_for(&conflict),
            ConflictResolution::SourceWins
        );
    }

    #[test]
    fn test_auto_resolver_repo_override() {
        let resolver = AutoResolver::new(ConflictResolution::SourceWins)
            .with_repo_default("org/special", ConflictResolution::TargetWins);
        let default_conflict = make_conflict(); // org/repo
        let special_conflict =
            ReplicationConflict::new("org/special", "refs/heads/main", "a", "b", "us", "eu");

        assert_eq!(
            resolver.strategy_for(&default_conflict),
            ConflictResolution::SourceWins
        );
        assert_eq!(
            resolver.strategy_for(&special_conflict),
            ConflictResolution::TargetWins
        );
    }

    #[test]
    fn test_resolve_batch() {
        let resolver = AutoResolver::new(ConflictResolution::SourceWins)
            .with_repo_default("org/repo", ConflictResolution::TargetWins);
        let c1 = ReplicationConflict::new("org/repo", "key1", "v1", "v2", "us", "eu");
        let c2 = ReplicationConflict::new("other/repo", "key2", "v3", "v4", "us", "eu");

        let results = resolver.resolve_batch(&[c1, c2]);
        assert_eq!(results.len(), 2);
        // org/repo uses TargetWins
        assert_eq!(results[0].1.winning_version, "v2");
        // other/repo uses global default SourceWins
        assert_eq!(results[1].1.winning_version, "v3");
    }
}
