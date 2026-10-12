//! Load the model and answer one prompt, printing timings: a manual check that the model file, the
//! runtime and the prompt format work together on this machine. Not a test (it needs a 1.1 GB file).
//!
//! `cargo run --release -p aad-llm --features llama --example load_and_ask -- "your question"`
//! The model is found like the application finds it (`AAD_MODEL`, `%APPDATA%\AAD\models`, ...).

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use aad_llm::{LoadOptions, Message, Model, Params};

fn main() {
    let question = std::env::args().nth(1).unwrap_or_else(|| "In one sentence, what is a polyline in a CAD drawing?".into());
    println!("model runtime built in: {}", aad_llm::available());
    let t = Instant::now();
    let model = match Model::load_default(&LoadOptions::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("could not load: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "loaded {} ({:.0} MB, trained for {} tokens) in {:.1} s",
        model.path().display(),
        model.size_bytes() as f64 / 1e6,
        model.n_ctx_train(),
        t.elapsed().as_secs_f64()
    );
    let params = Params { max_new_tokens: 200, stop: Vec::new(), ..Params::default() };
    let t = Instant::now();
    let mut first: Option<f64> = None;
    let r = model.chat(&[Message::user(question)], &params, &AtomicBool::new(false), &mut |piece| {
        first.get_or_insert(t.elapsed().as_secs_f64());
        print!("{piece}");
    });
    println!();
    match r {
        Ok(c) => {
            let s = t.elapsed().as_secs_f64();
            println!(
                "--- {} prompt tokens, {} new tokens, stop {:?}, first token after {:.2} s, {:.1} tokens/s overall",
                c.prompt_tokens,
                c.new_tokens,
                c.stop,
                first.unwrap_or(0.0),
                c.new_tokens as f64 / s.max(1e-9)
            );
        }
        Err(e) => {
            eprintln!("generation failed: {e}");
            std::process::exit(2);
        }
    }
}
