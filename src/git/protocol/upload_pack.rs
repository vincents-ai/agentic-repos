//! Git upload-pack protocol handler
//!
//! Implements the server-side of git-upload-pack for clone and fetch operations.
//! This handler responds to client requests by:
//! 1. Advertising available references
//! 2. Negotiating which objects the client needs
//! 3. Generating and sending a packfile

use super::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::git::error::{GitError, Result};
use git2::{Oid, Repository};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info};

pub struct UploadPackHandler {
    repo_path: PathBuf,
}

impl UploadPackHandler {
    pub fn new(repo_path: &Path) -> Result<Self> {
        Repository::open(repo_path)
            .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

        Ok(Self { repo_path: repo_path.to_path_buf() })
    }

    pub async fn handle<R, W>(&self, reader: R, writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Self::handle_impl(self.repo_path.clone(), reader, writer).await
    }

    async fn handle_impl<R, W>(repo_path: std::path::PathBuf, mut reader: R, mut writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("Starting upload-pack protocol handler");

        // Send capabilities and references
        Self::advertise_refs_impl(&repo_path, &mut writer).await?;

        // Read client wants and haves
        let (wants, haves) = Self::read_negotiation_impl(&mut reader).await?;

        if wants.is_empty() {
            debug!("Client wants nothing, ending connection");
            return Ok(());
        }

        // Generate and send packfile
        Self::send_packfile_impl(&repo_path, &mut writer, &wants, &haves).await?;

        info!("Upload-pack completed successfully");
        Ok(())
    }

