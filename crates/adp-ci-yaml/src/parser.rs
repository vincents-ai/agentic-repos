//! Workflow parser — parse adp-ci.yaml into CiWorkflow and convert to JobRequests.

use crate::error::YamlError;
use crate::types::{CiWorkflow, JobDefinition, TriggerEvent};
use adp_ci_types::{ArtifactStorage, ComplianceRequirements, JobRequest, JobStep, JobTrigger};
use std::collections::{HashMap, HashSet};

/// Parse a YAML string into a CiWorkflow.
pub fn parse_workflow(yaml: &str) -> Result<CiWorkflow, YamlError> {
    let workflow: CiWorkflow = serde_yaml::from_str(yaml)?;
    validate_workflow(&workflow)?;
    Ok(workflow)
}

/// Validate a parsed workflow.
fn validate_workflow(workflow: &CiWorkflow) -> Result<(), YamlError> {
    // Version check
    if workflow.version != 1 {
        return Err(YamlError::InvalidValue {
            field: "version".into(),
            value: workflow.version.to_string(),
        });
    }

    // Validate each job
    for (name, job) in &workflow.jobs {
        if job.steps.is_empty() {
            return Err(YamlError::Validation(format!(
                "Job '{}' has no steps",
                name
            )));
        }

        // Validate step names are present
        for (i, step) in job.steps.iter().enumerate() {
            if step.name.is_empty() {
                return Err(YamlError::Validation(format!(
                    "Job '{}' step {} has no name",
                    name, i
                )));
            }
            if step.command.is_empty() {
                return Err(YamlError::Validation(format!(
                    "Job '{}' step '{}' has no command",
                    name, step.name
                )));
            }
        }

        // Validate dependencies exist
        for dep in &job.needs {
            if !workflow.jobs.contains_key(dep) {
                return Err(YamlError::UnknownDependency {
                    job: name.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Check for circular dependencies
    check_circular_deps(&workflow.jobs)?;

    Ok(())
}

/// Detect circular dependencies in job `needs` graph.
fn check_circular_deps(jobs: &HashMap<String, JobDefinition>) -> Result<(), YamlError> {
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for job_name in jobs.keys() {
        if !visited.contains(job_name) {
            dfs_cycle(job_name, jobs, &mut visited, &mut in_stack)?;
        }
    }
    Ok(())
}

fn dfs_cycle(
    job: &str,
    jobs: &HashMap<String, JobDefinition>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
) -> Result<(), YamlError> {
    visited.insert(job.to_string());
    in_stack.insert(job.to_string());

    if let Some(def) = jobs.get(job) {
        for dep in &def.needs {
            if in_stack.contains(dep) {
                return Err(YamlError::CircularDependency {
                    cycle: format!("{} → {}", job, dep),
                });
            }
            if !visited.contains(dep) {
                dfs_cycle(dep, jobs, visited, in_stack)?;
            }
        }
    }

    in_stack.remove(job);
    Ok(())
}

/// Convert a workflow into job requests for a given trigger event.
///
/// Returns job requests in topological order (respecting `needs`).
/// Only includes jobs whose trigger config matches the event.
pub fn workflow_to_jobs(
    workflow: &CiWorkflow,
    event: &TriggerEvent,
    _repo_id: &str,
    commit_sha: &str,
    committer: &str,
    branch: &str,
) -> Result<Vec<JobRequest>, YamlError> {
    use crate::trigger::matches_trigger;

    // Check if the workflow triggers for this event
    if !matches_trigger(&workflow.on, event) {
        return Ok(Vec::new());
    }

    // Filter jobs that should run
    let ordered = topological_sort(&workflow.jobs)?;

    let mut requests = Vec::new();
    for job_name in ordered {
        let job_def = workflow.jobs.get(&job_name).unwrap();

        let trigger = JobTrigger {
            event: match event {
                TriggerEvent::Push { .. } => "push".into(),
                TriggerEvent::PullRequest { .. } => "pull_request".into(),
            },
            branch: branch.into(),
            commit_sha: commit_sha.into(),
            committer: committer.into(),
            init_submodules: true,
        };

        // Merge global + job env into each step
        let steps: Vec<JobStep> = job_def
            .steps
            .iter()
            .map(|step| {
                let mut env = workflow.env.clone();
                env.extend(job_def.env.clone());
                env.extend(step.env.clone());
                JobStep {
                    name: step.name.clone(),
                    environment: job_def.environment.clone(),
                    command: step.command.clone(),
                    timeout_secs: step.timeout,
                    working_dir: step.working_dir.clone(),
                    env,
                }
            })
            .collect();

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger,
            steps,
            secrets: job_def.secrets.clone(),
            compliance: ComplianceRequirements {
                data_residency: Vec::new(),
                requires_soc2: job_def.compliance.contains(&"soc2".to_string()),
                requires_hipaa: job_def.compliance.contains(&"hipaa".to_string()),
            },
            artifacts: job_def.artifacts.clone(),
            artifact_storage: ArtifactStorage::default(),
        };

        requests.push(request);
    }

    Ok(requests)
}

/// Topological sort of jobs respecting `needs` dependencies.
fn topological_sort(jobs: &HashMap<String, JobDefinition>) -> Result<Vec<String>, YamlError> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut temp_mark = HashSet::new();

    for name in jobs.keys() {
        visit(name, jobs, &mut visited, &mut temp_mark, &mut result)?;
    }

    Ok(result)
}

fn visit(
    name: &str,
    jobs: &HashMap<String, JobDefinition>,
    visited: &mut HashSet<String>,
    temp_mark: &mut HashSet<String>,
    result: &mut Vec<String>,
) -> Result<(), YamlError> {
    if visited.contains(name) {
        return Ok(());
    }
    if temp_mark.contains(name) {
        return Err(YamlError::CircularDependency {
            cycle: name.to_string(),
        });
    }

    temp_mark.insert(name.to_string());

    if let Some(job) = jobs.get(name) {
        for dep in &job.needs {
            visit(dep, jobs, visited, temp_mark, result)?;
        }
    }

    temp_mark.remove(name);
    visited.insert(name.to_string());
    result.push(name.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_WORKFLOW: &str = r#"
version: 1

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    environment: nix
    steps:
      - name: Build
        command: nix build
        timeout: 3600
      - name: Check
        command: nix flake check
    artifacts:
      - result/bin/adp
    secrets:
      - deploy-key

  test:
    environment: nix
    needs: [build]
    steps:
      - name: Test
        command: cargo test
        timeout: 1800
    compliance:
      - soc2
"#;

    #[test]
    fn parse_sample_workflow() {
        let workflow = parse_workflow(SAMPLE_WORKFLOW).unwrap();
        assert_eq!(workflow.version, 1);
        assert_eq!(workflow.jobs.len(), 2);
        assert!(workflow.jobs.contains_key("build"));
        assert!(workflow.jobs.contains_key("test"));

        let build = &workflow.jobs["build"];
        assert_eq!(build.environment, "nix");
        assert_eq!(build.steps.len(), 2);
        assert_eq!(build.artifacts, vec!["result/bin/adp"]);
    }

    #[test]
    fn reject_invalid_version() {
        let yaml = "version: 2\njobs: {}";
        let result = parse_workflow(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_steps() {
        let yaml = r#"
version: 1
jobs:
  build:
    environment: nix
    steps: []
"#;
        let result = parse_workflow(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn reject_unknown_dependency() {
        let yaml = r#"
version: 1
jobs:
  test:
    environment: nix
    needs: [nonexistent]
    steps:
      - name: Test
        command: echo test
"#;
        let result = parse_workflow(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn reject_circular_dependency() {
        let yaml = r#"
version: 1
jobs:
  a:
    environment: nix
    needs: [b]
    steps:
      - name: A
        command: echo a
  b:
    environment: nix
    needs: [a]
    steps:
      - name: B
        command: echo b
"#;
        let result = parse_workflow(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn topological_sort_respects_needs() {
        let yaml = r#"
version: 1
jobs:
  deploy:
    environment: nix
    needs: [test]
    steps:
      - name: Deploy
        command: echo deploy
  build:
    environment: nix
    steps:
      - name: Build
        command: echo build
  test:
    environment: nix
    needs: [build]
    steps:
      - name: Test
        command: echo test
"#;
        let workflow = parse_workflow(yaml).unwrap();
        let sorted = topological_sort(&workflow.jobs).unwrap();

        assert!(sorted.iter().position(|s| s == "build").unwrap()
            < sorted.iter().position(|s| s == "test").unwrap());
        assert!(sorted.iter().position(|s| s == "test").unwrap()
            < sorted.iter().position(|s| s == "deploy").unwrap());
    }

    #[test]
    fn workflow_to_jobs_on_matching_push() {
        let workflow = parse_workflow(SAMPLE_WORKFLOW).unwrap();
        let event = TriggerEvent::Push {
            branch: "main".into(),
            tag: None,
        };

        let jobs = workflow_to_jobs(&workflow, &event, "repo-1", "abc123", "alice", "main").unwrap();
        assert_eq!(jobs.len(), 2);

        // build should come before test (topological order)
        assert_eq!(jobs[0].steps[0].name, "Build");
        assert_eq!(jobs[1].steps[0].name, "Test");

        // Global env merged into steps
        assert_eq!(jobs[0].steps[0].env.get("CARGO_TERM_COLOR"), Some(&"always".to_string()));
    }

    #[test]
    fn workflow_to_jobs_no_match() {
        let workflow = parse_workflow(SAMPLE_WORKFLOW).unwrap();
        let event = TriggerEvent::Push {
            branch: "feature/something".into(),
            tag: None,
        };

        let jobs = workflow_to_jobs(&workflow, &event, "repo-1", "abc", "alice", "feature/something").unwrap();
        assert!(jobs.is_empty());
    }

    #[test]
    fn minimal_workflow() {
        let yaml = r#"
version: 1
jobs:
  hello:
    environment: sh
    steps:
      - name: Greet
        command: echo hello
"#;
        let workflow = parse_workflow(yaml).unwrap();
        assert_eq!(workflow.jobs.len(), 1);
        assert!(workflow.on.push.is_none());
        assert!(workflow.on.pull_request.is_none());
    }
}
