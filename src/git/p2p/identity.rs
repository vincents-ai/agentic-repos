//! P2P Identity — the Iroh Ed25519 keypair, derived from the SSH keypair.
//!
//! There is no new secret. The server derives its endpoint identity from the SSH
//! host key it already uses; the client derives it from `~/.ssh/id_ed25519`
//! (override with `AGENTIC_REPOS_SSH_KEY`). A Node ID is the public half of that
//! keypair, so a peer that completes the QUIC handshake has already proven
//! possession of the SSH key it claims.

use std::path::{Path, PathBuf};

use iroh_base::{NodeId, SecretKey};
use tracing::warn;

use super::P2pError;

/// Environment variable that overrides the client-side SSH key path.
pub const SSH_KEY_ENV: &str = "AGENTIC_REPOS_SSH_KEY";

/// Read an SSH Ed25519 private key (OpenSSH format) or a raw base64 32-byte seed.
///
/// Returns an error rather than a random key when the file cannot be parsed:
/// a silent random server identity would break every URL that pointed at the
/// configured host key.
pub fn secret_key_from_ssh_file(path: &Path) -> Result<SecretKey, P2pError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        P2pError::Key(format!(
            "cannot read SSH key {}: {e}",
            path.display()
        ))
    })?;
    secret_key_from_ssh_text(&content)
        .map_err(|e| P2pError::Key(format!("{}: {e}", path.display())))
}

/// Parse the text of an SSH Ed25519 private key or a raw base64 32-byte seed.
pub fn secret_key_from_ssh_text(content: &str) -> Result<SecretKey, P2pError> {
    if content.contains("-----BEGIN") {
        let private_key = ssh_key::PrivateKey::from_openssh(content)
            .map_err(|e| P2pError::Key(format!("invalid OpenSSH key: {e}")))?;
        let keypair = private_key
            .key_data()
            .ed25519()
            .ok_or_else(|| P2pError::Key("SSH key is not Ed25519".to_string()))?;
        let seed = &keypair.private.as_ref()[..32];
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| P2pError::Key("invalid Ed25519 seed length".to_string()))?;
        return Ok(secret_key_from_seed(seed));
    }

    // Raw base64 seed (test fixtures and meshes that already store this format).
    let b64: String = content
        .lines()
        .filter(|line| {
            !line.starts_with("-----") && !line.starts_with("Comment:") && !line.is_empty()
        })
        .collect();
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| P2pError::Key(format!("not OpenSSH and not base64: {e}")))?;
    if decoded.len() != 32 {
        return Err(P2pError::Key(format!(
            "expected an OpenSSH Ed25519 key or a 32-byte seed, got {} bytes",
            decoded.len()
        )));
    }
    let seed: [u8; 32] = decoded
        .try_into()
        .map_err(|_| P2pError::Key("invalid seed length".to_string()))?;
    Ok(secret_key_from_seed(seed))
}

fn secret_key_from_seed(seed: [u8; 32]) -> SecretKey {
    SecretKey::from_bytes(&seed)
}

/// The Node ID (public half) for a secret key.
pub fn node_id(secret: &SecretKey) -> NodeId {
    secret.public()
}

/// Lower-case hex of the Node ID's 32 public-key bytes.
///
/// The URL form uses Iroh's base32 `NodeId` display; this hex form is used
/// in logs and audit lines because it matches the fingerprint style used by the
/// existing SSH path.
pub fn node_id_hex(id: &NodeId) -> String {
    hex::encode(id.as_bytes())
}

/// Default client key path: `$AGENTIC_REPOS_SSH_KEY`, then `~/.ssh/id_ed25519`.
pub fn default_client_key_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(SSH_KEY_ENV) {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".ssh").join("id_ed25519"))
}

/// Client identity: the user's existing SSH Ed25519 key.
///
/// Read-only fetches do not need the key for authorisation in milestone 1, but
/// using it means the same identity the SSH transport would present is the one
/// the peer sees. When no key exists, an ephemeral key is generated and a warning
/// is printed; that identity cannot be authorised for push and is not persisted.
pub fn client_secret_key() -> SecretKey {
    if let Some(path) = default_client_key_path() {
        match secret_key_from_ssh_file(&path) {
            Ok(key) => return key,
            Err(e) => warn!(
                "P2P Git: no usable SSH identity at {} ({e}); using an ephemeral key",
                path.display()
            ),
        }
    } else {
        warn!("P2P Git: HOME is not set; using an ephemeral identity");
    }

    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("operating system randomness unavailable");
    secret_key_from_seed(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_seed_round_trip_matches_public_key() {
        let seed = [7u8; 32];
        use base64::Engine as _;
        let text = base64::engine::general_purpose::STANDARD.encode(seed);
        let key = secret_key_from_ssh_text(&text).unwrap();
        assert_eq!(node_id_hex(&node_id(&key)), hex::encode(seed));
    }

    #[test]
    fn openssh_key_derives_public_key() {
        // Fixed test key: `ssh-keygen -t ed25519 -N ''`. Its public key is
        // ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMiAdKD50rng6uL5xEiHvvvfYByVM0od3RPy8Guwdi8.
        let openssh = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACDjIgHSg+dK54Ori+cRIh77732AclTNKHd0T8vBrsHYvAAAAJiGu0cYhrtH\n\
GAAAAAtzc2gtZWQyNTUxOQAAACDjIgHSg+dK54Ori+cRIh77732AclTNKHd0T8vBrsHYvA\n\
AAAEDS/cEGOR47Uzh2WbhZVicoyj0ouxr0IU4E+4p+Cea3NuMiAdKD50rng6uL5xEiHvvv\n\
fYByVM0od3RPy8Guwdi8AAAAEXNoaWZ0QGNhcmJvbmFkaXVtAQIDBA==\n\
-----END OPENSSH PRIVATE KEY-----\n";
        let derived = secret_key_from_ssh_text(openssh).unwrap();
        // Node ID is the 32-byte Ed25519 public key, so deriving from the SSH
        // key and from its raw seed must agree.
        let seed_b64 = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(derived.to_bytes())
        };
        let from_seed = secret_key_from_ssh_text(&seed_b64).unwrap();
        assert_eq!(node_id(&derived).as_bytes(), node_id(&from_seed).as_bytes());
        assert_eq!(node_id_hex(&node_id(&derived)).len(), 64);
    }

    #[test]
    fn non_ed25519_key_is_rejected() {
        // Fixed ECDSA P-256 test key.
        let openssh = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAaAAAABNlY2RzYS\n\
1zaGEyLW5pc3RwMjU2AAAACG5pc3RwMjU2AAAAQQSWiOZeqvoqRl8j+ZH8Ot/Q4IDDEpao\n\
NzpBS9pzyOND9UM/1UPQ20SiisrDIitMtznlMClIE2ConfIyYtCvjYgcAAAAsOQNgPvkDY\n\
D7AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBJaI5l6q+ipGXyP5\n\
kfw639DggMMSlqg3OkFL2nPI40P1Qz/VQ9DbRKKKysMiK0y3OeUwKUgTYKid8jJi0K+NiB\n\
wAAAAhAJuspRWNM4xcEzkAEzO23btBjVwkPXA6qGfSKdLgPDPcAAAAEXNoaWZ0QGNhcmJv\n\
bmFkaXVtAQIDBAUG\n\
-----END OPENSSH PRIVATE KEY-----\n";
        let err = secret_key_from_ssh_text(openssh).unwrap_err();
        assert!(err.to_string().contains("not Ed25519"));
    }
}
