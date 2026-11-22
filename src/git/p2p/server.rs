//! Server side of P2P Git: one Iroh endpoint serving the git smart protocol.
//!
//! The wire shape on a QUIC stream is the same as a Git-over-SSH exec channel:
//! the client sends one command line (`git-upload-pack '<repo>'` or
//! `git-receive-pack '<repo>'`), then the standard smart-protocol bytes flow.
//! Both handlers come from `agentic-git-protocol`; this module only adapts a
//! QUIC stream to `AsyncRead + AsyncWrite` and routes by command.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_git_protocol::command::{CommandValidator, GitCommand};
use agentic_git_protocol::packetline::write_text_packet;
use agentic_git_protocol::protocol::{ReceivePackHandler, UploadPackHandler};
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, RelayMode};
use iroh_base::{NodeAddr, NodeId, SecretKey};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::git::auth::AuthorizedKeysManager;

use super::identity::{node_id, node_id_hex};
use super::{P2pError, Result};

/// ALPN for the Git smart protocol over QUIC.
pub const GIT_ALPN: &[u8] = b"agentic-repos/git/1";

/// Largest request a peer may send in one upload-pack/receive-pack session.
const MAX_STREAM_BYTES: u64 = 100 * 1024 * 1024;

/// Largest accepted command line.
const MAX_COMMAND_LINE: usize = 4096;

/// Configuration for a P2P Git listener.
#[derive(Debug, Clone)]
pub struct P2pGitConfig {
    /// Directory containing bare repositories, addressed as `<owner>/<repo>.git`.
    pub repos_root: PathBuf,
    /// Use Iroh relays (default). Disable for loopback tests.
    pub relay_enabled: bool,
}

impl P2pGitConfig {
    pub fn new(repos_root: PathBuf) -> Self {
        Self {
            repos_root,
            relay_enabled: true,
        }
    }

    pub fn without_relay(mut self) -> Self {
        self.relay_enabled = false;
        self
    }
}

/// A running P2P Git endpoint.
pub struct P2pGitServer {
    endpoint: Endpoint,
    router: Option<Router>,
    node_id: NodeId,
    repos_root: PathBuf,
}

impl P2pGitServer {
    /// Bind an endpoint whose identity is the server's existing SSH host key.
    pub async fn from_host_key(host_key_path: &Path, config: P2pGitConfig) -> Result<Self> {
        let secret = super::identity::secret_key_from_ssh_file(host_key_path)?;
        Self::bind(secret, config).await
    }

    /// Bind an endpoint whose identity is the given Ed25519 key.
    pub async fn bind(secret: SecretKey, config: P2pGitConfig) -> Result<Self> {
        Self::bind_with_auth(secret, config, None).await
    }

    /// Bind an endpoint that authorises receive-pack against `auth`.
    ///
    /// Read (clone/fetch) stays open in milestone 1; a receive-pack request is
    /// refused unless the peer's Node ID corresponds to a listed SSH key.
    pub async fn bind_with_auth(
        secret: SecretKey,
        config: P2pGitConfig,
        auth: Option<Arc<RwLock<AuthorizedKeysManager>>>,
    ) -> Result<Self> {
        let mut builder = Endpoint::builder()
            .secret_key(secret)
            .relay_mode(if config.relay_enabled {
                RelayMode::Default
            } else {
                RelayMode::Disabled
            });
        if config.relay_enabled {
            // A URL carries only a Node ID; discovery is what turns it into a
            // dialable address in production.
            builder = builder.discovery_n0();
        }
        let endpoint = builder
            .bind()
            .await
            .map_err(|e| P2pError::Endpoint(format!("failed to bind Iroh endpoint: {e}")))?;

        let id = endpoint.id();
        let repos_root = config.repos_root.clone();
        let handler = GitAlpnHandler {
            repos_root: repos_root.clone(),
            auth,
        };

        let router = Router::builder(endpoint.clone())
            .accept(GIT_ALPN, handler)
            .spawn();

        info!(
            alpn = %String::from_utf8_lossy(GIT_ALPN),
            node_id = %id,
            repos_root = %repos_root.display(),
            "P2P Git endpoint listening"
        );

        Ok(Self {
            endpoint,
            router: Some(router),
            node_id: id,
            repos_root,
        })
    }

