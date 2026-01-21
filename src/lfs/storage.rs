use async_trait::async_trait;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageKey {
    pub tenant: String,
    pub repo: String,
    pub oid: String,
}

impl StorageKey {
    pub fn new(tenant: impl Into<String>, repo: impl Into<String>, oid: impl Into<String>) -> Self {
        Self {
            tenant: tenant.into(),
            repo: repo.into(),
            oid: oid.into(),
        }
    }

    pub fn to_path(&self, pattern: &str) -> String {
        let oid_prefix_2 = if self.oid.len() >= 2 {
            &self.oid[0..2]
        } else {
            &self.oid
        };
        let oid_prefix_4 = if self.oid.len() >= 4 {
            &self.oid[2..4]
        } else {
            ""
        };

        pattern
            .replace("{tenant}", &self.tenant)
            .replace("{repo}", &self.repo)
            .replace("{oid}", &self.oid)
            .replace("{oid[0:2]}", oid_prefix_2)
            .replace("{oid[2:4]}", oid_prefix_4)
    }
}

#[async_trait]
pub trait LfsStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn get_object(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, Self::Error>;

    async fn put_object(&self, key: &StorageKey, data: Vec<u8>) -> Result<(), Self::Error>;

    async fn object_exists(&self, key: &StorageKey) -> Result<bool, Self::Error>;

    async fn object_size(&self, key: &StorageKey) -> Result<Option<u64>, Self::Error>;

    async fn delete_object(&self, key: &StorageKey) -> Result<(), Self::Error>;

    async fn get_upload_url(
        &self,
        key: &StorageKey,
        expires_in: Duration,
    ) -> Result<String, Self::Error>;

    async fn get_download_url(&self, key: &StorageKey) -> Result<String, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key_to_path() {
        let key = StorageKey::new("tenant1", "repo1", "abcdef1234567890");
        let path = key.to_path("{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}");
        assert_eq!(path, "tenant1/repo1.git/lfs/objects/ab/cd/abcdef1234567890");
    }

    #[test]
    fn test_storage_key_to_path_short_oid() {
        let key = StorageKey::new("tenant1", "repo1", "ab");
        let path = key.to_path("{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}");
        assert_eq!(path, "tenant1/repo1.git/lfs/objects/ab//ab");
    }
}
