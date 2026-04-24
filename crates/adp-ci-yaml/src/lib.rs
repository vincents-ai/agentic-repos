//! # adp-ci-yaml
//!
//! Parser for `adp-ci.yaml` workflow files stored in repositories.
//!
//! ## Workflow file format
//!
//! ```yaml
//! version: 1
//!
//! on:
//!   push:
//!     branches: [main, develop]
//!   pull_request:
//!     branches: [main]
//!
//! env:
//!   CARGO_TERM_COLOR: always
//!
//! jobs:
//!   build:
//!     environment: nix
//!     steps:
//!       - name: Checkout
//!         command: echo "Already checked out"
//!       - name: Build
//!         command: nix build
//!         timeout: 3600
//!     artifacts:
//!       - result/bin/adp
//!     secrets:
//!       - deploy-key
//!
//!   test:
//!     environment: nix
//!     needs: [build]
//!     steps:
//!       - name: Test
//!         command: nix flake check
//!         timeout: 1800
//! ```

mod parser;
mod types;
mod error;
mod trigger;

pub use error::YamlError;
pub use types::{
    CiWorkflow, JobDefinition, StepDefinition, TriggerConfig, TriggerEvent,
    PushTrigger, PullRequestTrigger, WorkflowEnv,
};
pub use parser::{parse_workflow, workflow_to_jobs};
pub use trigger::matches_trigger;
