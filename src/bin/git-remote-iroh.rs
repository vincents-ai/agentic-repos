//! `git-remote-iroh` — teach `git` to speak `iroh://` URLs.
//!
//! Install this binary as `git-remote-iroh` on `PATH`, then:
//!
//! ```text
//! git clone iroh://<node-id>/<owner>/<repo>.git
//! ```
//!
//! The P2P identity is the user's existing SSH Ed25519 key
//! (`~/.ssh/id_ed25519`, override with `AGENTIC_REPOS_SSH_KEY`). No new secret
//! is created.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args();
    let _program = args.next();
    // Git invokes: git-remote-iroh <remote-name> <url>
    let _remote_name = args.next();
    let Some(url) = args.next() else {
        eprintln!("usage: git-remote-iroh <remote> <iroh://node-id/repo>");
        return ExitCode::from(2);
    };

    match agentic_repos::git::p2p::helper::run(&url).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("git-remote-iroh: {e}");
            ExitCode::FAILURE
        }
    }
}
