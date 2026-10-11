//! The runtime's public face: errors, settings and [`Model`]. The llama.cpp code behind it is in
//! `llama.rs` and is built only with the `llama` feature; without it every call reports that this
//! build has no language model, so callers always have one honest code path.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::files::{self, FileProblem};
use crate::prompt::{Message, build_prompt};

#[derive(Debug)]
pub enum LlmError {
    File(FileProblem),
    /// This build does not include the llama.cpp backend.
    NotBuiltIn,
    Backend(String),
    Load(String),
    Context(String),
    Decode(String),
    PromptTooLong {
        tokens: usize,
        n_ctx: u32,
    },
    /// A panic inside the runtime, caught.
    Panic(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::File(p) => write!(f, "{p}"),
            LlmError::NotBuiltIn => write!(f, "this build of AAD does not include the language model runtime"),
            LlmError::Backend(e) => write!(f, "the language model runtime did not start: {e}"),
            LlmError::Load(e) => write!(f, "the model could not be loaded: {e}"),
            LlmError::Context(e) => write!(f, "the model could not make room to think: {e}"),
            LlmError::Decode(e) => write!(f, "the model failed while reading the request: {e}"),
            LlmError::PromptTooLong { tokens, n_ctx } => write!(f, "the request is too long for the model ({tokens} tokens, room for {n_ctx})"),
            LlmError::Panic(e) => write!(f, "the language model runtime stopped unexpectedly: {e}"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<FileProblem> for LlmError {
    fn from(p: FileProblem) -> Self {
        LlmError::File(p)
    }
}

/// How the model is loaded.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Map the file instead of reading it: starts faster and shares memory with the disk cache.
    pub use_mmap: bool,
    /// Layers to run on a GPU (0: all on the CPU; the build has no GPU backend yet).
    pub n_gpu_layers: u32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions { use_mmap: true, n_gpu_layers: 0 }
    }
}

/// Generation settings. The defaults are Qwen3's non-thinking profile with a low temperature for
/// tool routing: starting values to benchmark, not tuned results.
#[derive(Clone, Debug)]
pub struct Params {
    pub n_ctx: u32,
    pub max_new_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub min_p: f32,
    pub seed: u32,
    pub threads: Option<i32>,
    pub enable_thinking: bool,
    /// Generation ends as soon as the answer contains one of these (the match is kept).
    pub stop: Vec<String>,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            n_ctx: 8192,
            max_new_tokens: 512,
            temperature: 0.2,
            top_p: 0.8,
            top_k: 20,
            min_p: 0.0,
            seed: 1234,
            threads: None,
            enable_thinking: false,
            stop: vec!["</tool_call>".to_string()],
        }
    }
}

/// Why generation ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The model finished its turn.
    EndOfTurn,
    /// A stop string was produced.
    StopString,
    MaxTokens,
    /// The context window is full.
    ContextFull,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Completion {
    pub text: String,
    pub prompt_tokens: usize,
    pub new_tokens: usize,
    pub stop: Stop,
}

/// Whether this build includes the llama.cpp backend.
pub const fn available() -> bool {
    cfg!(feature = "llama")
}

/// A loaded model.
pub struct Model {
    #[cfg(feature = "llama")]
    pub(crate) inner: crate::llama::Inner,
    path: PathBuf,
    size_bytes: u64,
}

impl Model {
    /// Load the GGUF file at `path`.
    pub fn load(path: &Path, opts: &LoadOptions) -> Result<Model, LlmError> {
        let size_bytes = files::check(path)?;
        #[cfg(feature = "llama")]
        {
            let inner = crate::llama::load(path, opts)?;
            Ok(Model { inner, path: path.to_path_buf(), size_bytes })
        }
        #[cfg(not(feature = "llama"))]
        {
            let _ = (opts, size_bytes);
            Err(LlmError::NotBuiltIn)
        }
    }

    /// Load the model from the first place it is found ([`files::find_model`]).
    pub fn load_default(opts: &LoadOptions) -> Result<Model, LlmError> {
        match files::find_model() {
            Some(p) => Self::load(&p, opts),
            None => {
                Err(LlmError::File(FileProblem::Missing(files::candidates().into_iter().next().unwrap_or_else(|| PathBuf::from(files::MODEL_FILE)))))
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// The context length the model was trained for.
    pub fn n_ctx_train(&self) -> u32 {
        #[cfg(feature = "llama")]
        {
            self.inner.n_ctx_train()
        }
        #[cfg(not(feature = "llama"))]
        {
            0
        }
    }

    /// Answer a conversation. `on_piece` is called with each new piece of text; `cancel` is checked
    /// between tokens.
    pub fn chat(&self, messages: &[Message], params: &Params, cancel: &AtomicBool, on_piece: &mut dyn FnMut(&str)) -> Result<Completion, LlmError> {
        let prompt = build_prompt(messages, params.enable_thinking);
        self.complete(&prompt, params, cancel, on_piece)
    }

    /// Continue `prompt` (already in the model's format).
    pub fn complete(&self, prompt: &str, params: &Params, cancel: &AtomicBool, on_piece: &mut dyn FnMut(&str)) -> Result<Completion, LlmError> {
        #[cfg(feature = "llama")]
        {
            crate::llama::complete(&self.inner, prompt, params, cancel, on_piece)
        }
        #[cfg(not(feature = "llama"))]
        {
            let _ = (prompt, params, cancel, on_piece);
            Err(LlmError::NotBuiltIn)
        }
    }
}
