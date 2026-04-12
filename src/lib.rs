//! repos-mcp - Multi-tenant Git server with SSH protocol support

#![allow(clippy::result_large_err)]

pub mod adp;
pub mod analytics;
pub mod api;
pub mod auth;
pub mod backup;
pub mod cache;
pub mod cicd;
pub mod compliance;
pub mod config;
pub mod error;
pub mod features;
#[allow(
    clippy::while_let_loop,
    clippy::too_many_arguments,
    clippy::large_enum_variant
)]
pub mod git;
pub mod lfs;
pub mod mcp;
pub mod models;
pub mod rbac;
pub mod replication;
pub mod scaling;
pub mod sdlc;
pub mod security;
pub mod server;
#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::missing_safety_doc)]
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
