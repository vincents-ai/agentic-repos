# P2P Git

P2P Git lets `git` clone and fetch a repository over Iroh's QUIC network,
without SSH, HTTP or DNS. It is an **additional** transport: the SSH and HTTP
paths are unchanged.

Vocabulary follows `docs/CONTEXT-P2P-GIT.md` (vendored from ADP). The
user-facing name is **P2P Git**; "Iroh" is the implementation and appears only
in code, configuration and module names.

## URL

```
iroh://<node-id>/<owner>/<repo>.git
```

`<node-id>` is the base32 Node ID: the 32-byte Ed25519 public key of the server's
P2P Identity. There is no hostname and no port in the URL.

## Identity

The P2P Identity **is the existing SSH Ed25519 keypair**. No new secret file is
created and no new key is generated.

| Side | Key source |
|------|------------|
| Server | the SSH host key at `GitConfig::host_key_path` (default `host_key`) |
| Client | `~/.ssh/id_ed25519`, or `$AGENTIC_REPOS_SSH_KEY` when set |

An OpenSSH Ed25519 private key is parsed with the `ssh-key` crate; a raw
base64-encoded 32-byte seed is also accepted (used by tests and by the mesh
key format). The server fails to start its P2P listener when the host key is
missing or is not Ed25519 — it never falls back to a random identity, because
that would silently change every URL that pointed at the server.

The derived `SecretKey` is the same seed, so the Node ID is the SSH public key:

```
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...  ←→  aej7y3...  (node id)
```

For push authorisation, the peer's Node ID is treated as the SSH Ed25519 public
key it *is*: `AuthorizedKeysManager::user_for_ed25519_public_key` encodes the
32 key bytes into the SSH wire format and compares them with
`refs/adp/users/<user>/ssh_keys`. A peer that completes the QUIC handshake has
already proven possession of that key.

## ALPN names

| ALPN | Purpose | Status |
|------|---------|--------|
| `agentic-repos/git/1` | Git smart protocol (`git-upload-pack`, `git-receive-pack`) | served; upload-pack complete, receive-pack served |
| `agentic-repos/blobs/1` | LFS objects via `iroh-blobs` | **reserved, not implemented** (milestone 3) |

The git ALPN carries the same bytes as a Git-over-SSH exec channel: one command
line (`git-upload-pack '<repo>'`), then the standard smart-protocol stream. The
handlers are `agentic-git-protocol::{UploadPackHandler, ReceivePackHandler}` —
the same crate ADP uses — adapted from an SSH channel to a QUIC stream.

## Client: `git-remote-iroh`

`src/bin/git-remote-iroh.rs` implements the read-only remote-helper protocol:

- `capabilities` → `fetch`, `option`
- `list` → refs from the server's advertisement, with `HEAD @<branch>`
- `fetch <sha> <name>` (batched) → the received pack is piped to
  `git index-pack --stdin`, then a blank line is printed
- `option …` → `unsupported`

Push is **not** advertised. A helper that claims `push` and then fails halfway
would be worse than one that does not offer it.

### Install and clone

```bash
cargo build --release --bin git-remote-iroh
install -m755 target/release/git-remote-iroh ~/.local/bin/   # any dir on PATH

git clone iroh://<node-id>/acme/demo.git
```

Loopback/LAN deployments without discovery can pass the socket out of band:

```bash
AGENTIC_REPOS_IROH_ADDRS=127.0.0.1:49152 git clone iroh://<node-id>/acme/demo.git
```

or `iroh://<node-id>/acme/demo.git?addr=127.0.0.1:49152`. `AGENTIC_REPOS_IROH_RELAY=0`
disables relays client-side. Neither variable is needed when Iroh discovery can
resolve the Node ID.

## Server configuration

The listener is off by default. In `AppConfig`:

```yaml
p2p:
  enabled: true
  relay_enabled: true      # false for LAN-only
  repos_root: null         # defaults to the storage root
```

On startup the server logs its Node ID and the URL template. If the host key
cannot be read the listener logs an error and the SSH/HTTP servers continue.

## Tests

`tests/p2p_git_clone.rs` creates a server endpoint and a client endpoint in one
process, dialling `127.0.0.1:<ipv4-port>` explicitly (Iroh binds IPv4 and IPv6
on different ports, so the IPv6 port must not be mapped onto IPv4). It then:

1. lists refs and fetches a pack at the protocol level, checking `PACK` magic and
   indexing the pack with `git index-pack`;
2. runs a real `git clone` with `git-remote-iroh` first on `PATH` and asserts
   file **contents** and a two-commit history;
3. checks the server Node ID equals the Node ID derived from a fixed SSH key;
4. checks a receive-pack from an unmapped Node ID is refused with an `ERR`
   naming the peer.

Run with:

```bash
nix develop --command cargo test --test p2p_git_clone
```

## Not implemented

- **Push client.** The server serves `git-receive-pack` and refuses unmapped
  peers, but `git-remote-iroh` does not advertise `push`, so there is no
  end-to-end push yet.
- **LFS.** The `agentic-repos/blobs/1` ALPN and `iroh-blobs` transfer are not
  implemented; LFS pointer files do not resolve over P2P.
- **Incremental fetch.** `fetch` sends wants with no `have` lines, so the pack is
  self-contained (correct, but not bandwidth-optimal). `git index-pack` removes
  the duplicates on the client.