    async fn advertise_refs_impl<W: AsyncWrite + Unpin>(repo_path: &Path, writer: &mut W) -> Result<()> {
        debug!("Advertising references");

        // Get HEAD OID (extract before any async operations)
        let repo_path_clone = repo_path.to_path_buf();
        let head_target = tokio::task::spawn_blocking(move || -> Result<Option<Oid>> {
            let repo = Repository::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            Ok(repo.head().ok().and_then(|h| h.target()))
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        // Capabilities we support
        let capabilities = vec![
            "multi_ack",
            "side-band-64k",
            "ofs-delta",
            "agent=repos-mcp/0.1.0",
        ];

        let mut first = true;
        let mut count = 0;

        // Advertise HEAD
        if let Some(oid) = head_target {
            let caps = if first {
                format!(" {}", capabilities.join(" "))
            } else {
                String::new()
            };

            let line = format!("{} HEAD{}\n", oid, caps);
            write_data_packet(writer, line.as_bytes()).await?;
            first = false;
            count += 1;
        }

        // Advertise all references
        // Collect all reference data before async operations to avoid Send issues
        let repo_path_clone2 = repo_path.to_path_buf();
        let ref_data = tokio::task::spawn_blocking(move || -> Result<Vec<(String, Oid)>> {
            let repo = Repository::open(&repo_path_clone2)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            let refs = repo
                .references()
                .map_err(|e| GitError::ProtocolError(format!("Failed to list references: {}", e)))?;

            let mut results = Vec::new();
            for reference in refs {
                let reference = reference
                    .map_err(|e| GitError::ProtocolError(format!("Failed to read reference: {}", e)))?;

                if let (Some(name), Some(target)) = (reference.name(), reference.target()) {
                    results.push((name.to_string(), target));
                }
            }
            Ok(results)
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        for (name, target) in ref_data {
            let caps = if first {
                format!(" {}", capabilities.join(" "))
            } else {
                String::new()
            };

            let line = format!("{} {}{}\n", target, name, caps);
            write_data_packet(writer, line.as_bytes()).await?;
            first = false;
            count += 1;
        }

        if count == 0 {
            // Empty repository - send capabilities only
            let line = format!(
                "0000000000000000000000000000000000000000 capabilities^{}\n",
                capabilities.join(" ")
            );
            write_data_packet(writer, line.as_bytes()).await?;
        }

        write_flush_packet(writer).await?;
        writer.flush().await?;

        debug!("Advertised {} references", count);
        Ok(())
    }

    async fn read_negotiation_impl<R: AsyncRead + Unpin>(
        reader: &mut R,
    ) -> Result<(Vec<Oid>, Vec<Oid>)> {
        let mut wants = Vec::new();
        let mut haves = Vec::new();

        loop {
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                _ => break, // Flush or EOF
            };

            let line = String::from_utf8_lossy(&packet);
            let line = line.trim_end_matches('\n');

            if line.starts_with("want ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match Oid::from_str(oid_str) {
                        Ok(oid) => {
                            debug!("Client wants: {}", oid);
                            wants.push(oid);
                        }
                        Err(e) => {
                            error!("Invalid want OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line.starts_with("have ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match Oid::from_str(oid_str) {
                        Ok(oid) => {
                            debug!("Client has: {}", oid);
                            haves.push(oid);
                        }
                        Err(e) => {
                            error!("Invalid have OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line == "done" {
                debug!("Client sent done");
                break;
            }
        }

        Ok((wants, haves))
    }

    async fn send_packfile_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        wants: &[Oid],
        haves: &[Oid],
    ) -> Result<()> {
        debug!("Generating packfile for {} wants", wants.len());

        // Send NAK (we don't support multi_ack_detailed yet)
        write_text_packet(writer, "NAK").await?;

        // Build packfile using git2 (move to blocking task to avoid Send issues)
        let repo_path_clone = repo_path.to_path_buf();
        let wants_owned: Vec<Oid> = wants.to_vec();
        let haves_owned: Vec<Oid> = haves.to_vec();
        let packfile_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let repo_clone = Repository::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            let mut packbuilder = repo_clone
                .packbuilder()
                .map_err(|e| GitError::ProtocolError(format!("Failed to create packbuilder: {}", e)))?;

            // Add all wanted objects and their dependencies
            let have_set: HashSet<Oid> = haves_owned.iter().copied().collect();

            for want_oid in &wants_owned {
                // Insert the commit and walk its tree
                if let Ok(commit) = repo_clone.find_commit(*want_oid) {
                packbuilder.insert_commit(*want_oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to insert commit: {}", e))
                })?;

                // Insert the tree
                if let Ok(tree) = commit.tree() {
                    packbuilder.insert_tree(tree.id()).map_err(|e| {
                        GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                    })?;
                }

                // Walk parents (but skip those the client has)
                let mut revwalk = repo_clone.revwalk().map_err(|e| {
                    GitError::ProtocolError(format!("Failed to create revwalk: {}", e))
                })?;

                revwalk.push(*want_oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to push to revwalk: {}", e))
                })?;

                for oid_result in revwalk {
                    let oid = oid_result
                        .map_err(|e| GitError::ProtocolError(format!("Revwalk error: {}", e)))?;

                    if have_set.contains(&oid) {
                        continue;
                    }

                    if let Ok(commit) = repo_clone.find_commit(oid) {
                        packbuilder.insert_commit(oid).ok();
                        if let Ok(tree) = commit.tree() {
                            packbuilder.insert_tree(tree.id()).ok();
                        }
                    }
                }
            } else if let Ok(_tree) = repo_clone.find_tree(*want_oid) {
                // It's a tree
                packbuilder.insert_tree(*want_oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                })?;
            } else if let Ok(_blob) = repo_clone.find_blob(*want_oid) {
                // It's a blob
                packbuilder.insert_tree(*want_oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                })?;
            }
        }

            // Write packfile to a buffer
            let mut packfile_data = Vec::new();
            packbuilder
                .foreach(|data| {
                    packfile_data.extend_from_slice(data);
                    true
                })
            .map_err(|e| GitError::ProtocolError(format!("Failed to generate packfile: {}", e)))?;

            debug!("Generated packfile: {} bytes", packfile_data.len());
            Ok(packfile_data)
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        debug!("Generated packfile: {} bytes", packfile_data.len());

        // Send packfile in chunks using side-band-64k format
        // Side-band format: 1 byte channel (1=data, 2=progress, 3=error) + data
        const CHUNK_SIZE: usize = 65000;

        for chunk in packfile_data.chunks(CHUNK_SIZE) {
            let mut packet_data = vec![1u8]; // Channel 1 = pack data
            packet_data.extend_from_slice(chunk);
            write_data_packet(writer, &packet_data).await?;
        }

        write_flush_packet(writer).await?;
        writer.flush().await?;

        info!("Sent packfile: {} bytes", packfile_data.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        drop(tree);

        (dir, repo)
    }

    #[tokio::test]
    async fn test_advertise_refs() {
        let (dir, _repo) = create_test_repo();
        
        let mut output = Vec::new();
        UploadPackHandler::advertise_refs_impl(dir.path(), &mut output).await.unwrap();

        // Should contain references
        assert!(!output.is_empty());

        // Should contain capabilities
        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("multi_ack"));
        assert!(output_str.contains("side-band-64k"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let (dir, _repo) = create_test_repo();
        let result = UploadPackHandler::new(dir.path());
        assert!(result.is_ok());
    }
}
