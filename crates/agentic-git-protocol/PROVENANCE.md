# Provenance

Vendored into `agentic-repos` on 2026-11-22 from
`vincents-ai/agentic-loop`, path `crates/agentic-git-protocol`.

- Source revision: `cd4c7566` (`git log -- crates/agentic-git-protocol` in
  agentic-loop; first added in `ad3e902e`).
- Licence: AGPL-3.0-or-later, the same licence as this repository.
- Reason for a copy rather than a git dependency: the crate was extracted *from*
  the `agentic-repos` smart-protocol handlers so both repositories can share one
  implementation. `agentic-repos` is a single-package repository on
  `origin/main`, so the crate is compiled here as a path dependency in
  `crates/agentic-git-protocol` instead of being pulled over the network.
- Changes made in this copy: only the `license-file.workspace = true` line
  (agentic-loop workspace inheritance) was replaced with an explicit
  `license = "AGPL-3.0-or-later"`. No protocol code changed.

When the crate is published or moved to a shared dependency, delete this copy
and point `Cargo.toml` at that dependency.
