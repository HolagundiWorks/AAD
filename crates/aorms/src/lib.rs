//! AAD's connection to AORMS (docs/status.md; the contract is `docs/esti/AORMS-SUITE-SIGNIN.md` in the AORMS repository).
//!
//! One AORMS sign-in is shared by the desktop apps on a machine (AAD, AQC); each app has its own session, so they do not replace each
//! other. Everything is optional: signed out, AAD is a complete standalone drafting program. This crate is layer 3 and knows nothing
//! about drawings; the engine maps what it returns onto the drawing's project data.
//!
//! Errors are values. Server answers are untrusted: sizes are capped and every field is read defensively.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod client;
mod error;
pub mod mapping;
mod model;
mod store;
mod transport;

pub use client::{Client, RestoreOutcome};
pub use error::{AormsError, Result};
pub use model::{Capabilities, OnlineProject, ProjectBinding, SessionInfo, Studio};
pub use store::{MemorySessionStore, MemorySharedStore, SessionStore, SharedSignIn, SharedSignInStore};
pub use transport::{Body, Method, Request, Response, Transport};

#[cfg(not(target_arch = "wasm32"))]
pub use transport::UreqTransport;

#[cfg(all(feature = "keyring-store", not(target_arch = "wasm32")))]
pub use store::KeyringStore;

/// The wire contract this client implements. A server announcing another is refused ("update AAD").
pub const SUPPORTED_CONTRACT: &str = "2026-10-aqc-1";
/// Our app id in `POST /session`; sessions are one per user per app.
pub const APP_ID: &str = "aad";
