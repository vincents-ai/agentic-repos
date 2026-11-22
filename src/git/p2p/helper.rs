//! The `git-remote-iroh` remote-helper protocol.
//!
//! Git invokes a helper named `git-remote-<transport>` and speaks a line
//! protocol on stdin/stdout. This module implements the read-only subset:
//! `capabilities`, `list`, `fetch` and `option`. Push is deliberately absent —
//! the helper advertises no `push` capability rather than accepting a push and
//! failing halfway.
//!
//! `fetch` objects are *stored*, not printed: the helper protocol says the
//! helper writes the necessary objects to the database, so the received pack is
//! piped to `git index-pack --stdin` and a blank line is printed when done.

use std::fmt::Write as _;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::debug;

use super::client::{bind_client, fetch_pack, list_refs, parse_iroh_url, relay_enabled, AdvertisedRef};
use super::identity::client_secret_key;
use super::{P2pError, Result};

/// One command Git sent us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Capabilities,
    List { for_push: bool },
    Fetch { sha: String, name: String },
    Option,
    Blank,
    Other(String),
}

/// Classify one command line.
pub fn parse_request(line: &str) -> Request {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Request::Blank;
    }
    if line == "capabilities" {
        return Request::Capabilities;
    }
    if line == "list" {
        return Request::List { for_push: false };
    }
    if line == "list for-push" {
        return Request::List { for_push: true };
    }
    if let Some(rest) = line.strip_prefix("option ") {
        let _ = rest;
        return Request::Option;
    }
    if let Some(rest) = line.strip_prefix("fetch ") {
        let mut parts = rest.splitn(2, ' ');
        if let (Some(sha), Some(name)) = (parts.next(), parts.next()) {
            return Request::Fetch {
                sha: sha.to_string(),
                name: name.to_string(),
            };
        }
    }
    Request::Other(line.to_string())
}

/// Render the `list` response.
///
/// Git needs a `HEAD` entry to pick the initial checkout. When the server
/// advertises one, it is emitted first with the `@<branch>` attribute so Git
/// also learns the symbolic target.
pub fn render_ref_list(refs: &[AdvertisedRef]) -> String {
    let default_branch = refs
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .or_else(|| refs.iter().find(|r| r.name.starts_with("refs/heads/")))
        .map(|r| r.name.clone());

    let mut out = String::new();
    for reference in refs {
        if reference.name == "HEAD" {
            match &default_branch {
                Some(branch) => {
                    let _ = writeln!(out, "{} HEAD @{}", reference.sha, branch);
                }
                None => {
                    let _ = writeln!(out, "{} HEAD", reference.sha);
                }
            }
        } else {
            let _ = writeln!(out, "{} {}", reference.sha, reference.name);
        }
    }
    out
}

