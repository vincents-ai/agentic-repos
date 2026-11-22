//! End-to-end tests for P2P Git (milestone 1: clone and fetch over Iroh).
//!
//! Two Iroh endpoints are created in this process — one server, one client — and
//! dialled by explicit loopback address. That is the same fix as
//! `agentic-loop-mesh/tests/two_node_validation.rs`: Iroh binds IPv4 and IPv6 on
//! different ports, so only the IPv4 socket's `127.0.0.1:<port>` is dialable.
//!
//! The clone test then runs the real `git` binary, with `git-remote-iroh` first
//! on `PATH`, and asserts file contents rather than an exit code.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::Command;

use agentic_git_protocol::packetline::read_packet;
use agentic_repos::git::p2p::client::{bind_client, fetch_pack, list_refs};
use agentic_repos::git::p2p::identity::{node_id, secret_key_from_ssh_text};
use agentic_repos::git::p2p::{P2pGitConfig, P2pGitServer};
use iroh_base::SecretKey;
use tokio::io::AsyncWriteExt;

fn run_git(dir: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "P2P Test")
        .env("GIT_AUTHOR_EMAIL", "p2p@example.com")
        .env("GIT_COMMITTER_NAME", "P2P Test")
        .env("GIT_COMMITTER_EMAIL", "p2p@example.com")
        .output()
        .expect("git must be available in the dev shell");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Build a bare fixture repository with two commits and two files.
