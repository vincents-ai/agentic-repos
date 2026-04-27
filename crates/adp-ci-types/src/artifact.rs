//! Artifact types — build outputs and storage configuration.

use serde::{Deserialize, Serialize};

/// Reference to a build artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Artifact name (e.g. "binary", "coverage-report").
    pub name: String,
    /// Storage path (LFS path, S3 key, or ref path).
    pub path: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// SHA-256 digest.
    pub sha256: String,
}

/// Artifact storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorage {
    /// Git LFS (default).
    GitLfs,
    /// S3-compatible object storage.
    S3,
    /// Google Cloud Storage.
    GCS,
    /// Local filesystem.
    Local,
}

impl Default for ArtifactStorage {
    fn default() -> Self {
        ArtifactStorage::GitLfs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_ref_round_trip() {
        let artifact = ArtifactRef {
            name: "binary".into(),
            path: "artifacts/adp-linux-amd64".into(),
            size_bytes: 42_000_000,
            sha256: "abc123def456".into(),
        };
        let json = serde_json::to_string(&artifact).unwrap();
        let decoded: ArtifactRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "binary");
    }

    #[test]
    fn artifact_storage_default_is_lfs() {
        assert_eq!(ArtifactStorage::default(), ArtifactStorage::GitLfs);
    }
}
