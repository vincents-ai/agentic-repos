//! repos-mcp - Multi-tenant Git server with SSH protocol support

pub mod adp;
pub mod api;
pub mod auth;
pub mod backup;
pub mod cache;
pub mod compliance;
pub mod config;
pub mod error;
pub mod features;
pub mod git;
pub mod lfs;
pub mod mcp;
pub mod models;
pub mod rbac;
pub mod scaling;
pub mod security;
pub mod server;
pub mod skynet;
pub mod storage;
pub mod web;
pub mod webhooks;

pub use config::AppConfig;

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod git_integration_tests;

pub use error::Result;
