//! Git smart-protocol handlers — transport-agnostic.
//!
//! Provides `UploadPackHandler` and `ReceivePackHandler` that operate on
//! `AsyncRead + AsyncWrite` pairs. Callers provide a `RepoContext` with
//! repository path and any hooks/callbacks they need.

mod receive_pack;
mod upload_pack;

pub use receive_pack::ReceivePackHandler;
pub use upload_pack::{RefFilter, ShowAllRefs, UploadPackHandler};

use crate::error::Result;
use std::path::Path;
use tokio::io::{AsyncRead, AsyncWrite};

/// Callback context provided by the caller (agentic-repos or agentic-loop).
///
/// This replaces the server-specific `Hooks` and `CallerIdentity` types.
/// The consumer wraps their own types into this struct before calling handlers.
pub struct RepoContext {
    /// Path to the bare git repository on disk.
    pub repo_path: std::path::PathBuf,
    /// Optional caller identity string for logging (e.g. username or node ID).
    pub caller_label: Option<String>,
}

impl RepoContext {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
            caller_label: None,
        }
    }

    pub fn with_caller_label(mut self, label: impl Into<String>) -> Self {
        self.caller_label = Some(label.into());
        self
    }
}

/// Git protocol handler for server-side operations.
pub struct GitProtocolHandler;

impl GitProtocolHandler {
    pub fn new() -> Self {
        Self
    }

    /// Handle git-upload-pack command (clone/fetch).
    pub async fn handle_upload_pack<R, W>(
        &self,
        reader: R,
        writer: W,
        ctx: &RepoContext,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let handler = UploadPackHandler::new(&ctx.repo_path)?;
        handler.handle(reader, writer).await
    }

    /// Handle git-receive-pack command (push).
    pub async fn handle_receive_pack<R, W>(
        &self,
        reader: R,
        writer: W,
        ctx: &RepoContext,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let handler = ReceivePackHandler::new(&ctx.repo_path)?;
        handler.handle(reader, writer).await
    }
}

impl Default for GitProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}
