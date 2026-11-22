//! Git receive-pack protocol handler (server-side push)
//!
//! Handles the server side of a `git-receive-pack` session:
//! 1. Advertise refs to the client
//! 2. Read ref update commands
//! 3. Receive the packfile
//! 4. Apply ref updates
//! 5. Send status back

use crate::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::error::{GitProtocolError, Result};
use crate::protocol::upload_pack::RefFilter;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
use gix::refs::FullName;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, info};

/// Callback invoked for each ref update, allowing the consumer to
/// authorize or reject individual updates.
pub trait RefUpdateHook: Send + Sync {
    /// Return `Err(reason)` to reject this ref update, `Ok(())` to allow.
    fn validate(&self, update: &RefUpdate) -> std::result::Result<(), String>;

    /// Called after a ref has been successfully written.
    /// Use this for post-receive hooks, entity lifecycle events, etc.
    fn on_ref_written(&self, _repo_path: &Path, _update: &RefUpdate) {}
}

/// A no-op hook that allows all updates.
pub struct AllowAllUpdates;
impl RefUpdateHook for AllowAllUpdates {
    fn validate(&self, _update: &RefUpdate) -> std::result::Result<(), String> {
        Ok(())
    }
}

/// A single ref update command received from the client.
#[derive(Debug, Clone)]
pub struct RefUpdate {
    pub refname: String,
    pub old_oid: Option<gix::ObjectId>,
    pub new_oid: Option<gix::ObjectId>,
}

/// Result of applying a single ref update.
#[derive(Debug, Clone)]
pub enum RefUpdateResult {
    Ok { refname: String },
    Rejected { refname: String, reason: String },
}

pub struct ReceivePackHandler {
    repo_path: PathBuf,
    ref_filter: Box<dyn RefFilter>,
    update_hook: Box<dyn RefUpdateHook>,
}

