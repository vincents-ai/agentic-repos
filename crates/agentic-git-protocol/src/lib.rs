//! # agentic-git-protocol
//!
//! Git smart-protocol handlers extracted for reuse across agentic-repos and agentic-loop.
//!
//! Provides:
//! - **CommandValidator** — parse and validate git-upload-pack / git-receive-pack commands
//! - **GitProtocolHandler** — dispatch to upload-pack or receive-pack handlers
//! - **UploadPackHandler** — server-side clone/fetch via git smart-protocol
//! - **ReceivePackHandler** — server-side push via git smart-protocol
//! - **packet-line** — pkt-line encoding/decoding for the git wire protocol
//!
//! All handlers take `AsyncRead + AsyncWrite` trait objects, making them
//! transport-agnostic (SSH channels, Iroh QUIC streams, TCP, etc.).

pub mod command;
pub mod error;
pub mod packetline;
pub mod protocol;

pub use command::{CommandValidationError, CommandValidator, GitCommand};
pub use error::{GitProtocolError, Result};
pub use protocol::{GitProtocolHandler, ReceivePackHandler, UploadPackHandler};
