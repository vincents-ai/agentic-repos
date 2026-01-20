//! oGit - Multi-tenant Git server with MCP integration

pub mod config;
pub mod error;
pub mod git;
pub mod mcp;
pub mod models;
pub mod server;
pub mod storage;

pub use error::Result;