impl ReceivePackHandler {
    pub fn new(repo_path: &Path) -> Result<Self> {
        gix::open(repo_path)
            .map_err(|e| GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

        Ok(Self {
            repo_path: repo_path.to_path_buf(),
            ref_filter: Box::new(crate::protocol::upload_pack::ShowAllRefs),
            update_hook: Box::new(AllowAllUpdates),
        })
    }

    /// Set a custom ref filter for advertisement.
    pub fn with_ref_filter(mut self, filter: Box<dyn RefFilter>) -> Self {
        self.ref_filter = filter;
        self
    }

    /// Set a custom hook for validating and post-processing ref updates.
    pub fn with_update_hook(mut self, hook: Box<dyn RefUpdateHook>) -> Self {
        self.update_hook = hook;
        self
    }

    /// Run the full receive-pack protocol.
    pub async fn handle<R, W>(&self, reader: R, writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Self::handle_impl(
            self.repo_path.clone(),
            reader,
            writer,
            &*self.ref_filter,
            &*self.update_hook,
        )
        .await
    }

    async fn handle_impl<R, W>(
        repo_path: std::path::PathBuf,
        mut reader: R,
        mut writer: W,
        ref_filter: &dyn RefFilter,
        update_hook: &dyn RefUpdateHook,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("Starting receive-pack protocol handler");

        Self::advertise_refs_impl(&repo_path, &mut writer, ref_filter).await?;

        let updates = Self::read_updates_impl(&mut reader).await?;

        if updates.is_empty() {
            debug!("No updates received");
            return Ok(());
        }

        Self::receive_packfile_impl(&repo_path, &mut reader).await?;

        let results = Self::apply_updates_impl(&repo_path, &updates, update_hook).await?;

        Self::send_status_impl(&mut writer, &results).await?;

        info!("Receive-pack completed successfully");
        Ok(())
    }

    async fn advertise_refs_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        ref_filter: &dyn RefFilter,
    ) -> Result<()> {
        debug!("Advertising references for receive-pack");

        let capabilities = [
            "report-status",
            "delete-refs",
            "ofs-delta",
            "agent=agentic-git-protocol/0.1.0",
        ];

        let repo_path_clone = repo_path.to_path_buf();
        let (head_target, ref_data) = tokio::task::spawn_blocking(move || -> Result<(Option<gix::ObjectId>, Vec<(String, gix::ObjectId)>)> {
            let repo = gix::open(&repo_path_clone)
                .map_err(|e| GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

            let head_target = repo.head_id().ok().map(|id| id.detach());

            let refs_platform = repo.references().map_err(|e| {
                GitProtocolError::ProtocolError(format!("Failed to list references: {}", e))
            })?;

            let mut results = Vec::new();
            for reference in refs_platform.all().map_err(|e| {
                GitProtocolError::ProtocolError(format!("Failed to iterate references: {}", e))
            })? {
                let reference = reference.map_err(|e| {
                    GitProtocolError::ProtocolError(format!("Failed to read reference: {}", e))
                })?;

                let name = reference.name().as_bstr().to_string();
                if let Some(target_id) = reference.try_id().map(|id| id.detach()) {
                    results.push((name, target_id));
                }
            }
            Ok((head_target, results))
        })
        .await
        .map_err(|e| GitProtocolError::ProtocolError(format!("Blocking task failed: {}", e)))??;

        let mut first = true;
        let mut count = 0;

        if let Some(oid) = head_target {
            let caps = if first { capabilities.join("\0") } else { String::new() };
            let line = format!("{} HEAD\0{}\n", oid.to_hex(), caps);
            write_data_packet(writer, line.as_bytes()).await?;
            first = false;
            count += 1;
        }

        for (name, target) in ref_data {
            if ref_filter.is_filtered(&name) {
                continue;
            }

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
        const MAX_REF_UPDATES: usize = 10000;
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

                // Strip NUL-terminated capabilities from the refname.
                // Git sends: "old_oid new_oid refname\0cap1 cap2\n"
                let refname = if let Some(nul_pos) = refname.find('\0') {
                    &refname[..nul_pos]
                } else {
                    refname
                };

                let old_oid = if old_oid_str == "0000000000000000000000000000000000000000" {
                    None
                } else {
                    Some(gix::ObjectId::from_hex(old_oid_str.as_bytes()).map_err(|e| {
                        GitProtocolError::ProtocolError(format!("Invalid old OID: {}", e))
                    })?)
                };

                let new_oid = if new_oid_str == "0000000000000000000000000000000000000000" {
                    None
                } else {
                    Some(gix::ObjectId::from_hex(new_oid_str.as_bytes()).map_err(|e| {
                        GitProtocolError::ProtocolError(format!("Invalid new OID: {}", e))
                    })?)
                };

                if updates.len() >= MAX_REF_UPDATES {
                    return Err(GitProtocolError::ProtocolError(format!(
                        "Too many ref updates (max {})",
                        MAX_REF_UPDATES
                    )));
                }

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
        debug!("Receiving packfile as raw bytes");

        // Git sends the packfile as raw bytes (not pktline-wrapped) after the
        // ref update commands. The pack starts with "PACK" magic header.
        // Read the full pack until EOF.
        let mut packfile_data = Vec::new();
        reader
            .read_to_end(&mut packfile_data)
            .await
            .map_err(GitProtocolError::Io)?;

        if packfile_data.is_empty() {
            debug!("No packfile data received (empty push or delete-only)");
            return Ok(());
        }

        debug!("Received {} bytes of packfile data", packfile_data.len());

        if !packfile_data.is_empty() {
            let repo_path_clone = repo_path.to_path_buf();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let pack_dir = repo_path_clone.join("objects").join("pack");
                std::fs::create_dir_all(&pack_dir).map_err(|e| {
                    GitProtocolError::Io(std::io::Error::other(format!(
                        "Failed to create pack directory: {}",
                        e
                    )))
                })?;

                let repo = gix::open(&repo_path_clone)
                    .map_err(|e| GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

                let mut pack_reader = std::io::Cursor::new(&packfile_data);
                let interrupt = std::sync::atomic::AtomicBool::new(false);
                let mut prog = gix::progress::Discard;

                let outcome = gix_pack::Bundle::write_to_directory(
                    &mut pack_reader,
                    Some(&pack_dir),
                    &mut prog,
                    &interrupt,
                    Some(&repo),
                    repo.object_hash(),
                    gix_pack::bundle::write::Options::default(),
                )
                .map_err(|e| GitProtocolError::ProtocolError(format!("Failed to write pack: {}", e)))?;

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
            .map_err(|e| GitProtocolError::ProtocolError(format!("Blocking task failed: {}", e)))??;
        }

        Ok(())
    }

    async fn apply_updates_impl(
        repo_path: &Path,
        updates: &[RefUpdate],
        update_hook: &dyn RefUpdateHook,
    ) -> Result<Vec<RefUpdateResult>> {
        // Pre-validate all updates via the hook
        let mut results = Vec::with_capacity(updates.len());
        let mut all_valid = true;

        for update in updates {
            match update_hook.validate(update) {
                Ok(()) => results.push(None), // placeholder, will be filled during apply
                Err(reason) => {
                    all_valid = false;
                    results.push(Some(RefUpdateResult::Rejected {
                        refname: update.refname.clone(),
                        reason,
                    }));
                }
            }
        }

        // If any update was rejected, reject the entire batch
        if !all_valid {
            return Ok(updates
                .iter()
                .zip(results.into_iter())
                .map(|(update, result)| {
                    result.unwrap_or_else(|| RefUpdateResult::Rejected {
                        refname: update.refname.clone(),
                        reason: "Batch rejected: sibling update failed validation".to_string(),
                    })
                })
                .collect());
        }

        // All validated — apply
        let repo_path_clone = repo_path.to_path_buf();
        let updates_owned: Vec<RefUpdate> = updates.to_vec();

        let apply_results =
            tokio::task::spawn_blocking(move || -> Result<Vec<RefUpdateResult>> {
                let repo = gix::open(&repo_path_clone).map_err(|e| {
                    GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e))
                })?;

                let mut apply_results = Vec::with_capacity(updates_owned.len());

                for update in &updates_owned {
                    let ref_name =
                        FullName::try_from(update.refname.as_str()).map_err(|e| {
                            GitProtocolError::ProtocolError(format!(
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
                                GitProtocolError::ProtocolError(format!(
                                    "Failed to update ref {}: {}",
                                    update.refname, e
                                ))
                            })?;

                            info!("Updated {} to {}", update.refname, new_oid.to_hex());
                            apply_results.push(RefUpdateResult::Ok {
                                refname: update.refname.clone(),
                            });
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
                                GitProtocolError::ProtocolError(format!(
                                    "Failed to delete ref {}: {}",
                                    update.refname, e
                                ))
                            })?;

                            info!("Deleted {}", update.refname);
                            apply_results.push(RefUpdateResult::Ok {
                                refname: update.refname.clone(),
                            });
                        }
                        (None, None) => {
                            return Err(GitProtocolError::ProtocolError(
                                "Invalid update: both old and new OID are None".to_string(),
                            ));
                        }
                    }
                }
                Ok(apply_results)
            })
            .await
            .map_err(|e| {
                GitProtocolError::ProtocolError(format!("Blocking task failed: {}", e))
            })??;

