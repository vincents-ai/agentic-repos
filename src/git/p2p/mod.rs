//! P2P Git — clone/fetch/push over Iroh QUIC.
//!
//! This module is additive: the SSH and HTTP transports are untouched. It adds
//! one server ALPN (`agentic-repos/git/1`) and one client program
//! (`git-remote-iroh`) so `git clone iroh://<node-id>/<repo>` works when the
//! helper is on `PATH`.
//!
//! Identity: the endpoint's Ed25519 key is the existing SSH Ed25519 key — the
//! host key on the server, `~/.ssh/id_ed25519` on the client. The Node ID in the
//! URL is that key's public half.
//!
//! Vocabulary follows `docs/CONTEXT-P2P-GIT.md`: the user-facing feature is
//! **P2P Git**; "Iroh" appears only in code, config and module names.

pub mod client;
pub mod helper;
pub mod identity;
pub mod server;

pub use client::{fetch_pack, list_refs, AdvertisedRef};
pub use server::{P2pGitConfig, P2pGitServer, GIT_ALPN};

/// Errors produced by the P2P Git transport.
#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("P2P I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("P2P protocol error: {0}")]
    Protocol(String),

    #[error("invalid iroh URL: {0}")]
    InvalidUrl(String),

    #[error("P2P identity error: {0}")]
    Key(String),

    #[error("P2P endpoint error: {0}")]
    Endpoint(String),

    #[error("repository path rejected: {0}")]
    RepositoryPath(String),

    #[error("push rejected: {0}")]
    Unauthorized(String),

    #[error("git protocol error: {0}")]
    Git(#[from] agentic_git_protocol::GitProtocolError),
}

pub type Result<T> = std::result::Result<T, P2pError>;
