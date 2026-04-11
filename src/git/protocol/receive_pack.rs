//! Git receive-pack protocol handler
//!
//! Implements the server-side of git-receive-pack for push operations.

use super::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::adp::{CallerIdentity, ACCESS_PREFIX, CONFIG_PREFIX, SECRETS_PREFIX};
use crate::git::error::{GitError, Result};
use gix::features::progress;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
use gix::refs::FullName;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info};

pub struct ReceivePackHandler {
    repo_path: PathBuf,
    caller_identity: Option<CallerIdentity>,
}

impl ReceivePackHandler {
    pub fn new(repo_path: &Path) -> Result<Self> {
        gix::open(repo_path)
            .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

        Ok(Self {
            repo_path: repo_path.to_path_buf(),
            caller_identity: None,
        })
    }

    pub fn with_caller_identity(mut self, identity: CallerIdentity) -> Self {
        self.caller_identity = Some(identity);
        self
    }

    pub async fn handle<R, W>(&self, reader: R, writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Self::handle_impl(
            self.repo_path.clone(),
            reader,
            writer,
            self.caller_identity.clone(),
        )
        .await
    }

    async fn handle_impl<R, W>(
        repo_path: std::path::PathBuf,
        mut reader: R,
        mut writer: W,
        caller_identity: Option<CallerIdentity>,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("Starting receive-pack protocol handler");

        Self::advertise_refs_impl(&repo_path, &mut writer, caller_identity.as_ref()).await?;

        let updates = Self::read_updates_impl(&mut reader).await?;

        if updates.is_empty() {
            debug!("No updates received");
            return Ok(());
        }

        Self::receive_packfile_impl(&repo_path, &mut reader).await?;

        Self::apply_updates_impl(&repo_path, &updates).await?;

        Self::send_status_impl(&mut writer, &updates).await?;

        info!("Receive-pack completed successfully");
        Ok(())
    }