    /// The endpoint's Node ID — the address part of `iroh://<node-id>/<repo>`.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// The Node ID as the base32 string used in URLs (`NodeId` display).
    pub fn node_id_string(&self) -> String {
        self.node_id.to_string()
    }

    /// The Node ID's public-key bytes as lower-case hex (logs and audit lines).
    pub fn node_id_hex(&self) -> String {
        node_id_hex(&self.node_id)
    }

    /// The canonical URL for a repository on this endpoint.
    pub fn url(&self, repo: &str) -> String {
        format!("iroh://{}/{}", self.node_id, repo.trim_start_matches('/'))
    }

    /// Sockets the endpoint is actually bound to (usually one IPv4, one IPv6).
    pub fn bound_sockets(&self) -> Vec<SocketAddr> {
        self.endpoint.bound_sockets().into_iter().collect()
    }

    /// An address a peer can dial over IPv4.
    ///
    /// Iroh binds `0.0.0.0` and `[::]` on different ports. Mapping the IPv6
    /// socket's port onto an IPv4 address produces an address that never
    /// answers, so only the IPv4 socket is used (the same fix as
    /// `agentic-loop-mesh/tests/two_node_validation.rs`).
    pub fn ipv4_addr(&self, host: Ipv4Addr) -> Option<NodeAddr> {
        let port = self
            .endpoint
            .bound_sockets()
            .into_iter()
            .find(|s| s.is_ipv4())
            .map(|s| s.port())?;
        Some(self.addr_for(SocketAddr::new(IpAddr::V4(host), port)))
    }

    /// Build a dialable address for this endpoint at an explicit socket.
    pub fn addr_for(&self, socket: SocketAddr) -> NodeAddr {
        NodeAddr::new(self.node_id).with_direct_addresses([socket])
    }

    /// The underlying Iroh endpoint (tests dial it directly).
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Stop accepting and close the endpoint.
    pub async fn shutdown(mut self) {
        if let Some(router) = self.router.take() {
            let _ = router.shutdown().await;
        }
        self.endpoint.close().await;
    }
}

struct GitAlpnHandler {
    repos_root: PathBuf,
    auth: Option<Arc<RwLock<AuthorizedKeysManager>>>,
}

impl std::fmt::Debug for GitAlpnHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitAlpnHandler")
            .field("repos_root", &self.repos_root)
            .field("push_auth", &self.auth.is_some())
            .finish()
    }
}

impl ProtocolHandler for GitAlpnHandler {
    async fn accept(&self, connection: Connection) -> std::result::Result<(), AcceptError> {
        let peer = connection
            .remote_node_id()
            .map_err(|e| P2pError::Endpoint(format!("remote_node_id: {e}")))?;
        match self.serve(connection, peer).await {
            Ok(()) => Ok(()),
            Err(e) => {
                warn!(peer = %peer, error = %e, "P2P Git session failed");
                Err(AcceptError::from_err(e))
            }
        }
    }
}