/// Run the helper for a URL until Git closes the command stream.
pub async fn run(url: &str) -> Result<()> {
    let remote = parse_iroh_url(url)?;
    let endpoint = bind_client(client_secret_key(), relay_enabled()).await?;
    let addr = remote.endpoint_addr();
    debug!(node_id = %remote.node_id, repo = %remote.repo, addrs = ?remote.addrs, "git-remote-iroh started");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    let mut cached_refs: Option<Vec<AdvertisedRef>> = None;

    while let Some(line) = lines.next_line().await.map_err(P2pError::Io)? {
        match parse_request(&line) {
            Request::Capabilities => {
                out.write_all(b"fetch\noption\n\n")
                    .await
                    .map_err(P2pError::Io)?;
                out.flush().await.map_err(P2pError::Io)?;
            }
            Request::List { for_push } => {
                if for_push {
                    // No push capability was advertised; refuse rather than
                    // pretend the remote is writable.
                    out.write_all(b"\n").await.map_err(P2pError::Io)?;
                    out.flush().await.map_err(P2pError::Io)?;
                    continue;
                }
                let refs = list_refs(&endpoint, addr.clone(), &remote.repo).await?;
                out.write_all(render_ref_list(&refs).as_bytes())
                    .await
                    .map_err(P2pError::Io)?;
                out.write_all(b"\n").await.map_err(P2pError::Io)?;
                out.flush().await.map_err(P2pError::Io)?;
                cached_refs = Some(refs);
            }
            Request::Fetch { sha, name } => {
                let mut wants = vec![(sha, name)];
                // The rest of the batch, up to its blank line.
                while let Some(next) = lines.next_line().await.map_err(P2pError::Io)? {
                    match parse_request(&next) {
                        Request::Fetch { sha, name } => wants.push((sha, name)),
                        Request::Blank => break,
                        other => debug!("ignoring mid-batch command {other:?}"),
                    }
                }
                if let Some(refs) = &cached_refs {
                    debug!("fetching {} refs; remote has {}", wants.len(), refs.len());
                }
                let shas: Vec<String> = wants.into_iter().map(|(sha, _)| sha).collect();
                store_pack(&endpoint, addr.clone(), &remote.repo, &shas).await?;
                out.write_all(b"\n").await.map_err(P2pError::Io)?;
                out.flush().await.map_err(P2pError::Io)?;
            }
            Request::Option => {
                // Every option is optional; "unsupported" is the documented
                // answer for one we do not implement.
                out.write_all(b"unsupported\n")
                    .await
                    .map_err(P2pError::Io)?;
                out.flush().await.map_err(P2pError::Io)?;
            }
            Request::Blank => break,
            Request::Other(other) => {
                out.write_all(b"unsupported\n").await.map_err(P2pError::Io)?;
                out.flush().await.map_err(P2pError::Io)?;
                debug!("unsupported remote-helper command {other:?}");
            }
        }
    }

    Ok(())
}

/// Fetch a pack from the peer and store its objects in the local repository.
async fn store_pack(
    endpoint: &iroh::Endpoint,
    addr: iroh_base::NodeAddr,
    repo: &str,
    wants: &[String],
) -> Result<()> {
    let mut child = Command::new("git")
        .args(["index-pack", "--stdin"])
        // index-pack prints the resulting pack name on stdout; that stream
        // belongs to the remote-helper protocol, so keep it off it.
        .stdout(Stdio::null())
        .stdin(Stdio::piped())
        .spawn()
        .map_err(P2pError::Io)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| P2pError::Protocol("git index-pack has no stdin".to_string()))?;
    fetch_pack(endpoint, addr, repo, wants, &mut stdin).await?;
    drop(stdin);
    let status = child.wait().await.map_err(P2pError::Io)?;
    if !status.success() {
        return Err(P2pError::Protocol(format!(
            "git index-pack failed with {status}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_commands() {
        assert_eq!(parse_request("capabilities"), Request::Capabilities);
        assert_eq!(parse_request("list"), Request::List { for_push: false });
        assert_eq!(
            parse_request("list for-push"),
            Request::List { for_push: true }
        );
        assert_eq!(parse_request(""), Request::Blank);
        assert_eq!(
            parse_request("fetch 0123456789012345678901234567890123456789 refs/heads/main"),
            Request::Fetch {
                sha: "0123456789012345678901234567890123456789".to_string(),
                name: "refs/heads/main".to_string(),
            }
        );
        assert_eq!(parse_request("option progress true"), Request::Option);
        assert_eq!(
            parse_request("push refs/heads/x:refs/heads/x"),
            Request::Other("push refs/heads/x:refs/heads/x".to_string())
        );
    }

    #[test]
    fn renders_head_with_symref() {
        let refs = vec![
            AdvertisedRef {
                sha: "aa".repeat(20),
                name: "HEAD".to_string(),
            },
            AdvertisedRef {
                sha: "aa".repeat(20),
                name: "refs/heads/main".to_string(),
            },
        ];
        let rendered = render_ref_list(&refs);
        assert!(rendered.starts_with(&format!("{} HEAD @refs/heads/main\n", "aa".repeat(20))));
        assert!(rendered.contains(&format!("{} refs/heads/main\n", "aa".repeat(20))));
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn renders_head_without_branch() {
        let refs = vec![AdvertisedRef {
            sha: "bb".repeat(20),
            name: "HEAD".to_string(),
        }];
        assert_eq!(render_ref_list(&refs), format!("{} HEAD\n", "bb".repeat(20)));
    }
}
