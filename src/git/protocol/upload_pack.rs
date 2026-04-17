//! Git upload-pack protocol handler

use super::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::adp::{CallerIdentity, ACCESS_PREFIX, CONFIG_PREFIX, SECRETS_PREFIX};
use crate::git::error::{GitError, Result};
use gix::objs;
use gix_pack::data::{self as pack_data, output};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, info};

pub struct UploadPackHandler {
    repo_path: PathBuf,
    caller_identity: Option<CallerIdentity>,
}

impl UploadPackHandler {
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
        info!("Starting upload-pack protocol handler");
        tracing::debug!("[UPLOAD-PACK] Starting handler");

        Self::advertise_refs_impl(&repo_path, &mut writer, caller_identity.as_ref()).await?;
        tracing::debug!("[UPLOAD-PACK] Advertised refs");

        let (wants, haves, capabilities) = Self::read_negotiation_impl(&mut reader).await?;
        tracing::debug!(
            "[UPLOAD-PACK] Read negotiation: {} wants, capabilities: {:?}",
            wants.len(),
            capabilities
        );

        if wants.is_empty() {
            info!("Client wants nothing, ending connection");
            tracing::debug!("[UPLOAD-PACK] No wants, returning Ok");
            return Ok(());
        }

        Self::send_packfile_impl(&repo_path, &mut writer, &wants, &haves, &capabilities).await?;

