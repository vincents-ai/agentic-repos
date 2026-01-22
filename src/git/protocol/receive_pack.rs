//! Git receive-pack protocol handler
//!
//! Implements the server-side of git-receive-pack for push operations.

use super::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::git::error::{GitError, Result};
use git2::{Oid, Repository};
use std::path::Path;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info};

pub struct ReceivePackHandler {
    repo: Repository,
}

impl ReceivePackHandler {
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
        info!("Starting receive-pack protocol handler");

        self.advertise_refs(&mut writer).await?;

        let updates = self.read_updates(&mut reader).await?;

        if updates.is_empty() {
            debug!("No updates received");
            return Ok(());
        }

        self.receive_packfile(&mut reader).await?;

        self.apply_updates(&updates).await?;

        self.send_status(&mut writer, &updates).await?;

        info!("Receive-pack completed successfully");
        Ok(())
    }

    async fn advertise_refs<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<()> {
        debug!("Advertising references for receive-pack");

        let capabilities = vec![
            "report-status",
            "delete-refs",
            "ofs-delta",
            "agent=repos-mcp/0.1.0",
        ];

        let head = self.repo.head().ok();
        let head_target = head.as_ref().and_then(|h| h.target());

        let mut first = true;
        let mut count = 0;

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

    async fn read_updates<R: AsyncRead + Unpin>(&self, reader: &mut R) -> Result<Vec<RefUpdate>> {
        let mut updates = Vec::new();

        loop {
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                _ => break,
            };

            let line = String::from_utf8_lossy(&packet);
            let line = line.trim_end_matches('\n');

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let old_oid_str = parts[0];
                let new_oid_str = parts[1];
                let refname = parts[2];

                let old_oid =
                    if old_oid_str == "0000000000000000000000000000000000000000" {
                        None
                    } else {
                        Some(Oid::from_str(old_oid_str).map_err(|e| {
                            GitError::ProtocolError(format!("Invalid old OID: {}", e))
                        })?)
                    };

                let new_oid =
                    if new_oid_str == "0000000000000000000000000000000000000000" {
                        None
                    } else {
                        Some(Oid::from_str(new_oid_str).map_err(|e| {
                            GitError::ProtocolError(format!("Invalid new OID: {}", e))
                        })?)
                    };

                updates.push(RefUpdate {
                    refname: refname.to_string(),
                    old_oid,
                    new_oid,
                });

                debug!("Update: {} -> {} ({})", old_oid_str, new_oid_str, refname);
            }
        }

        Ok(updates)
    }

    async fn receive_packfile<R: AsyncRead + Unpin>(&self, reader: &mut R) -> Result<()> {
        debug!("Receiving packfile");

        let mut packfile_data = Vec::new();

        loop {
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                _ => break,
            };

            if packet[0] == 1 {
                packfile_data.extend_from_slice(&packet[1..]);
            } else if packet[0] == 2 {
                let progress = String::from_utf8_lossy(&packet[1..]);
                debug!("Progress: {}", progress);
            } else if packet[0] == 3 {
                let error = String::from_utf8_lossy(&packet[1..]);
                error!("Error: {}", error);
            }
        }

        if !packfile_data.is_empty() {
            use std::io::Write;

            let odb = self
                .repo
                .odb()
                .map_err(|e| GitError::ProtocolError(format!("Failed to get ODB: {}", e)))?;

            let mut packwriter = odb.packwriter().map_err(|e| {
                GitError::ProtocolError(format!("Failed to create packwriter: {}", e))
            })?;

            packwriter
                .write_all(&packfile_data)
                .map_err(|e| GitError::IoError(e))?;

            packwriter.commit().map_err(|e| {
                GitError::ProtocolError(format!("Failed to commit packfile: {}", e))
            })?;

            debug!(
                "Received and indexed packfile: {} bytes",
                packfile_data.len()
            );
        }

        Ok(())
    }

    async fn apply_updates(&self, updates: &[RefUpdate]) -> Result<()> {
        for update in updates {
            match (&update.old_oid, &update.new_oid) {
                (_, Some(new_oid)) => {
                    self.repo
                        .reference(&update.refname, *new_oid, true, "push")
                        .map_err(|e| {
                            GitError::ProtocolError(format!(
                                "Failed to update ref {}: {}",
                                update.refname, e
                            ))
                        })?;
                    info!("Updated {} to {}", update.refname, new_oid);
                }
                (Some(_), None) => {
                    let mut reference = self.repo.find_reference(&update.refname).map_err(|e| {
                        GitError::ProtocolError(format!(
                            "Failed to find ref {}: {}",
                            update.refname, e
                        ))
                    })?;

                    reference.delete().map_err(|e| {
                        GitError::ProtocolError(format!(
                            "Failed to delete ref {}: {}",
                            update.refname, e
                        ))
                    })?;
                    info!("Deleted {}", update.refname);
                }
                (None, None) => {
                    return Err(GitError::ProtocolError(
                        "Invalid update: both old and new OID are None".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    async fn send_status<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        updates: &[RefUpdate],
    ) -> Result<()> {
        write_text_packet(writer, "unpack ok").await?;

        for update in updates {
            let status = format!("ok {}", update.refname);
            write_text_packet(writer, &status).await?;
        }

        write_flush_packet(writer).await?;
        writer.flush().await?;

        Ok(())
    }
}

#[derive(Debug)]
struct RefUpdate {
    refname: String,
    old_oid: Option<Oid>,
    new_oid: Option<Oid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init_bare(dir.path()).unwrap();
        (dir, repo)
    }

    #[tokio::test]
    async fn test_advertise_refs() {
        let (_dir, repo) = create_test_repo();
        let handler = ReceivePackHandler { repo };

        let mut output = Vec::new();
        handler.advertise_refs(&mut output).await.unwrap();

        assert!(!output.is_empty());

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("report-status"));
        assert!(output_str.contains("delete-refs"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let (_dir, _repo) = create_test_repo();
        let result = ReceivePackHandler::new(_dir.path());
        assert!(result.is_ok());
    }
}
