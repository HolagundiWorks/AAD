//! AAD's local language model runtime.
//!
//! Qwen3-1.7B runs inside the application through llama.cpp, loaded from a GGUF file. The weights
//! are not in the source tree: the installer ships them and a developer points `AAD_MODEL` at one
//! (see [`files`]). Nothing here knows about CAD: the agent crate turns a request into a prompt,
//! this crate generates text, and the agent crate checks every proposed tool call before anything
//! touches a drawing.
//!
//! - [`prompt`]: the Qwen3 chat format, built as text; testable without a model.
//! - [`files`]: finding and checking the model file.
//! - [`engine`]: the runtime ([`Model`]): load, chat, stream, cancel.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod engine;
pub mod files;
#[cfg(feature = "llama")]
mod llama;
pub mod prompt;
pub mod worker;

pub use engine::{Completion, LlmError, LoadOptions, Model, Params, Stop, available};
pub use files::{FileProblem, MODEL_ENV, MODEL_FILE, find_model};
pub use prompt::{Message, Role, build_prompt, strip_thinking};
