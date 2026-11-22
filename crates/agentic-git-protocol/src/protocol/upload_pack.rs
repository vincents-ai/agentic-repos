//! Git upload-pack protocol handler (server-side clone/fetch)
//!
//! Handles the server side of a `git-upload-pack` session:
//! 1. Advertise refs to the client
//! 2. Read want/have negotiation
//! 3. Generate and send the packfile

use crate::packetline::{read_packet, write_data_packet, write_flush_packet, write_text_packet};
use crate::error::{GitProtocolError, Result};
use gix::objs;
use gix_pack::data::{self as pack_data, output};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, info};

/// Callback to determine if a ref should be filtered from advertisement.
///
/// Return `true` to **exclude** the ref from the client's view.
pub trait RefFilter: Send + Sync {
    fn is_filtered(&self, ref_name: &str) -> bool;
}

/// A no-op filter that shows all refs.
pub struct ShowAllRefs;
impl RefFilter for ShowAllRefs {
    fn is_filtered(&self, _ref_name: &str) -> bool {
        false
    }
}

pub struct UploadPackHandler {
    repo_path: PathBuf,
    ref_filter: Box<dyn RefFilter>,
}

impl UploadPackHandler {
    pub fn new(repo_path: &Path) -> Result<Self> {
        // Verify the repo can be opened
        gix::open(repo_path)
            .map_err(|e| GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

        Ok(Self {
            repo_path: repo_path.to_path_buf(),
            ref_filter: Box::new(ShowAllRefs),
        })
    }

    /// Set a custom ref filter (e.g. to hide private refs from non-privileged callers).
    pub fn with_ref_filter(mut self, filter: Box<dyn RefFilter>) -> Self {
        self.ref_filter = filter;
        self
    }

    /// Run the full upload-pack protocol.
    pub async fn handle<R, W>(&self, reader: R, writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Self::handle_impl(self.repo_path.clone(), reader, writer, &*self.ref_filter).await
    }

    async fn handle_impl<R, W>(
        repo_path: std::path::PathBuf,
        mut reader: R,
        mut writer: W,
        ref_filter: &dyn RefFilter,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("Starting upload-pack protocol handler");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            Self::handle_impl_inner(&repo_path, &mut reader, &mut writer, ref_filter),
        )
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                tracing::debug!("[UPLOAD-PACK] Handler timed out");
                Ok(())
            }
        }
    }

    async fn handle_impl_inner<R, W>(
        repo_path: &std::path::Path,
        reader: &mut R,
        writer: &mut W,
        ref_filter: &dyn RefFilter,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Self::advertise_refs_impl(repo_path, writer, ref_filter).await?;
        debug!("[UPLOAD-PACK] Advertised refs");

        let negotiation_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::read_negotiation_impl(reader),
        )
        .await;

        let (wants, haves, capabilities) = match negotiation_result {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                debug!("[UPLOAD-PACK] Negotiation timed out, treating as no wants");
                (Vec::new(), Vec::new(), HashSet::new())
            }
        };
        debug!(
            "[UPLOAD-PACK] Read negotiation: {} wants, {} haves",
            wants.len(),
            haves.len()
        );

        if wants.is_empty() {
            info!("Client wants nothing, ending connection");
            return Ok(());
        }

        Self::send_packfile_impl(repo_path, writer, &wants, &haves, &capabilities).await?;

        info!("Upload-pack completed successfully");
        Ok(())
    }

    /// Advertise refs to the client.
    pub async fn advertise_refs_impl<W: AsyncWrite + Unpin>(
        repo_path: &Path,
        writer: &mut W,
        ref_filter: &dyn RefFilter,
    ) -> Result<()> {
        debug!("Advertising references");

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

                let name_bytes = reference.name().as_bstr();
                let name = String::from_utf8_lossy(name_bytes).into_owned();

                if let Some(id) = reference.try_id() {
                    results.push((name, id.detach()));
                }
            }
            Ok((head_target, results))
        })
        .await
        .map_err(|e| GitProtocolError::ProtocolError(format!("Blocking task failed: {}", e)))??;

        let capabilities = [
            "multi_ack",
            "side-band-64k",
            "ofs-delta",
            "agent=agentic-git-protocol/0.1.0",
        ];

        let mut count = 0;

        if let Some(oid) = head_target {
            let caps = capabilities.join("\0");
            let line = format!("{} HEAD\0{}\n", oid.to_hex(), caps);
            write_data_packet(writer, line.as_bytes()).await?;
            count += 1;
        }

        for (name, target) in ref_data {
            // Apply the caller's ref filter
            if ref_filter.is_filtered(&name) {
                continue;
            }

            let line = format!("{} {}\n", target.to_hex(), name);
            write_data_packet(writer, line.as_bytes()).await?;
            count += 1;
        }

        if count == 0 {
            let line = format!(
                "0000000000000000000000000000000000000000 capabilities^{}\n",
                capabilities.join("\0")
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
        const MAX_WANTS: usize = 10000;
        const MAX_HAVES: usize = 100000;
        let mut wants = Vec::new();
        let mut haves = Vec::new();
        let mut capabilities = HashSet::new();

        loop {
            let packet = match read_packet(reader).await? {
                Some(data) if !data.is_empty() => data,
                Some(_) => break,
                None => break,
            };

            let line = String::from_utf8_lossy(&packet);
            let line = line.trim_end_matches('\n');

            if line.starts_with("want ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(oid) = gix::ObjectId::from_hex(parts[1].as_bytes()) {
                        if wants.len() < MAX_WANTS {
                            wants.push(oid);
                        }
                        if capabilities.is_empty() && parts.len() > 2 {
                            for cap in &parts[2..] {
                                capabilities.insert(cap.to_string());
                            }
                        }
                    }
                }
            } else if line.starts_with("have ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(oid) = gix::ObjectId::from_hex(parts[1].as_bytes()) {
                        if haves.len() < MAX_HAVES {
                            haves.push(oid);
                        }
                    }
                }
            } else if line == "done" {
                break;
            }
        }

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
                .map_err(|e| GitProtocolError::RepoNotFound(format!("Failed to open repository: {}", e)))?;

            let object_hash = repo.object_hash();
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
                    GitProtocolError::ProtocolError(format!("Failed to find object {}: {}", oid, e))
                })?;

                match obj.kind {
                    objs::Kind::Commit => {
                        let commit = objs::CommitRef::from_bytes(&obj.data, object_hash).map_err(|e| {
                            GitProtocolError::ProtocolError(format!("Failed to parse commit: {}", e))
                        })?;
                        for parent_id in commit.parents() {
                            if !have_set.contains(&parent_id) {
                                queue.push_back(parent_id);
                            }
                        }
                        let tree_id = commit.tree();
                        if !have_set.contains(&tree_id) {
                            queue.push_back(tree_id);
                        }
                    }
                    objs::Kind::Tree => {
                        let tree = objs::TreeRef::from_bytes(&obj.data, object_hash).map_err(|e| {
                            GitProtocolError::ProtocolError(format!("Failed to parse tree: {}", e))
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
                let obj_data = objs::Data::new(data, *kind, object_hash);
                let entry = output::Entry::from_data(&count, &obj_data, gix::zlib::Compression::DEFAULT).map_err(|e| {
                    GitProtocolError::ProtocolError(format!("Failed to create pack entry: {}", e))
                })?;
                entries.push(entry);
            }

            let mut packfile_buf = Vec::new();
            let input = std::iter::once(Ok(entries));

            let pack_writer = output::bytes::FromEntriesIter::new(
                input,
                &mut packfile_buf,
                num_entries,
                pack_data::Version::V2,
                gix::hash::Kind::Sha1,
            );

            for result in pack_writer {
                result.map_err(|e| {
                    GitProtocolError::ProtocolError(format!("Pack write error: {}", e))
                })?;
            }

            debug!("Generated packfile: {} bytes", packfile_buf.len());
            Ok(packfile_buf)
        })
        .await
        .map_err(|e| GitProtocolError::ProtocolError(format!("Blocking task failed: {}", e)))??;

        debug!("Sending packfile: {} bytes", packfile_data.len());

        let use_sideband = capabilities.contains("side-band-64k");
        const CHUNK_SIZE: usize = 65000;

        if use_sideband {
            for chunk in packfile_data.chunks(CHUNK_SIZE) {
                let mut packet_data = vec![1u8];
                packet_data.extend_from_slice(chunk);
                write_data_packet(writer, &packet_data).await?;
            }
            write_flush_packet(writer).await?;
        } else {
            writer.write_all(&packfile_data).await?;
        }
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
        UploadPackHandler::advertise_refs_impl(dir.path(), &mut output, &ShowAllRefs)
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

    #[tokio::test]
    async fn test_ref_filter() {
        let (dir, _commit_id) = create_test_repo();

        struct FilterPrivate;
        impl RefFilter for FilterPrivate {
            fn is_filtered(&self, name: &str) -> bool {
                name.starts_with("refs/adp/")
            }
        }

        let mut output = Vec::new();
        UploadPackHandler::advertise_refs_impl(dir.path(), &mut output, &FilterPrivate)
            .await
            .unwrap();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("refs/heads/main"));
    }
}
