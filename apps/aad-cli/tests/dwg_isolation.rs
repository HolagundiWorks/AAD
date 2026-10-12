//! DWG files are read in a worker process, so a file that crashes the DWG reader is an error
//! message and not a crashed program. Regression tests for BUG-001 (docs/BUG_BACKLOG.md): a damaged
//! DWG made the reader ask for ~3.4 petabytes, which aborts the process and cannot be caught.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

const EXE: &str = env!("CARGO_BIN_EXE_aad-cli");

fn fixture() -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dwg/alloc-bomb-ac18-section-size.dwg")).expect("the fixture exists")
}

/// This test process reads DWG through the CLI binary as the worker (the test runner itself is not
/// one), exactly as the applications read it through themselves.
fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(aad_dwg::isolated::enable_with(PathBuf::from(EXE)));
    });
}

#[test]
fn the_file_that_aborted_the_reader_is_now_an_error_and_the_process_lives() {
    isolate();
    let bytes = fixture();
    assert!(aad_dwg::is_dwg(&bytes));
    // Before the fix this call killed the test process (exit status 0xc0000409 after
    // "memory allocation of 3377699721332651 bytes failed").
    let e = aad_io::read(&bytes, "bomb.dwg").map(|_| ()).unwrap_err();
    assert!(e.to_string().contains("stopped on this file"), "{e}");
    // And the program can go on to read a good drawing afterwards.
    let d = aad_io::read(include_bytes!("../../../examples/apartment.dxf"), "apartment.dxf").unwrap();
    assert!(d.entity_count() > 50);
}

#[test]
fn a_good_dwg_still_reads_through_the_worker() {
    isolate();
    let d = aad_io::read(include_bytes!("../../../examples/apartment.dxf"), "apartment.dxf").unwrap();
    let dwg = aad_io::write(&d, "apartment.dwg").unwrap();
    let back = aad_io::read(&dwg, "apartment.dwg").unwrap();
    assert_eq!(back.entity_count(), d.entity_count(), "the same drawing came back through the worker");
    assert_eq!(back.layers.len(), d.layers.len());
}

#[test]
fn the_command_line_tool_reports_the_bad_file_and_exits_normally() {
    // Run the real program on the file, as a user would.
    let dir = std::env::temp_dir().join(format!("aad-dwg-iso-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("bomb.dwg");
    std::fs::write(&copy, fixture()).unwrap();
    let out = Command::new(EXE).args(["info", copy.to_str().unwrap()]).output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(code, 1, "an ordinary failure, not an abort (0xc0000409 is {}): {err}", 0xc000_0409u32 as i32);
    assert!(err.contains("DWG"), "{err}");
    assert!(copy.is_file(), "the file was not touched");
    std::fs::remove_dir_all(&dir).ok();
}

/// Corrupted copies of a real DWG, read through the worker: none may take this process down, and
/// the number that the reader could not survive is reported (they are errors, not crashes).
#[test]
fn corrupted_dwg_files_never_take_the_program_down() {
    isolate();
    let d = aad_io::read(include_bytes!("../../../examples/apartment.dxf"), "apartment.dxf").unwrap();
    let dwg = aad_io::write(&d, "apartment.dwg").unwrap();
    let iters: u64 = std::env::var("AAD_MUTATION_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let first: u64 = std::env::var("AAD_MUTATION_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let (mut ok, mut err) = (0, 0);
    for s in first..first + iters {
        let mut x = s.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut next = |n: usize| {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            (x.wrapping_mul(0x2545_f491_4f6c_dd1d) % n.max(1) as u64) as usize
        };
        let mut b = dwg.clone();
        match next(3) {
            0 => b.truncate(next(b.len())),
            1 => {
                for _ in 0..1 + next(8) {
                    let i = next(b.len());
                    b[i] = next(256) as u8;
                }
            }
            _ => {
                let a = next(b.len());
                let len = next(64).min(b.len() - a);
                b.drain(a..a + len);
            }
        }
        match aad_io::read(&b, "mutant.dwg") {
            Ok(_) => ok += 1,
            Err(_) => err += 1,
        }
    }
    eprintln!("{iters} corrupted DWG files: {ok} read, {err} refused");
    assert_eq!(ok + err, iters);
}