fn fixture_repo(repos_root: &Path) -> PathBuf {
    let work = repos_root.join("_work");
    std::fs::create_dir_all(&work).unwrap();
    run_git(&work, &["init", "-q", "-b", "main"]);
    std::fs::write(work.join("hello.txt"), b"hello over p2p\n").unwrap();
    std::fs::create_dir_all(work.join("sub")).unwrap();
    std::fs::write(work.join("sub/nested.txt"), b"nested contents\n").unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-qm", "initial"]);
    std::fs::write(work.join("hello.txt"), b"hello over p2p\nsecond line\n").unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-qm", "second"]);

    let bare = repos_root.join("acme/demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    run_git(
        repos_root,
        &[
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    std::fs::remove_dir_all(&work).unwrap();
    bare
}

fn server_port(server: &P2pGitServer) -> u16 {
    server
        .bound_sockets()
        .into_iter()
        .find(|socket| socket.is_ipv4())
        .map(|socket| socket.port())
        .expect("an IPv4 socket")
}

async fn start(repos_root: &Path, seed: u8) -> P2pGitServer {
    let config = P2pGitConfig::new(repos_root.to_path_buf()).without_relay();
    P2pGitServer::bind(SecretKey::from_bytes(&[seed; 32]), config)
        .await
        .expect("server binds")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_endpoints_list_and_fetch_pack() {
    let dir = tempfile::tempdir().unwrap();
    let repos_root = dir.path().join("repos");
    std::fs::create_dir_all(&repos_root).unwrap();
    fixture_repo(&repos_root);

    let server = start(&repos_root, 0x21).await;
    let addr = server
        .ipv4_addr(Ipv4Addr::LOCALHOST)
        .expect("IPv4 address");
    assert_eq!(addr.direct_addresses.len(), 1);
    assert_eq!(
        addr.direct_addresses.iter().next().unwrap().ip(),
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
    );

    // Client endpoint in the same process.
    let client = bind_client(SecretKey::from_bytes(&[0x42; 32]), false)
        .await
        .expect("client binds");

    let refs = list_refs(&client, addr.clone(), "acme/demo.git")
        .await
        .expect("list refs");
    assert!(
        refs.iter().any(|r| r.name == "refs/heads/main"),
        "expected refs/heads/main in {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r.name == "HEAD"),
        "expected HEAD in {refs:?}"
    );
    let head_sha = refs
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .unwrap()
        .sha
        .clone();

    let mut pack = Vec::new();
    fetch_pack(
        &client,
        addr,
        "acme/demo.git",
        &[head_sha.clone()],
        &mut pack,
    )
    .await
    .expect("fetch pack");
    assert!(pack.len() > 32, "pack should not be empty");
    assert_eq!(&pack[..4], b"PACK", "pack magic");

    // Integrity check: index the received pack in a scratch repository.
    let scratch = dir.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    run_git(&scratch, &["init", "-q", "--bare"]);
    let mut child = Command::new("git")
        .args(["index-pack", "--stdin"])
        .current_dir(&scratch)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(&pack).unwrap();
    }
    drop(child.stdin.take());
    let status = child.wait().unwrap();
    assert!(status.success(), "index-pack rejected the fetched pack");

    let commit = run_git(&scratch, &["cat-file", "-p", &head_sha]);
    let commit_text = String::from_utf8_lossy(&commit.stdout);
    assert!(commit_text.contains("second"), "commit subject: {commit_text}");

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_clone_over_iroh_asserts_file_contents() {
    let dir = tempfile::tempdir().unwrap();
    let repos_root = dir.path().join("repos");
    std::fs::create_dir_all(&repos_root).unwrap();
    fixture_repo(&repos_root);

    let server = start(&repos_root, 0x31).await;
    let node = server.node_id_string();
    let port = server_port(&server);
    let destination = dir.path().join("clone");

    let helper = Path::new(env!("CARGO_BIN_EXE_git-remote-iroh"));
    assert!(
        helper.exists(),
        "the git-remote-iroh binary must be built for this test"
    );
    let helper_dir = helper.parent().unwrap();
    let path = format!(
        "{}:{}",
        helper_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // The URL is the user-facing form: iroh://<node-id>/<owner>/<repo>.git.
    // The loopback socket is passed out of band because this test has no relay
    // or discovery; a production clone resolves the Node ID instead.
    let url = format!("iroh://{node}/acme/demo.git");
    let output = Command::new("git")
        .arg("clone")
        .arg(&url)
        .arg(&destination)
        .env("PATH", path)
        .env("AGENTIC_REPOS_IROH_ADDRS", format!("127.0.0.1:{port}"))
        .env("AGENTIC_REPOS_IROH_RELAY", "0")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "git clone {url} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        std::fs::read_to_string(destination.join("hello.txt")).unwrap(),
        "hello over p2p\nsecond line\n"
    );
    assert_eq!(
        std::fs::read_to_string(destination.join("sub/nested.txt")).unwrap(),
        "nested contents\n"
    );
    let count = run_git(&destination, &["rev-list", "--count", "HEAD"]);
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_identity_is_derived_from_ssh_host_key() {
    // Fixed Ed25519 OpenSSH key; the Node ID must be deterministic and equal to
    // the one derived from the same key's seed.
    let openssh = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACDjIgHSg+dK54Ori+cRIh77732AclTNKHd0T8vBrsHYvAAAAJiGu0cYhrtH\n\
GAAAAAtzc2gtZWQyNTUxOQAAACDjIgHSg+dK54Ori+cRIh77732AclTNKHd0T8vBrsHYvA\n\
AAAEDS/cEGOR47Uzh2WbhZVicoyj0ouxr0IU4E+4p+Cea3NuMiAdKD50rng6uL5xEiHvvv\n\
fYByVM0od3RPy8Guwdi8AAAAEXNoaWZ0QGNhcmJvbmFkaXVtAQIDBA==\n\
-----END OPENSSH PRIVATE KEY-----\n";
    let expected = node_id(&secret_key_from_ssh_text(openssh).unwrap()).to_string();

    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("host_key");
    std::fs::write(&key_path, openssh).unwrap();
    std::fs::create_dir_all(dir.path().join("repos")).unwrap();

    let server = P2pGitServer::from_host_key(
        &key_path,
        P2pGitConfig::new(dir.path().join("repos")).without_relay(),
    )
    .await
    .expect("server binds from SSH host key");
    assert_eq!(server.node_id_string(), expected);
    assert_eq!(server.url("acme/demo.git"), format!("iroh://{expected}/acme/demo.git"));
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn receive_pack_is_refused_for_unknown_node_id() {
    let dir = tempfile::tempdir().unwrap();
    let repos_root = dir.path().join("repos");
    std::fs::create_dir_all(&repos_root).unwrap();
    fixture_repo(&repos_root);

    // Empty SSH key directory: this peer is not mapped to any user.
    let auth = std::sync::Arc::new(tokio::sync::RwLock::new(
        agentic_repos::git::auth::AuthorizedKeysManager::from_authorized_keys(
            std::collections::HashMap::new(),
            repos_root.clone(),
        ),
    ));
    let server = P2pGitServer::bind_with_auth(
        SecretKey::from_bytes(&[0x55; 32]),
        P2pGitConfig::new(repos_root.clone()).without_relay(),
        Some(auth),
    )
    .await
    .unwrap();
    let addr = server.ipv4_addr(Ipv4Addr::LOCALHOST).unwrap();
    let client_secret = SecretKey::from_bytes(&[0x66; 32]);
    let peer_id = node_id(&client_secret).to_string();

    let client = bind_client(client_secret, false).await.unwrap();
    let connection = client
        .connect(addr, agentic_repos::git::p2p::GIT_ALPN)
        .await
        .expect("connect on git ALPN");
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    send.write_all(b"git-receive-pack 'acme/demo.git'\n")
        .await
        .unwrap();
    send.flush().await.unwrap();

    let packet = read_packet(&mut recv)
        .await
        .expect("read refusal")
        .expect("a packet");
    let text = String::from_utf8_lossy(&packet);
    assert!(text.starts_with("ERR "), "expected ERR, got {text:?}");
    assert!(
        text.contains("no SSH key"),
        "refusal must name the reason: {text:?}"
    );
    assert!(
        text.contains(&peer_id),
        "refusal must name the peer Node ID: {text:?}"
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clone_of_missing_repository_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let repos_root = dir.path().join("repos");
    std::fs::create_dir_all(&repos_root).unwrap();
    let server = start(&repos_root, 0x71).await;
    let addr = server.ipv4_addr(Ipv4Addr::LOCALHOST).unwrap();
    let client = bind_client(SecretKey::from_bytes(&[0x72; 32]), false)
        .await
        .unwrap();

    let err = list_refs(&client, addr, "acme/missing.git")
        .await
        .expect_err("missing repo must not advertise refs");
    let message = err.to_string();
    assert!(
        message.contains("Repository not found") || message.contains("repository"),
        "unexpected error: {message}"
    );

    server.shutdown().await;
}
