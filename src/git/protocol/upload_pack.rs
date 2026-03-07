//! Git upload-pack protocol handler

use super::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::git::error::{GitError, Result};
use git2::{Oid, Repository};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, info};

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
        eprintln!("[UPLOAD-PACK] Starting handler");

        Self::advertise_refs_impl(&repo_path, &mut writer).await?;
        eprintln!("[UPLOAD-PACK] Advertised refs");

        let (wants, haves, capabilities) = Self::read_negotiation_impl(&mut reader).await?;
        eprintln!("[UPLOAD-PACK] Read negotiation: {} wants, capabilities: {:?}", wants.len(), capabilities);

        if wants.is_empty() {
            info!("Client wants nothing, ending connection");
            eprintln!("[UPLOAD-PACK] No wants, returning Ok");
            return Ok(());
        }

        Self::send_packfile_impl(&repo_path, &mut writer, &wants, &haves, &capabilities).await?;

        info!("Upload-pack completed successfully");
        eprintln!("[UPLOAD-PACK] Completed successfully");
        Ok(())
    }

    async fn advertise_refs_impl<W: AsyncWrite + Unpin>(repo_path: &Path, writer: &mut W) -> Result<()> {
        debug!("Advertising references");

        let repo_path_clone = repo_path.to_path_buf();
        let head_target = tokio::task::spawn_blocking(move || -> Result<Option<Oid>> {
            let repo = Repository::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            Ok(repo.head().ok().and_then(|h| h.target()))
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        let capabilities = vec![
            "multi_ack",
            "side-band-64k",
            "ofs-delta",
            "agent=repos-mcp/0.1.0",
        ];

        let mut count = 0;

        if let Some(oid) = head_target {
            let caps = format!(" {}", capabilities.join(" "));
            let line = format!("{} HEAD{}\n", oid, caps);
            write_data_packet(writer, line.as_bytes()).await?;
            count += 1;
        }

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
            let line = format!("{} {}\n", target, name);
            write_data_packet(writer, line.as_bytes()).await?;
            count += 1;
        }

        if count == 0 {
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
    ) -> Result<(Vec<Oid>, Vec<Oid>, HashSet<String>)> {
        let mut wants = Vec::new();
        let mut haves = Vec::new();
        let mut capabilities = HashSet::new();

        eprintln!("[UPLOAD-PACK] Starting negotiation read");
        loop {
            eprintln!("[UPLOAD-PACK] Reading packet...");
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                Some(_) => {
                    eprintln!("[UPLOAD-PACK] Got flush/delim packet, ending negotiation");
                    break;
                }
                None => {
                    eprintln!("[UPLOAD-PACK] Got EOF from client");
                    break;
                }
            };

            eprintln!("[UPLOAD-PACK] Got packet: {} bytes", packet.len());
            let line = String::from_utf8_lossy(&packet);
            let line = line.trim_end_matches('\n');
            eprintln!("[UPLOAD-PACK] Packet content: {:?}", line);

            if line.starts_with("want ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match Oid::from_str(oid_str) {
                        Ok(oid) => {
                            eprintln!("[UPLOAD-PACK] Client wants: {}", oid);
                            wants.push(oid);
                            
                            // Parse capabilities from the first want line
                            // Format: "want <sha> cap1 cap2 cap3"
                            if capabilities.is_empty() && parts.len() > 2 {
                                for cap in &parts[2..] {
                                    eprintln!("[UPLOAD-PACK] Client capability: {}", cap);
                                    capabilities.insert(cap.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[UPLOAD-PACK] Invalid want OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line.starts_with("have ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match Oid::from_str(oid_str) {
                        Ok(oid) => {
                            eprintln!("[UPLOAD-PACK] Client has: {}", oid);
                            haves.push(oid);
                        }
                        Err(e) => {
                            eprintln!("[UPLOAD-PACK] Invalid have OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line == "done" {
                eprintln!("[UPLOAD-PACK] Client sent done");
                break;
            }
        }

        eprintln!("[UPLOAD-PACK] Negotiation complete: {} wants, {} haves, {} capabilities", 
                  wants.len(), haves.len(), capabilities.len());
        Ok((wants, haves, capabilities))
    }

    async fn send_packfile_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        wants: &[Oid],
        haves: &[Oid],
        capabilities: &HashSet<String>,
    ) -> Result<()> {
        debug!("Generating packfile for {} wants", wants.len());

        write_text_packet(writer, "NAK").await?;

        let repo_path_clone = repo_path.to_path_buf();
        let wants_owned: Vec<Oid> = wants.to_vec();
        let haves_owned: Vec<Oid> = haves.to_vec();
        let packfile_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let repo_clone = Repository::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            let mut packbuilder = repo_clone
                .packbuilder()
                .map_err(|e| GitError::ProtocolError(format!("Failed to create packbuilder: {}", e)))?;

            let have_set: HashSet<Oid> = haves_owned.iter().copied().collect();

            for want_oid in &wants_owned {
                if let Ok(commit) = repo_clone.find_commit(*want_oid) {
                    packbuilder.insert_commit(*want_oid).map_err(|e| {
                        GitError::ProtocolError(format!("Failed to insert commit: {}", e))
                    })?;

                    if let Ok(tree) = commit.tree() {
                        packbuilder.insert_tree(tree.id()).map_err(|e| {
                            GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                        })?;
                    }

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
                    packbuilder.insert_tree(*want_oid).map_err(|e| {
                        GitError::ProtocolError(format!("Failed to insert tree: {}", e))
                    })?;
                }
            }

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

        // Check if client requested side-band-64k capability
        let use_sideband = capabilities.contains("side-band-64k");
        eprintln!("[UPLOAD-PACK] Using side-band: {}", use_sideband);

        const CHUNK_SIZE: usize = 65000;

        if use_sideband {
            // Use side-band protocol with channel marker
            for chunk in packfile_data.chunks(CHUNK_SIZE) {
                let mut packet_data = vec![1u8]; // Channel 1 = packfile data
                packet_data.extend_from_slice(chunk);
                write_data_packet(writer, &packet_data).await?;
            }
        } else {
            // Send packfile directly without side-band
            for chunk in packfile_data.chunks(CHUNK_SIZE) {
                write_data_packet(writer, chunk).await?;
            }
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

        assert!(!output.is_empty());

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