        // Fire post-write hooks
        for (update, result) in updates.iter().zip(apply_results.iter()) {
            if let RefUpdateResult::Ok { .. } = result {
                update_hook.on_ref_written(repo_path, update);
            }
        }

        Ok(apply_results)
    }

    async fn send_status_impl<W: AsyncWrite + Unpin>(
        writer: &mut W,
        results: &[RefUpdateResult],
    ) -> Result<()> {
        write_text_packet(writer, "unpack ok").await?;

        for result in results {
            match result {
                RefUpdateResult::Ok { refname } => {
                    write_text_packet(writer, &format!("ok {}", refname)).await?;
                }
                RefUpdateResult::Rejected { refname, reason } => {
                    write_text_packet(writer, &format!("ng {} {}", refname, reason)).await?;
                }
            }
        }

        write_flush_packet(writer).await?;
        writer.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
    use gix::refs::{FullName, Target};
    use tempfile::TempDir;

    fn create_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo = gix::init_bare(dir.path()).unwrap();

        let empty_tree = gix::objs::Tree::empty();
        let tree_id = repo.write_object(&empty_tree).unwrap().detach();

        let author = gix::actor::Signature {
            name: "Test".into(),
            email: "test@example.com".into(),
            time: gix::date::Time::new(1700000000, 0),
        };
        let commit = gix::objs::Commit {
            tree: tree_id,
            parents: Default::default(),
            author: author.clone(),
            committer: author,
            message: "Initial commit\n".into(),
            encoding: None,
            extra_headers: Default::default(),
        };
        let commit_id = repo.write_object(&commit).unwrap().detach();

        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::MustNotExist,
                new: Target::Object(commit_id),
            },
            name: FullName::try_from("refs/heads/main").unwrap(),
            deref: false,
        })
        .unwrap();

        dir
    }

    #[tokio::test]
    async fn test_advertise_refs() {
        let dir = create_test_repo();

        let mut output = Vec::new();
        ReceivePackHandler::advertise_refs_impl(
            dir.path(),
            &mut output,
            &crate::protocol::upload_pack::ShowAllRefs,
        )
        .await
        .unwrap();

        assert!(!output.is_empty());
        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("report-status"));
        assert!(output_str.contains("delete-refs"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let dir = create_test_repo();
        let result = ReceivePackHandler::new(dir.path());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_hook_reject() {
        let dir = create_test_repo();

        struct RejectSecrets;
        impl RefUpdateHook for RejectSecrets {
            fn validate(&self, update: &RefUpdate) -> std::result::Result<(), String> {
                if update.refname.starts_with("refs/secrets/") {
                    Err("No pushing to secrets namespace".to_string())
                } else {
                    Ok(())
                }
            }
        }

        let updates = vec![RefUpdate {
            refname: "refs/secrets/api-key".to_string(),
            old_oid: None,
            new_oid: Some(
                gix::ObjectId::from_hex(b"0000000000000000000000000000000000000001").unwrap(),
            ),
        }];

        let results =
            ReceivePackHandler::apply_updates_impl(dir.path(), &updates, &RejectSecrets)
                .await
                .unwrap();

        assert_eq!(results.len(), 1);
        match &results[0] {
            RefUpdateResult::Rejected { reason, .. } => {
                assert!(reason.contains("secrets"));
            }
            RefUpdateResult::Ok { .. } => panic!("Expected rejection"),
        }
    }

    #[tokio::test]
    async fn test_update_hook_batch_reject() {
        let dir = create_test_repo();

        struct RejectSecrets;
        impl RefUpdateHook for RejectSecrets {
            fn validate(&self, update: &RefUpdate) -> std::result::Result<(), String> {
                if update.refname.starts_with("refs/secrets/") {
                    Err("No pushing to secrets namespace".to_string())
                } else {
                    Ok(())
                }
            }
        }

        let updates = vec![
            RefUpdate {
                refname: "refs/heads/main".to_string(),
                old_oid: None,
                new_oid: Some(
                    gix::ObjectId::from_hex(b"0000000000000000000000000000000000000001").unwrap(),
                ),
            },
            RefUpdate {
                refname: "refs/secrets/api-key".to_string(),
                old_oid: None,
                new_oid: Some(
                    gix::ObjectId::from_hex(b"0000000000000000000000000000000000000002").unwrap(),
                ),
            },
        ];

        let results =
            ReceivePackHandler::apply_updates_impl(dir.path(), &updates, &RejectSecrets)
                .await
                .unwrap();

        // Entire batch rejected because one update failed validation
        assert_eq!(results.len(), 2);
        for result in &results {
            match result {
                RefUpdateResult::Rejected { .. } => {}
                RefUpdateResult::Ok { .. } => panic!("Expected batch rejection"),
            }
        }
    }
}
