//! # adp-rbac-engine
//!
//! RBAC enforcement engine for service key namespace restrictions.
//!
//! Provides pattern-matching access control on top of `adp_proto::KeyPermissions`.
//! Used by the SSH handler and git protocol layer to enforce namespace-based
//! restrictions for automated service keys (agentic-loop, CI runners, integrations).
//!
//! ## Usage
//!
//! ```ignore
//! use adp_rbac_engine::RbacEngine;
//! use adp_proto::rbac::KeyPermissions;
//!
//! let engine = RbacEngine::new();
//! let perms = KeyPermissions { /* ... */ };
//! let decision = engine.check_write(&perms, "refs/secrets/agentic-loop/instance-1/secret");
//! assert!(decision.is_allowed());
//! ```

mod checker;
mod codeowners;
mod engine;
mod error;

pub use checker::{AccessDecision, PermissionCheck};
pub use codeowners::{CodeownersEntry, CodeownersParser};
pub use engine::RbacEngine;
pub use error::RbacError;
