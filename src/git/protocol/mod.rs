//! Git Smart Protocol implementation
//!
//! This module provides native implementations of git-upload-pack and git-receive-pack
//! using the `gix` crate for repository access and reference management.

mod packetline;
mod receive_pack;
mod upload_pack;

pub use receive_pack::ReceivePackHandler;
pub use upload_pack::UploadPackHandler;

use crate::git::error::Result;
use std::path::Path;
use tokio::io::{AsyncRead, AsyncWrite};

/// Git protocol handler for server-side operations
pub struct GitProtocolHandler;

impl GitProtocolHandler {
    pub fn new() -> Self {
        Self
    }

    /// Handle git-upload-pack command (clone/fetch)
    pub async fn handle_upload_pack<R, W>(
        &self,
        reader: R,
        writer: W,
        repo_path: &Path,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let handler = UploadPackHandler::new(repo_path)?;
        handler.handle(reader, writer).await
    }

    /// Handle git-receive-pack command (push)
    pub async fn handle_receive_pack<R, W>(
        &self,
        reader: R,
        writer: W,
        repo_path: &Path,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let handler = ReceivePackHandler::new(repo_path)?;
        handler.handle(reader, writer).await
    }
}

impl Default for GitProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}
