use sha2::{Digest, Sha256};

pub fn verify_sha256(data: &[u8], expected_oid: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let actual_oid = format!("{:x}", result);
    actual_oid == expected_oid
}

pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_sha256() {
        let data = b"hello world";
        let oid = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_sha256(data, oid));
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let data = b"hello world";
        let oid = "invalid";
        assert!(!verify_sha256(data, oid));
    }

    #[test]
    fn test_compute_sha256() {
        let data = b"hello world";
        let oid = compute_sha256(data);
        assert_eq!(
            oid,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
