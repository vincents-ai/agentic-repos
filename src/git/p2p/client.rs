//! Client side of P2P Git: speak the git smart protocol over an Iroh stream.
//!
//! This is what `git-remote-iroh` calls. It deliberately does not reimplement
//! pkt-line framing — `agentic-git-protocol::packetline` is the one codec, shared
//! with the server and with ADP.

use std::net::SocketAddr;
use std::str::FromStr;

use agentic_git_protocol::packetline::{read_packet, write_flush_packet, write_text_packet};
use iroh::{Endpoint, RelayMode};
use iroh_base::{NodeAddr, NodeId, SecretKey};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::debug;

use super::server::GIT_ALPN;
use super::{P2pError, Result};

/// Environment variable holding comma-separated `ip:port` dial addresses.
///
/// The URL form (`iroh://<node-id>/<repo>`) has no port; without discovery a peer
/// needs an explicit socket. Tests and LAN-only deployments set this (or the
/// `?addr=` query) instead of relying on a relay.
pub const ADDRS_ENV: &str = "AGENTIC_REPOS_IROH_ADDRS";

/// Environment variable that disables relays for the client (`0`/`false`).
pub const RELAY_ENV: &str = "AGENTIC_REPOS_IROH_RELAY";

/// One ref advertised by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisedRef {
    pub sha: String,
    pub name: String,
}

/// A parsed `iroh://<node-id>/<repo>` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl {
    pub node_id: NodeId,
    pub repo: String,
    pub addrs: Vec<SocketAddr>,
}

impl RemoteUrl {
    /// Canonical form, without the optional `?addr=` query.
    pub fn canonical(&self) -> String {
        format!("iroh://{}/{}", self.node_id, self.repo)
    }

    /// The dial address for this remote.
    ///
    /// Explicit addresses (URL query or environment) win. Otherwise this is a
    /// Node ID with no sockets, which asks Iroh's discovery to resolve it.
    pub fn endpoint_addr(&self) -> NodeAddr {
        NodeAddr::new(self.node_id).with_direct_addresses(self.addrs.iter().copied())
    }
}

/// Parse the URL form Git hands to a remote helper.
///
/// Accepted: `iroh://<node-id>/<owner>/<repo>.git`,
/// `iroh://<node-id>/<repo>?addr=127.0.0.1:49152`, and repeated/comma-separated
/// `addr` values. `AGENTIC_REPOS_IROH_ADDRS` supplies defaults.
pub fn parse_iroh_url(url: &str) -> Result<RemoteUrl> {
    let rest = url
        .strip_prefix("iroh://")
        .ok_or_else(|| P2pError::InvalidUrl(format!("expected iroh://, got {url}")))?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (node, repo) = path
        .split_once('/')
        .ok_or_else(|| P2pError::InvalidUrl(format!("missing repository in {url}")))?;
    let node_id = NodeId::from_str(node)
        .map_err(|e| P2pError::InvalidUrl(format!("bad node id {node:?}: {e}")))?;
    let repo = repo.trim_matches('/').to_string();
    if repo.is_empty() {
        return Err(P2pError::InvalidUrl(format!("empty repository in {url}")));
    }
    if repo.split('/').any(|part| part == ".." || part == "." || part.is_empty()) {
        return Err(P2pError::InvalidUrl(format!(
            "repository path {repo:?} contains an empty or parent component"
        )));
    }

    let mut addrs = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "addr" {
            parse_addr_list(value, &mut addrs)?;
        }
    }
    if addrs.is_empty() {
        if let Ok(value) = std::env::var(ADDRS_ENV) {
            parse_addr_list(&value, &mut addrs)?;
        }
    }

    Ok(RemoteUrl {
        node_id,
        repo,
        addrs,
    })
}

fn parse_addr_list(value: &str, out: &mut Vec<SocketAddr>) -> Result<()> {
    for item in value.split(',').filter(|s| !s.trim().is_empty()) {
        let socket = item
            .trim()
            .parse::<SocketAddr>()
            .map_err(|e| P2pError::InvalidUrl(format!("bad address {item:?}: {e}")))?;
        out.push(socket);
    }
    Ok(())
}

/// Whether relays are enabled for the client (default true).
pub fn relay_enabled() -> bool {
    match std::env::var(RELAY_ENV) {
        Ok(v) => !(v == "0" || v.eq_ignore_ascii_case("false")),
        Err(_) => true,
    }
}

/// Bind a client endpoint from the user's existing SSH Ed25519 identity.
pub async fn bind_client(secret: SecretKey, relay: bool) -> Result<Endpoint> {
    let mut builder = Endpoint::builder()
        .secret_key(secret)
        .relay_mode(if relay {
            RelayMode::Default
        } else {
            RelayMode::Disabled
        });
    if relay {
        builder = builder.discovery_n0();
    }
    builder
        .bind()
        .await
        .map_err(|e| P2pError::Endpoint(format!("failed to bind client endpoint: {e}")))
}

/// Open an upload-pack stream and send the command line.
async fn open_upload_pack(
    endpoint: &Endpoint,
    addr: NodeAddr,
    repo: &str,
) -> Result<(iroh::endpoint::Connection, iroh::endpoint::SendStream, iroh::endpoint::RecvStream)>
{
    let connection = endpoint
        .connect(addr, GIT_ALPN)
        .await
        .map_err(|e| P2pError::Endpoint(format!("connect to {}: {e}", repo)))?;
    let (mut send, recv) = connection
        .open_bi()
        .await
        .map_err(|e| P2pError::Endpoint(format!("open stream: {e}")))?;
    // The command line is a plain line, not a pkt-line: the server reads it
    // byte-by-byte up to '\n' before the smart protocol starts.
    tokio::io::AsyncWriteExt::write_all(
        &mut send,
        format!("git-upload-pack '{repo}'\n").as_bytes(),
    )
    .await
    .map_err(|e| P2pError::Endpoint(format!("write command: {e}")))?;
    send.flush().await?;
    Ok((connection, send, recv))
}

