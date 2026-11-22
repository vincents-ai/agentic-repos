# agentic-git-protocol

Transport-agnostic server-side Git smart-protocol handlers: command validation,
pkt-line encoding/decoding, client advertise-refs, upload-pack (clone/fetch) and
receive-pack (push) over any `AsyncRead + AsyncWrite` pair. Extracted so
`agentic-repos` and `agentic-loop` share one implementation rather than two
drifting copies.

## Status

**Library only** — no binary in this workspace calls it.

- Public surface: `CommandValidator`/`GitCommand` (`crates/agentic-git-protocol/src/lib.rs:17`), `GitProtocolHandler` (`:18`), `UploadPackHandler` (`:19`), `ReceivePackHandler` (`:20`).
- `GitProtocolHandler::handle_upload_pack` (`crates/agentic-git-protocol/src/protocol/mod.rs:51`) and `handle_receive_pack` (`:66`) dispatch to the two handlers.
- Handler entry points are `UploadPackHandler::handle` (`crates/agentic-git-protocol/src/protocol/upload_pack.rs:56`) and `ReceivePackHandler::handle` (`crates/agentic-git-protocol/src/protocol/receive_pack.rs:84`).
- The only importer in this workspace is `crates/agentic-loop-mesh/src/git_alpn.rs:10-12`; that module is itself declared but never called (`crates/agentic-loop-mesh/src/lib.rs:35`; grep for `handle_git_connection`/`GIT_ALPN` outside the file returns nothing).
- This is expected, not a defect: the crate is shared with another repository (`docs/disconnected-features.md:49`).

## Integration

- `crates/agentic-loop-mesh/src/git_alpn.rs:10` — imports `CommandValidator`, `GitCommand`.
- `crates/agentic-loop-mesh/src/git_alpn.rs:11-12` — imports `ReceivePackHandler` and `UploadPackHandler`.
- `crates/agentic-loop-mesh/src/git_alpn.rs:58` — validates the first command line.
- `crates/agentic-loop-mesh/src/git_alpn.rs:78` / `:87` — constructs the upload/receive handlers.
- No call site in `agentic-loop-bin`, `agentic-core`, or any other crate; grep for `agentic_git_protocol` outside this crate and `mesh/src/git_alpn.rs` is empty.

## Verification

```bash
nix develop --command bash -c 'CARGO_TARGET_DIR=/tmp/opencode/target cargo test -p agentic-git-protocol --no-fail-fast'
```

**20 passed; 0 failed**. No failures to paste.

## Known gaps

- **No binary integration here.** `handle_upload_pack` / `handle_receive_pack` are never called in this repository; the mesh git ALPN adapter that would call them is not attached to any server. This is the documented shared-crate situation (`docs/disconnected-features.md:49`), not a wiring regression.
- **The handler is transport-only.** Authentication and authorization are deliberately outside this crate; `RepoContext.caller_label` (`crates/agentic-git-protocol/src/protocol/mod.rs:17`) is a logging label, not an identity check. A caller must not treat it as authorization.
- **`ReceivePackHandler` carries the same hook-based update path as upload-pack**; there is no built-in quota or repository-size limit in the crate. Limits belong to the transport adapter (the mesh adapter uses a 100 MB read cap).
