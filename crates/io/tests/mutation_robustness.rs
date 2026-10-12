//! Mutation robustness of the file readers: valid drawings are corrupted in many ways (truncated,
//! bytes flipped, lines deleted, duplicated or swapped, numbers replaced by hostile values,
//! sections cut) and every corrupted file must be handled without a panic or a hang, whatever the
//! reader makes of it: reading, rendering, extents and writing back must all stay safe.
//!
//! It is deterministic: a seed and an iteration count reproduce the same files. The default is
//! small enough for every pull request; a scheduled job raises it with environment variables:
//!
//! - `AAD_MUTATION_ITERS`: mutants per seed file (default 300)
//! - `AAD_MUTATION_SEED`: first random seed (default 1)
//!
//! A failing file is saved to `target/quality/crashes/` (never next to the fixtures) and the
//! failure message gives the seed, the operation and that path, so it can be replayed and turned
//! into a permanent regression test.
//!
//! This is not coverage-guided fuzzing: it explores around known-good files, which is where real
//! files go wrong. See docs/TESTING.md for the `cargo-fuzz` plan.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use aad_doc::Space;

/// xorshift64*: small, deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

const HOSTILE: &[&str] = &[
    "NaN",
    "inf",
    "-inf",
    "1e308",
    "-1e308",
    "1e-308",
    "0",
    "-1",
    "99999999999",
    "4294967296",
    "-2147483649",
    "",
    "abc",
    "0x10",
    "1.5.2",
    "  ",
    "#",
    "99999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
];

/// One corruption of a DXF (text) file; returns what was done.
fn mutate_text(text: &str, rng: &mut Rng) -> (String, String) {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if lines.is_empty() {
        return (String::new(), "empty".into());
    }
    match rng.below(8) {
        0 => {
            let cut = rng.below(text.len().max(1));
            let mut end = cut;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            (text[..end].to_string(), format!("truncate at byte {end}"))
        }
        1 => {
            let i = rng.below(lines.len());
            lines.remove(i);
            (lines.join("\n"), format!("delete line {i}"))
        }
        2 => {
            let i = rng.below(lines.len());
            let l = lines[i].clone();
            lines.insert(i, l);
            (lines.join("\n"), format!("duplicate line {i}"))
        }
        3 => {
            let i = rng.below(lines.len().saturating_sub(1).max(1));
            if i + 1 < lines.len() {
                lines.swap(i, i + 1);
            }
            (lines.join("\n"), format!("swap lines {i},{}", i + 1))
        }
        4 | 5 => {
            // Replace a value line (odd positions are values) with a hostile token.
            let i = (rng.below(lines.len() / 2) * 2 + 1).min(lines.len() - 1);
            let h = HOSTILE[rng.below(HOSTILE.len())];
            let old = std::mem::replace(&mut lines[i], h.to_string());
            (lines.join("\n"), format!("line {i}: `{old}` -> `{h}`"))
        }
        6 => {
            let a = rng.below(lines.len());
            let b = (a + 1 + rng.below(40)).min(lines.len());
            lines.drain(a..b);
            (lines.join("\n"), format!("cut lines {a}..{b}"))
        }
        _ => {
            let i = rng.below(lines.len());
            let inserts = [
                "0\nINSERT\n2\nX\n10\n1\n20\n1\n",
                "0\nHATCH\n91\n3\n",
                "0\nENDSEC\n",
                "0\nLWPOLYLINE\n90\n99999\n",
                "0\nBLOCK\n2\nX\n",
                "0\nSEQEND\n",
            ];
            let ins = inserts[rng.below(inserts.len())];
            lines.insert(i, ins.trim_end().to_string());
            (lines.join("\n"), format!("insert group before line {i}"))
        }
    }
}

fn crash_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/quality/crashes");
    std::fs::create_dir_all(&d).ok();
    d
}