        info!("Upload-pack completed successfully");
        tracing::debug!("[UPLOAD-PACK] Completed successfully");
        Ok(())
    }

    async fn advertise_refs_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        caller_identity: Option<&CallerIdentity>,
    ) -> Result<()> {
        debug!("Advertising references");

        let repo_path_clone = repo_path.to_path_buf();
        let can_see_private = caller_identity
            .map(|id| id.can_see_private_refs())
            .unwrap_or(false);
        let (head_target, ref_data) = tokio::task::spawn_blocking(
            #[allow(clippy::type_complexity)]
            move || -> Result<(Option<gix::ObjectId>, Vec<(String, gix::ObjectId)>)> {
                let repo = gix::open(&repo_path_clone).map_err(|e| {
                    GitError::RepoNotFound(format!("Failed to open repository: {}", e))
                })?;

                let head_target = repo.head_id().ok().map(|id| id.detach());

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

                    let name_bytes = reference.name().as_bstr();
                    let name = String::from_utf8_lossy(name_bytes).into_owned();

                    // Strip refs/adp/access/*, refs/adp/config/*, and refs/secrets/* from non-privileged clients
                    let is_private_ref = name.starts_with(ACCESS_PREFIX)
                        || name.starts_with(CONFIG_PREFIX)
                        || name.starts_with(SECRETS_PREFIX);

                    if is_private_ref && !can_see_private {
                        continue;
                    }

                    if let Some(id) = reference.try_id() {
                        results.push((name, id.detach()));
                    }
                }
                Ok((head_target, results))
            },
        )
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        let capabilities = [
            "multi_ack",
            "side-band-64k",
            "ofs-delta",
            "agent=agentic-repos/0.1.0",
        ];

        let mut count = 0;

        if let Some(oid) = head_target {
            let caps = format!(" {}", capabilities.join(" "));
            let line = format!("{} HEAD{}\n", oid.to_hex(), caps);
            write_data_packet(writer, line.as_bytes()).await?;
            count += 1;
        }

        for (name, target) in ref_data {
            let line = format!("{} {}\n", target.to_hex(), name);
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
    ) -> Result<(Vec<gix::ObjectId>, Vec<gix::ObjectId>, HashSet<String>)> {
        let mut wants = Vec::new();
        let mut haves = Vec::new();
        let mut capabilities = HashSet::new();

        tracing::debug!("[UPLOAD-PACK] Starting negotiation read");
        loop {
            tracing::debug!("[UPLOAD-PACK] Reading packet...");
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                Some(_) => {
                    tracing::debug!("[UPLOAD-PACK] Got flush/delim packet, ending negotiation");
                    break;
                }
                None => {
                    tracing::debug!("[UPLOAD-PACK] Got EOF from client");
                    break;
                }
            };

            tracing::debug!("[UPLOAD-PACK] Got packet: {} bytes", packet.len());
            let line = String::from_utf8_lossy(&packet);
            let line = line.trim_end_matches('\n');
            tracing::debug!("[UPLOAD-PACK] Packet content: {:?}", line);

            if line.starts_with("want ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match gix::ObjectId::from_hex(oid_str.as_bytes()) {
                        Ok(oid) => {
                            tracing::debug!("[UPLOAD-PACK] Client wants: {}", oid.to_hex());
                            wants.push(oid);

                            if capabilities.is_empty() && parts.len() > 2 {
                                for cap in &parts[2..] {
                                    tracing::debug!("[UPLOAD-PACK] Client capability: {}", cap);
                                    capabilities.insert(cap.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("[UPLOAD-PACK] Invalid want OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line.starts_with("have ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let oid_str = parts[1];
                    match gix::ObjectId::from_hex(oid_str.as_bytes()) {
                        Ok(oid) => {
                            tracing::debug!("[UPLOAD-PACK] Client has: {}", oid.to_hex());
                            haves.push(oid);
                        }
                        Err(e) => {
                            tracing::debug!("[UPLOAD-PACK] Invalid have OID {}: {}", oid_str, e);
                        }
                    }
                }
            } else if line == "done" {
                tracing::debug!("[UPLOAD-PACK] Client sent done");
                break;
            }
        }

        tracing::debug!(
            "[UPLOAD-PACK] Negotiation complete: {} wants, {} haves, {} capabilities",
            wants.len(),
            haves.len(),
            capabilities.len()
        );
        Ok((wants, haves, capabilities))
    }

    async fn send_packfile_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        wants: &[gix::ObjectId],
        haves: &[gix::ObjectId],
        capabilities: &HashSet<String>,
    ) -> Result<()> {
        debug!("Generating packfile for {} wants", wants.len());

        write_text_packet(writer, "NAK").await?;

        let repo_path_clone = repo_path.to_path_buf();
        let wants_owned: Vec<gix::ObjectId> = wants.to_vec();
        let haves_owned: Vec<gix::ObjectId> = haves.to_vec();
        let packfile_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let repo = gix::open(&repo_path_clone)
                .map_err(|e| GitError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

            let have_set: HashSet<gix::ObjectId> = haves_owned.iter().copied().collect();

            let mut visited: HashSet<gix::ObjectId> = HashSet::new();
            let mut queue: VecDeque<gix::ObjectId> = VecDeque::new();
            let mut objects: Vec<(gix::ObjectId, objs::Kind, Vec<u8>)> = Vec::new();

            for want_id in &wants_owned {
                if !have_set.contains(want_id) {
                    queue.push_back(*want_id);
                }
            }

            while let Some(oid) = queue.pop_front() {
                if visited.contains(&oid) {
                    continue;
                }
                visited.insert(oid);

                let obj = repo.find_object(oid).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to find object {}: {}", oid, e))
                })?;

                match obj.kind {
                    objs::Kind::Commit => {
                        let commit = objs::CommitRef::from_bytes(&obj.data).map_err(|e| {
                            GitError::ProtocolError(format!("Failed to parse commit: {}", e))
                        })?;
                        for parent_id in commit.parents() {
                            let parent_owned = parent_id;
                            if !have_set.contains(&parent_owned) {
                                queue.push_back(parent_owned);
                            }
                        }
                        let tree_id = commit.tree();
                        if !have_set.contains(&tree_id) {
                            queue.push_back(tree_id);
                        }
                    }
                    objs::Kind::Tree => {
                        let tree = objs::TreeRef::from_bytes(&obj.data).map_err(|e| {
                            GitError::ProtocolError(format!("Failed to parse tree: {}", e))
                        })?;
                        for entry in tree.entries.iter() {
                            let entry_oid = gix::ObjectId::from(entry.oid);
                            if !have_set.contains(&entry_oid) {
                                queue.push_back(entry_oid);
                            }
                        }
                    }
                    _ => {}
                }

                objects.push((obj.id, obj.kind, obj.data.clone()));
            }

            let num_entries = objects.len() as u32;
            if num_entries == 0 {
                return Ok(Vec::new());
            }

            let mut entries = Vec::with_capacity(objects.len());
            for (oid, kind, data) in &objects {
                let count = output::Count {
                    id: *oid,
                    entry_pack_location: output::count::PackLocation::NotLookedUp,
                };
                let obj_data = objs::Data::new(*kind, data);
                let entry = output::Entry::from_data(&count, &obj_data).map_err(|e| {
                    GitError::ProtocolError(format!("Failed to create pack entry: {}", e))
                })?;
                entries.push(entry);
            }

            let mut packfile_buf = Vec::new();
            let input: std::iter::Once<
                std::result::Result<Vec<output::Entry>, std::convert::Infallible>,
            > = std::iter::once(Ok(entries));

            let pack_writer = output::bytes::FromEntriesIter::new(
                input,
                &mut packfile_buf,
                num_entries,
                pack_data::Version::V2,
                gix::hash::Kind::Sha1,
            );

            for result in pack_writer {
                result.map_err(|e: output::bytes::Error<std::convert::Infallible>| {
                    GitError::ProtocolError(format!("Pack write error: {}", e))
                })?;
            }

            debug!("Generated packfile: {} bytes", packfile_buf.len());
            Ok(packfile_buf)
        })
        .await
        .map_err(|e| GitError::ProtocolError(format!("Failed to spawn blocking task: {}", e)))??;

        debug!("Generated packfile: {} bytes", packfile_data.len());

        let use_sideband = capabilities.contains("side-band-64k");
        tracing::debug!("[UPLOAD-PACK] Using side-band: {}", use_sideband);

        const CHUNK_SIZE: usize = 65000;

        if use_sideband {
            for chunk in packfile_data.chunks(CHUNK_SIZE) {
                let mut packet_data = vec![1u8];
                packet_data.extend_from_slice(chunk);
                write_data_packet(writer, &packet_data).await?;
            }
        } else {
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
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
    use gix::refs::{FullName, Target};
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, gix::ObjectId) {
        let dir = TempDir::new().unwrap();
        let repo = gix::init(dir.path()).unwrap();

        let empty_tree = gix::objs::TreeRef::empty();
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

        (dir, commit_id)
    }

    #[tokio::test]
    async fn test_advertise_refs() {
        let (dir, _commit_id) = create_test_repo();

        let mut output = Vec::new();
        UploadPackHandler::advertise_refs_impl(dir.path(), &mut output, None)
            .await
            .unwrap();

        assert!(!output.is_empty());

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("multi_ack"));
        assert!(output_str.contains("side-band-64k"));
    }

    #[tokio::test]
    async fn test_new_handler() {
        let (dir, _commit_id) = create_test_repo();
        let result = UploadPackHandler::new(dir.path());
        assert!(result.is_ok());
    }
}
