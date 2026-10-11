//! llama.cpp through the `llama-cpp-2` crate (built only with the `llama` feature).
//!
//! One backend lives for the whole process. Every call into the native library runs behind
//! `catch_unwind`, so a panic anywhere becomes an [`LlmError`].

use std::num::NonZeroU32;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;

use crate::engine::{Completion, LlmError, LoadOptions, Params, Stop};

pub(crate) struct Inner {
    model: LlamaModel,
}

impl Inner {
    pub(crate) fn n_ctx_train(&self) -> u32 {
        self.model.n_ctx_train()
    }
}

fn backend() -> Result<&'static LlamaBackend, LlmError> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| {
            let mut b = LlamaBackend::init().map_err(|e| e.to_string())?;
            // llama.cpp prints a lot while loading; the app has its own log.
            b.void_logs();
            Ok(b)
        })
        .as_ref()
        .map_err(|e| LlmError::Backend(e.clone()))
}

fn contained<T>(f: impl FnOnce() -> Result<T, LlmError>) -> Result<T, LlmError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => Err(LlmError::Panic(
            p.downcast_ref::<&str>().map(|s| (*s).to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into()),
        )),
    }
}

pub(crate) fn load(path: &Path, opts: &LoadOptions) -> Result<Inner, LlmError> {
    let be = backend()?;
    contained(|| {
        let params = LlamaModelParams::default().with_n_gpu_layers(opts.n_gpu_layers).with_use_mmap(opts.use_mmap);
        let model = LlamaModel::load_from_file(be, path, &params).map_err(|e| LlmError::Load(e.to_string()))?;
        Ok(Inner { model })
    })
}

pub(crate) fn complete(
    m: &Inner,
    prompt: &str,
    params: &Params,
    cancel: &AtomicBool,
    on_piece: &mut dyn FnMut(&str),
) -> Result<Completion, LlmError> {
    let be = backend()?;
    contained(|| run(m, be, prompt, params, cancel, on_piece))
}

fn run(
    m: &Inner,
    be: &LlamaBackend,
    prompt: &str,
    params: &Params,
    cancel: &AtomicBool,
    on_piece: &mut dyn FnMut(&str),
) -> Result<Completion, LlmError> {
    let n_ctx = params.n_ctx.clamp(256, m.model.n_ctx_train().max(256));
    let threads = params.threads.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| i32::try_from(n.get().min(8)).unwrap_or(4)));
    let n_batch: u32 = 512;
    let cparams =
        LlamaContextParams::default().with_n_ctx(NonZeroU32::new(n_ctx)).with_n_batch(n_batch).with_n_threads(threads).with_n_threads_batch(threads);
    let mut ctx = m.model.new_context(be, cparams).map_err(|e| LlmError::Context(e.to_string()))?;
    let vocab = m.model.vocab();
    // Special tokens such as <|im_start|> are written in the prompt text, so they are parsed.
    let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
    if tokens.is_empty() || tokens.len() + 1 >= n_ctx as usize {
        return Err(LlmError::PromptTooLong { tokens: tokens.len(), n_ctx });
    }

    // Read the prompt in chunks; only its last token needs logits.
    let mut batch = LlamaBatch::new(n_batch as usize, 1);
    let last = tokens.len() - 1;
    for (chunk_start, chunk) in tokens.chunks(n_batch as usize).enumerate().map(|(i, c)| (i * n_batch as usize, c)) {
        batch.clear();
        for (j, t) in chunk.iter().enumerate() {
            let pos = chunk_start + j;
            batch.add(*t, i32::try_from(pos).unwrap_or(i32::MAX), &[0], pos == last).map_err(|e| LlmError::Decode(e.to_string()))?;
        }
        ctx.decode(&mut batch).map_err(|e| LlmError::Decode(e.to_string()))?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(Completion { text: String::new(), prompt_tokens: tokens.len(), new_tokens: 0, stop: Stop::Cancelled });
        }
    }

    let mut sampler = if params.temperature <= 0.0 {
        LlamaSampler::greedy()
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::top_k(params.top_k.max(1)),
            LlamaSampler::top_p(params.top_p.clamp(0.01, 1.0), 1),
            LlamaSampler::min_p(params.min_p.clamp(0.0, 1.0), 1),
            LlamaSampler::temp(params.temperature),
            LlamaSampler::dist(params.seed),
        ])
    };

    let mut text = String::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut pos = tokens.len();
    let mut new_tokens = 0usize;
    let stop = loop {
        if cancel.load(Ordering::Relaxed) {
            break Stop::Cancelled;
        }
        if new_tokens >= params.max_new_tokens as usize {
            break Stop::MaxTokens;
        }
        if pos + 1 >= n_ctx as usize {
            break Stop::ContextFull;
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if vocab.is_eog(token) {
            break Stop::EndOfTurn;
        }
        new_tokens += 1;
        // A piece can end in the middle of a character: keep the bytes until they are whole.
        pending.extend_from_slice(&vocab.token_to_piece(token, false, None));
        let valid = match std::str::from_utf8(&pending) {
            Ok(s) => s.len(),
            Err(e) => e.valid_up_to(),
        };
        if valid > 0 {
            let piece = String::from_utf8_lossy(&pending[..valid]).to_string();
            pending.drain(..valid);
            on_piece(&piece);
            text.push_str(&piece);
            if let Some(end) = params.stop.iter().filter_map(|s| text.find(s.as_str()).map(|i| i + s.len())).min() {
                text.truncate(end);
                break Stop::StopString;
            }
        }
        batch.clear();
        batch.add(token, i32::try_from(pos).unwrap_or(i32::MAX), &[0], true).map_err(|e| LlmError::Decode(e.to_string()))?;
        pos += 1;
        ctx.decode(&mut batch).map_err(|e| LlmError::Decode(e.to_string()))?;
    };
    Ok(Completion { text, prompt_tokens: tokens.len(), new_tokens, stop })
}
