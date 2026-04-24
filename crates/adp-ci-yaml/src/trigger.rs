//! Trigger matching — determine if a workflow should run for a given event.

use crate::types::{TriggerConfig, TriggerEvent};
use globset::GlobBuilder;

/// Check if a trigger event matches the workflow's trigger configuration.
pub fn matches_trigger(config: &TriggerConfig, event: &TriggerEvent) -> bool {
    match event {
        TriggerEvent::Push { branch, tag } => {
            if let Some(push_cfg) = &config.push {
                // Check tag match first
                if let Some(t) = tag {
                    if !push_cfg.tags.is_empty() {
                        return matches_glob_patterns(t, &push_cfg.tags);
                    }
                }
                // Check branch match
                if !push_cfg.branches.is_empty() {
                    return matches_glob_patterns(branch, &push_cfg.branches);
                }
                // No branch/tag filters means match all pushes
                return true;
            }
            false
        }
        TriggerEvent::PullRequest { branch, action } => {
            if let Some(pr_cfg) = &config.pull_request {
                // Check action
                if !pr_cfg.types.is_empty() && !pr_cfg.types.contains(action) {
                    return false;
                }
                // Check branch
                if !pr_cfg.branches.is_empty() {
                    return matches_glob_patterns(branch, &pr_cfg.branches);
                }
                return true;
            }
            false
        }
    }
}

/// Match a string against a list of glob patterns.
fn matches_glob_patterns(value: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        match GlobBuilder::new(p)
            .literal_separator(true)
            .build()
        {
            Ok(glob) => glob.compile_matcher().is_match(value),
            Err(_) => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PushTrigger, PullRequestTrigger};

    #[test]
    fn push_matches_exact_branch() {
        let config = TriggerConfig {
            push: Some(PushTrigger {
                branches: vec!["main".into()],
                tags: vec![],
                paths: vec![],
                paths_ignore: vec![],
            }),
            pull_request: None,
        };

        assert!(matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "main".into(),
                tag: None,
            }
        ));
        assert!(!matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "develop".into(),
                tag: None,
            }
        ));
    }

    #[test]
    fn push_matches_glob_branch() {
        let config = TriggerConfig {
            push: Some(PushTrigger {
                branches: vec!["release/*".into()],
                tags: vec![],
                paths: vec![],
                paths_ignore: vec![],
            }),
            pull_request: None,
        };

        assert!(matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "release/v1.0".into(),
                tag: None,
            }
        ));
        assert!(!matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "feature/foo".into(),
                tag: None,
            }
        ));
    }

    #[test]
    fn push_matches_tag() {
        let config = TriggerConfig {
            push: Some(PushTrigger {
                branches: vec![],
                tags: vec!["v*".into()],
                paths: vec![],
                paths_ignore: vec![],
            }),
            pull_request: None,
        };

        assert!(matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "main".into(),
                tag: Some("v1.0.0".into()),
            }
        ));
    }

    #[test]
    fn push_no_filters_matches_all() {
        let config = TriggerConfig {
            push: Some(PushTrigger {
                branches: vec![],
                tags: vec![],
                paths: vec![],
                paths_ignore: vec![],
            }),
            pull_request: None,
        };

        assert!(matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "anything".into(),
                tag: None,
            }
        ));
    }

    #[test]
    fn pr_matches_action() {
        let config = TriggerConfig {
            push: None,
            pull_request: Some(PullRequestTrigger {
                branches: vec!["main".into()],
                types: vec!["opened".into(), "synchronize".into()],
            }),
        };

        assert!(matches_trigger(
            &config,
            &TriggerEvent::PullRequest {
                branch: "main".into(),
                action: "opened".into(),
            }
        ));
        assert!(!matches_trigger(
            &config,
            &TriggerEvent::PullRequest {
                branch: "main".into(),
                action: "closed".into(),
            }
        ));
    }

    #[test]
    fn no_triggers_matches_nothing() {
        let config = TriggerConfig::default();
        assert!(!matches_trigger(
            &config,
            &TriggerEvent::Push {
                branch: "main".into(),
                tag: None,
            }
        ));
    }
}
