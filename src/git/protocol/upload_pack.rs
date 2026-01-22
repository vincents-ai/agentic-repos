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
use std::path::Path;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info};

pub struct UploadPackHandler {
    repo: Repository,
}

impl UploadPackHandler {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

        Ok(Self { repo })
    }

    pub async fn handle<R, W>(&self, mut reader: R, mut writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("Starting upload-pack protocol handler");

        // Send capabilities and references
        self.advertise_refs(&mut writer).await?;

        // Read client wants and haves
        let (wants, haves) = self.read_negotiation(&mut reader).await?;

        if wants.is_empty() {
            debug!("Client wants nothing, ending connection");
            return Ok(());
        }

        // Generate and send packfile
        self.send_packfile(&mut writer, &wants, &haves).await?;

        info!("Upload-pack completed successfully");
        Ok(())
    }

    async fn advertise_refs<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<()> {
        debug!("Advertising references");

        // Get HEAD
        let head = self.repo.head().ok();
        let head_target = head.as_ref().and_then(|h| h.target());

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
        let refs = self
            .repo
            .references()
            .map_err(|e| GitError::ProtocolError(format!("Failed to list references: {}", e)))?;

        for reference in refs {
            let reference = reference
                .map_err(|e| GitError::ProtocolError(format!("Failed to read reference: {}", e)))?;

            if let (Some(name), Some(target)) = (reference.name(), reference.target()) {
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

    async fn read_negotiation<R: AsyncRead + Unpin>(
        &self,
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

    async fn send_packfile<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        wants: &[Oid],
        haves: &[Oid],
    ) -> Result<()> {
        debug!("Generating packfile for {} wants", wants.len());

        // Send NAK (we don't support multi_ack_detailed yet)
        write_text_packet(writer, "NAK").await?;

        // Build packfile using git2
        let mut packbuilder = self
            .repo
            .packbuilder()
            .map_err(|e| GitError::ProtocolError(format!("Failed to create packbuilder: {}", e)))?;

        // Add all wanted objects and their dependencies
        let have_set: HashSet<Oid> = haves.iter().copied().collect();

        for want_oid in wants {
            // Insert the commit and walk its tree
            if let Ok(commit) = self.repo.find_commit(*want_oid) {
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
                let mut revwalk = self.repo.revwalk().map_err(|e| {
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

                    if let Ok(commit) = self.repo.find_commit(oid) {
                        packbuilder.insert_commit(oid).ok();
                        if let Ok(tree) = commit.tree() {
                            packbuilder.insert_tree(tree.id()).ok();
                        }
                    }
                }
            } else if let Ok(_tree) = self.repo.find_tree(*want_oid) {
                // It's a tree
                packbuilder.insert_tree(*want_oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                })?;
            } else if let Ok(_blob) = self.repo.find_blob(*want_oid) {
                // It's a blob
                packbuilder.insert_object(*want_oid, None).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to insert blob: {}", e))
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
        let (_dir, repo) = create_test_repo();
        let handler = UploadPackHandler { repo };

        let mut output = Vec::new();
        handler.advertise_refs(&mut output).await.unwrap();

        // Should contain references
        assert!(!output.is_empty());

        // Should contain capabilities
        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("multi_ack"));
        assert!(output_str.contains("side-band-64k"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let (_dir, _repo) = create_test_repo();
        let result = UploadPackHandler::new(_dir.path());
        assert!(result.is_ok());
    }
}