/// Everything a reader's output is put through: it must all be safe for any drawing the reader
/// accepts.
fn use_drawing(bytes: Vec<u8>, name: String) -> Result<(), String> {
    match aad_io::read(&bytes, &name) {
        Ok(d) => {
            let _ = d.extents(&Space::Model);
            let _ = aad_render::build(&d, &Space::Model, &aad_render::Options::default());
            for l in &d.layouts {
                let _ = aad_render::build(&d, &Space::Paper(l.name.clone()), &aad_render::Options::default());
            }
            let _ = aad_io::write(&d, "again.dxf");
            let _ = aad_io::write(&d, "again.svg");
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

/// Run one mutant on its own thread so a hang is a failure, not a stuck test run.
fn check(bytes: &[u8], name: &str) -> Result<(), String> {
    // An allocation failure aborts the process and cannot be caught, so the file being read is on
    // disk first: after an abort, `last-mutant.*` is the input that caused it.
    let ext = name.rsplit('.').next().unwrap_or("bin");
    std::fs::write(crash_dir().join(format!("last-mutant.{ext}")), bytes).ok();
    let (tx, rx) = mpsc::channel();
    let (b, n) = (bytes.to_vec(), name.to_string());
    std::thread::Builder::new()
        .name("mutant".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let r = catch_unwind(AssertUnwindSafe(|| use_drawing(b, n)));
            let _ = tx.send(match r {
                Ok(r) => r,
                Err(p) => Err(format!(
                    "panic: {}",
                    p.downcast_ref::<&str>().map(|s| (*s).to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default()
                )),
            });
        })
        .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(r) => r,
        Err(_) => Err("no result within 20 s (hang or runaway allocation)".into()),
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn run_text_seed(label: &str, seed_text: &str) {
    let iters = env_u64("AAD_MUTATION_ITERS", 300);
    let first = env_u64("AAD_MUTATION_SEED", 1);
    let mut failures = Vec::new();
    for s in first..first + iters {
        let mut rng = Rng::new(s);
        // Mutate once or twice: real damage is rarely a single edit.
        let (mut t, mut what) = mutate_text(seed_text, &mut rng);
        if rng.below(3) == 0 {
            let (t2, w2) = mutate_text(&t, &mut rng);
            t = t2;
            what = format!("{what}; {w2}");
        }
        if let Err(e) = check(t.as_bytes(), "mutant.dxf") {
            let path = crash_dir().join(format!("{label}-seed{s}.dxf"));
            std::fs::write(&path, t.as_bytes()).ok();
            failures.push(format!("{label} seed {s} ({what}): {e}; saved {}", path.display()));
        }
    }
    assert!(failures.is_empty(), "{} of {iters} mutants failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn the_example_drawing_survives_text_corruption() {
    let text = include_str!("../../../examples/apartment.dxf");
    // The unmodified file is valid: this is what the mutants are corrupted from.
    assert!(aad_io::read(text.as_bytes(), "apartment.dxf").is_ok());
    run_text_seed("apartment", text);
}

#[test]
fn a_small_hostile_structure_survives_text_corruption() {
    // Blocks that nest, a hatch, a spline, an insert with attributes, a polyline with bulges.
    let text = "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nA\n10\n0\n20\n0\n0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDBLK\n0\nBLOCK\n2\nB\n0\nINSERT\n2\nA\n10\n1\n20\n1\n0\nENDBLK\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nINSERT\n2\nB\n10\n5\n20\n5\n41\n2\n42\n2\n50\n30\n70\n2\n71\n2\n44\n3\n45\n3\n0\nLWPOLYLINE\n90\n3\n70\n1\n10\n0\n20\n0\n42\n0.5\n10\n4\n20\n0\n10\n4\n20\n4\n0\nSPLINE\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n10\n0\n20\n0\n10\n1\n20\n2\n10\n2\n20\n2\n10\n3\n20\n0\n0\nCIRCLE\n10\n1\n20\n1\n40\n2\n0\nARC\n10\n0\n20\n0\n40\n3\n50\n0\n51\n90\n0\nTEXT\n10\n0\n20\n0\n40\n1\n1\nHello\n0\nENDSEC\n0\nEOF\n";
    assert!(aad_io::read(text.as_bytes(), "small.dxf").is_ok());
    run_text_seed("small", text);
}

// DWG is not mutated here: the DWG reader (a third-party library) aborts the whole process on some
// damaged files, which no in-process test can survive. DWG corruption is tested through the worker
// process in `apps/aad-cli/tests/dwg_isolation.rs`.