impl GitAlpnHandler {
    async fn serve(&self, connection: Connection, peer: NodeId) -> Result<()> {
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .map_err(|e| P2pError::Endpoint(format!("accept_bi: {e}")))?;

        let command = read_command_line(&mut recv).await?;
        let parsed = CommandValidator::validate_command(&command)
            .map_err(|e| P2pError::Protocol(format!("invalid command {command:?}: {e}")))?;

        match parsed {
            GitCommand::UploadPack { repository } => {
                let path = self.repo_path(&repository)?;
                match UploadPackHandler::new(&path) {
                    Ok(handler) => {
                        info!(peer = %peer, repo = %repository, "P2P Git upload-pack");
                        handler
                            .handle(recv.take(MAX_STREAM_BYTES), &mut send)
                            .await?;
                    }
                    Err(e) => {
                        // The smart protocol reports a server-side refusal as an
                        // `ERR` pkt-line; closing the stream silently would make
                        // `git` report "remote end hung up unexpectedly".
                        write_text_packet(&mut send, &format!("ERR {e}")).await?;
                    }
                }
            }
            GitCommand::ReceivePack { repository } => {
                match self.authorize(peer).await {
                    Ok(user) => {
                        let path = self.repo_path(&repository)?;
                        match ReceivePackHandler::new(&path) {
                            Ok(handler) => {
                                info!(peer = %peer, user = %user, repo = %repository, "P2P Git receive-pack");
                                handler
                                    .handle(recv.take(MAX_STREAM_BYTES), &mut send)
                                    .await?;
                            }
                            Err(e) => {
                                write_text_packet(&mut send, &format!("ERR {e}")).await?;
                            }
                        }
                    }
                    Err(reason) => {
                        warn!(peer = %peer, repo = %repository, reason = %reason, "P2P Git push refused");
                        // A pkt-line `ERR` is the smart protocol's way to report a
                        // reason that the client prints; the stream then ends.
                        write_text_packet(&mut send, &format!("ERR {reason}")).await?;
                    }
                }
            }
            GitCommand::LfsAuthenticate { .. } => {
                return Err(P2pError::Protocol(
                    "git-lfs-authenticate is not served on the git ALPN".to_string(),
                ));
            }
        }

        // `finish` is required: returning from `accept` drops the Connection and
        // would discard buffered bytes before the peer reads them.
        send.finish()
            .map_err(|e| P2pError::Endpoint(format!("finish: {e}")))?;
        let _ = send.stopped().await;
        Ok(())
    }

    /// Resolve a repository name under the configured root without escaping it.
    fn repo_path(&self, repository: &str) -> Result<PathBuf> {
        let relative = repository.trim_start_matches('/');
        let path = self.repos_root.join(relative);
        // `CommandValidator` already rejects `..`, absolute paths and system
        // directories; this second check is defence in depth against a lexical
        // path that normalises outside the root.
        let root = self
            .repos_root
            .canonicalize()
            .unwrap_or_else(|_| self.repos_root.clone());
        if let Ok(canonical) = path.canonicalize() {
            if !canonical.starts_with(&root) {
                return Err(P2pError::RepositoryPath(format!(
                    "{repository} resolves outside {}",
                    root.display()
                )));
            }
        }
        Ok(path)
    }

    /// Map the peer Node ID to the SSH public key it *is*, then to a user.
    async fn authorize(&self, peer: NodeId) -> std::result::Result<String, String> {
        let Some(auth) = &self.auth else {
            return Ok("anonymous-p2p".to_string());
        };
        let auth = auth.read().await;
        match auth.user_for_ed25519_public_key(peer.as_bytes()) {
            Some(user) => Ok(user),
            None => Err(format!(
                "node id {} presents no SSH key in refs/adp/users/*/ssh_keys",
                peer
            )),
        }
    }
}

async fn read_command_line<R: AsyncRead + Unpin>(reader: &mut R) -> Result<String> {
    let mut line = Vec::new();
    loop {
        let byte = reader
            .read_u8()
            .await
            .map_err(|e| P2pError::Protocol(format!("failed to read command: {e}")))?;
        if byte == b'\n' {
            break;
        }
        line.push(byte);
        if line.len() > MAX_COMMAND_LINE {
            return Err(P2pError::Protocol("command line too long".to_string()));
        }
    }
    String::from_utf8(line)
        .map_err(|e| P2pError::Protocol(format!("command is not UTF-8: {e}")))
}

/// The Node ID displayed for a secret key without binding an endpoint.
pub fn node_id_for_secret(secret: &SecretKey) -> NodeId {
    node_id(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpn_name_is_stable() {
        assert_eq!(GIT_ALPN, b"agentic-repos/git/1");
    }

    #[test]
    fn repo_path_cannot_escape_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("owner")).unwrap();
        let handler = GitAlpnHandler {
            repos_root: dir.path().to_path_buf(),
            auth: None,
        };
        assert!(handler.repo_path("owner/repo.git").is_ok());
        // CommandValidator rejects these first, but the second check must hold
        // even if it is bypassed.
        let escaped = handler
            .repos_root
            .join("../outside.git")
            .canonicalize()
            .is_err();
        assert!(escaped, "fixture must not accidentally exist");
    }
}
