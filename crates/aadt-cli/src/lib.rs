//! Library surface for the AAD CLI command registrations (OPEN/SAVE, assist, …).
//!
//! The `aadt` binary is thin; WinUI / [`aadt_ffi`] call [`register`] so a Session
//! gets the same host commands as `aadt shell`.

pub mod aorms_bridge;
mod commands;
pub mod connect_session;
mod enterprise;
pub mod shell;
mod site_ingest;

pub use commands::register;
pub use enterprise::parse_role;
