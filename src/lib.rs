//! agentic-repos - Multi-tenant Git server with SSH protocol support

#![allow(clippy::result_large_err)]

pub mod api;
pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod error;
pub mod features;
#[allow(
    clippy::while_let_loop,
    clippy::too_many_arguments,
    clippy::large_enum_variant
)]
pub mod git;
pub mod hooks;
pub mod lfs;
pub mod mcp;
pub mod models;
pub mod rbac;
pub mod server;
#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::missing_safety_doc)]
pub mod skynet;
pub mod storage;

pub use config::AppConfig;

#[cfg(test)]
mod auth_integration_tests;

#[cfg(test)]
mod lfs_security_tests;

#[cfg(test)]
mod access_control_tests;

#[cfg(test)]
mod perf_tests;

#[cfg(test)]
mod git_protocol_tests;

pub use error::Result;
