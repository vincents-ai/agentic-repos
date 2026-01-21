//! repos-mcp - Multi-tenant Git server with SSH protocol support

pub mod api;
pub mod config;
pub mod error;
pub mod features;
pub mod git;
pub mod lfs;
pub mod mcp;
pub mod models;
pub mod rbac;
pub mod server;
pub mod storage;
pub mod web;

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod git_integration_tests;

pub use error::Result;