    async fn advertise_refs_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        caller_identity: Option<&CallerIdentity>,
    ) -> Result<()> {
        debug!("Advertising references for receive-pack");

        let capabilities = vec![
            "report-status",
            "delete-refs",
            "ofs-delta",
            "agent=repos-mcp/0.1.0",
        ];

        let repo_path_clone1 = repo_path.to_path_buf();
        let head_target = tokio::task::spawn_blocking(move || -> Result<Option<gix::ObjectId>> {
            let repo = gix::open(&repo_path_clone1)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            Ok(repo.head_id().ok().map(|id| id.detach()))
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        let repo_path_clone2 = repo_path.to_path_buf();
        let can_see_private = caller_identity
            .map(|id| id.can_see_private_refs())
            .unwrap_or(false);
        let ref_data = tokio::task::spawn_blocking(move || -> Result<Vec<(String, gix::ObjectId)>> {
            let repo = gix::open(&repo_path_clone2)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;
            let refs_platform = repo.references().map_err(|e| {
                GitError::ProtocolError(format!("Failed to list references: {}", e))
            })?;

            let mut results = Vec::new();
            for reference in refs_platform.all().map_err(|e| {
                GitError::ProtocolError(format!("Failed to iterate references: {}", e))
            })? {
                let reference = reference.map_err(|e| {
                    GitError::ProtocolError(format!("Failed to read reference: {}", e))
                })?;

                let name = reference.name().as_bstr().to_string();
                let target = reference.try_id().map(|id| id.detach());

                if let Some(target_id) = target {
                    // Strip refs/adp/access/*, refs/adp/config/*, and refs/secrets/* from non-privileged clients
                    let is_private_ref = name.starts_with(ACCESS_PREFIX) 
                        || name.starts_with(CONFIG_PREFIX) 
                        || name.starts_with(SECRETS_PREFIX);
                    
                    if is_private_ref && !can_see_private {
                        // Skip this ref - it's private and client can't see private refs
                    } else {
                        results.push((name, target_id));
                    }
                }
            }
            Ok(results)
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        let mut first = true;
        let mut count = 0;

        if let Some(oid) = head_target {
            let caps = if first {
                format!(" {}", capabilities.join(" "))
            } else {
                String::new()
            };

            let line = format!("{} HEAD{}\n", oid.to_hex(), caps);
            write_data_packet(writer, line.as_bytes()).await?;
            first = false;
            count += 1;
        }

        for (name, target) in ref_data {
            let caps = if first {
                format!(" {}", capabilities.join(" "))
            } else {
                String::new()
            };

            let line = format!("{} {}{}\n", target.to_hex(), name, caps);
            write_data_packet(writer, line.as_bytes()).await?;
            first = false;
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

    async fn read_updates_impl<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<RefUpdate>> {
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
                        Some(gix::ObjectId::from_hex(old_oid_str.as_bytes()).map_err(|e| {
                            GitError::ProtocolError(format!("Invalid old OID: {}", e))
                        })?)
                    };

                let new_oid =
                    if new_oid_str == "0000000000000000000000000000000000000000" {
                        None
                    } else {
                        Some(gix::ObjectId::from_hex(new_oid_str.as_bytes()).map_err(|e| {
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

    async fn receive_packfile_impl<R: AsyncRead + Unpin>(
        repo_path: &Path,
        reader: &mut R,
    ) -> Result<()> {
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
                let err = String::from_utf8_lossy(&packet[1..]);
                error!("Error: {}", err);
            }
        }

        if !packfile_data.is_empty() {
            let repo_path_clone = repo_path.to_path_buf();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let pack_dir = repo_path_clone.join("objects").join("pack");
                std::fs::create_dir_all(&pack_dir).map_err(|e| {
                    GitError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to create pack directory: {}", e),
                    ))
                })?;

                let repo = gix::open(&repo_path_clone).map_err(|e| {
                    GitError::RepoNotFound(format!(
                        "Failed to open repository: {}",
                        e
                    ))
                })?;

                let mut pack_reader = std::io::Cursor::new(&packfile_data);
                let interrupt = std::sync::atomic::AtomicBool::new(false);
                let mut prog = progress::Discard;

                let outcome = gix_pack::Bundle::write_to_directory(
                    &mut pack_reader,
                    Some(&pack_dir),
                    &mut prog,
                    &interrupt,
                    Some(&repo),
                    gix_pack::bundle::write::Options::default(),
                )
                .map_err(|e| {
                    GitError::ProtocolError(format!(
                        "Failed to write pack: {}",
                        e
                    ))
                })?;

                debug!(
                    "Received and indexed pack: {} objects, {} bytes",
                    outcome.index.num_objects,
                    packfile_data.len()
                );

                if let Some(keep_path) = outcome.keep_path {
                    let _ = std::fs::remove_file(keep_path);
                }

                Ok(())
            })
            .await
            .map_err(|e| {
                GitError::ProtocolError(format!(
                    "Failed to spawn blocking task: {}",
                    e
                ))
            })??;
        }

        Ok(())
    }

    async fn apply_updates_impl(repo_path: &Path, updates: &[RefUpdate]) -> Result<()> {
        let repo_path_clone = repo_path.to_path_buf();
        let updates_owned: Vec<RefUpdate> = updates
            .iter()
            .map(|u| RefUpdate {
                refname: u.refname.clone(),
                old_oid: u.old_oid,
                new_oid: u.new_oid,
            })
            .collect();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let repo = gix::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

            for update in &updates_owned {
                let ref_name = FullName::try_from(update.refname.as_str()).map_err(|e| {
                    GitError::ProtocolError(format!(
                        "Invalid ref name {}: {}",
                        update.refname, e
                    ))
                })?;

                match (&update.old_oid, &update.new_oid) {
                    (_, Some(new_oid)) => {
                        repo.edit_reference(RefEdit {
                            change: Change::Update {
                                log: LogChange::default(),
                                expected: PreviousValue::Any,
                                new: gix::refs::Target::Object(*new_oid),
                            },
                            name: ref_name,
                            deref: false,
                        })
                        .map_err(|e| {
                            GitError::ProtocolError(format!(
                                "Failed to update ref {}: {}",
                                update.refname, e
                            ))
                        })?;

                        info!("Updated {} to {}", update.refname, new_oid.to_hex());
                    }
                    (Some(_), None) => {
                        repo.edit_reference(RefEdit {
                            change: Change::Delete {
                                expected: PreviousValue::MustExist,
                                log: gix::refs::transaction::RefLog::AndReference,
                            },
                            name: ref_name,
                            deref: false,
                        })
                        .map_err(|e| {
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
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        Ok(())
    }

    async fn send_status_impl<W: AsyncWrite + Unpin>(
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

#[derive(Debug, Clone)]
struct RefUpdate {
    refname: String,
    old_oid: Option<gix::ObjectId>,
    new_oid: Option<gix::ObjectId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, gix::Repository) {
        let dir = TempDir::new().unwrap();
        let repo = gix::init_bare(dir.path()).unwrap();
        (dir, repo)
    }

    #[tokio::test]
    async fn test_advertise_refs() {
        let (dir, _repo) = create_test_repo();

        let mut output = Vec::new();
        ReceivePackHandler::advertise_refs_impl(dir.path(), &mut output, None)
            .await
            .unwrap();

        assert!(!output.is_empty());

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("report-status"));
        assert!(output_str.contains("delete-refs"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let (dir, _repo) = create_test_repo();
        let result = ReceivePackHandler::new(dir.path());
        assert!(result.is_ok());
    }
}