/// Read the ref advertisement and stop at the flush packet.
async fn read_advertisement(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Vec<AdvertisedRef>> {
    let mut refs = Vec::new();
    loop {
        let packet = read_packet(recv).await?;
        let Some(data) = packet else { break };
        if data.is_empty() {
            break;
        }
        let line = String::from_utf8_lossy(&data);
        let line = line.trim_end_matches('\n');
        // Capabilities ride on the first advertised line, after NUL.
        let (line, _caps) = match line.split_once('\0') {
            Some((head, caps)) => (head, Some(caps)),
            None => (line, None),
        };
        if let Some(reason) = line.strip_prefix("ERR ") {
            return Err(P2pError::Protocol(reason.to_string()));
        }
        let Some((sha, name)) = line.split_once(' ') else {
            continue;
        };
        if name.starts_with("capabilities^{}") || sha.chars().all(|c| c == '0') {
            continue;
        }
        refs.push(AdvertisedRef {
            sha: sha.to_string(),
            name: name.to_string(),
        });
    }
    debug!("P2P Git: advertised {} refs", refs.len());
    Ok(refs)
}

/// List the refs of a repository over P2P.
pub async fn list_refs(
    endpoint: &Endpoint,
    addr: NodeAddr,
    repo: &str,
) -> Result<Vec<AdvertisedRef>> {
    let (_connection, _send, mut recv) = open_upload_pack(endpoint, addr, repo).await?;
    read_advertisement(&mut recv).await
}

/// Fetch the objects reachable from `wants`, writing a raw packfile to `out`.
///
/// No `have` lines are sent, so the pack is self-contained: this is the clone
/// path. The server writes `NAK` as a pkt-line followed by the raw pack (no
/// side-band was requested), which is what the two-phase read below expects.
pub async fn fetch_pack<W: AsyncWrite + Unpin>(
    endpoint: &Endpoint,
    addr: NodeAddr,
    repo: &str,
    wants: &[String],
    out: &mut W,
) -> Result<()> {
    if wants.is_empty() {
        return Ok(());
    }
    let (_connection, mut send, mut recv) = open_upload_pack(endpoint, addr, repo).await?;
    // Drain the advertisement: a want is only valid after it.
    read_advertisement(&mut recv).await?;

    for want in wants {
        write_text_packet(&mut send, &format!("want {want}")).await?;
    }
    write_flush_packet(&mut send).await?;
    send.flush().await?;

    loop {
        let packet = read_packet(&mut recv).await?;
        let Some(data) = packet else {
            return Err(P2pError::Protocol(
                "server closed before sending a pack".to_string(),
            ));
        };
        if data.is_empty() {
            continue;
        }
        let line = String::from_utf8_lossy(&data);
        let line = line.trim_end_matches('\n');
        if let Some(reason) = line.strip_prefix("ERR ") {
            return Err(P2pError::Protocol(reason.to_string()));
        }
        if line == "NAK" || line.starts_with("ACK") {
            break;
        }
    }

    tokio::io::copy(&mut recv, out).await?;
    out.flush().await?;
    send.finish()
        .map_err(|e| P2pError::Endpoint(format!("finish: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_id() -> String {
        // Any valid 32-byte key string; use the real Display so the fixture
        // cannot drift from Iroh's encoding.
        let key = SecretKey::from_bytes(&[9u8; 32]);
        key.public().to_string()
    }

    #[test]
    fn parses_plain_url() {
        let url = format!("iroh://{}/owner/repo.git", node_id());
        let parsed = parse_iroh_url(&url).unwrap();
        assert_eq!(parsed.repo, "owner/repo.git");
        assert!(parsed.addrs.is_empty());
        assert_eq!(parsed.canonical(), url);
    }

    #[test]
    fn parses_explicit_address() {
        let url = format!(
            "iroh://{}/owner/repo.git?addr=127.0.0.1:49152",
            node_id()
        );
        let parsed = parse_iroh_url(&url).unwrap();
        assert_eq!(parsed.addrs, vec!["127.0.0.1:49152".parse().unwrap()]);
        let addr = parsed.endpoint_addr();
        assert_eq!(addr.node_id, parsed.node_id);
        assert_eq!(addr.direct_addresses.len(), 1);
    }

    #[test]
    fn parses_repeated_addresses() {
        let url = format!(
            "iroh://{}/r.git?addr=127.0.0.1:1&addr=127.0.0.1:2,127.0.0.1:3",
            node_id()
        );
        let parsed = parse_iroh_url(&url).unwrap();
        assert_eq!(parsed.addrs.len(), 3);
    }

    #[test]
    fn rejects_bad_inputs() {
        assert!(parse_iroh_url("https://example.com/x").is_err());
        assert!(parse_iroh_url("iroh://not-a-node/repo").is_err());
        assert!(parse_iroh_url(&format!("iroh://{}/a/../b", node_id())).is_err());
        assert!(parse_iroh_url(&format!("iroh://{}/", node_id())).is_err());
    }
}
