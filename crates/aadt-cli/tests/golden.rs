//! End-to-end test that the `aadt` binary drives the command interpreter.
//!
//! Runs `aadt run fixtures/demo.aadt` and asserts the observable milestones in
//! stdout: OPEN, transactional LINE, UNDO, LIST — with no GUI (spec §8
//! reproducibility).

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/aadt-cli; the workspace root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root")
}

#[test]
fn run_script_drives_the_interpreter() {
    // `aadt run` executes a command script: OPEN, transactional LINE, UNDO, LIST.
    let root = workspace_root();
    let script = root.join("fixtures/demo.aadt");

    let output = Command::new(env!("CARGO_BIN_EXE_aadt"))
        .arg("run")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("run aadt binary");
    assert!(output.status.success(), "aadt run exited with failure");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(stdout.contains("opened"), "OPEN did not report: {stdout}");
    assert!(
        stdout.contains("undo: LINE"),
        "UNDO did not report: {stdout}"
    );
}
