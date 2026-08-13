//! WebSocket remote-control API for Zellij.
//!
//! See `REMOTE_API.md` at the repository root for the protocol and the design
//! rationale. In short: this crate is a headless multi-session Zellij client
//! that exposes session/tab/pane management, input injection and a git-style
//! diff history of everything that appears on screen over a WebSocket.

pub mod canvas;
pub mod diff;
pub mod keys;
pub mod protocol;
pub mod server;
pub mod session_link;
pub mod sessions;
