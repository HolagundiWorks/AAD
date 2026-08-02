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

/// The Object DB (objects + audit) must persist into a `.vdb` on SAVE and load
/// back in a *fresh* process — `OBJECTS LIST` reads from the file without
/// re-promoting, so seeing objects proves persistence, not re-detection.
#[test]
fn vdb_round_trips_the_object_db() {
    let root = workspace_root();
    let dir = std::env::temp_dir();
    let vdb = dir.join("aadt-vdb-objtest.vdb");
    let save = dir.join("aadt-vdb-save.txt");
    let open = dir.join("aadt-vdb-open.txt");
    let _ = std::fs::remove_file(&vdb);
    let vdb_s = vdb.to_string_lossy().replace('\\', "/");

    std::fs::write(
        &save,
        format!(
            "RECTANGLE 0 0 4000 150 A-WALL\n\
             RECTANGLE 0 0 150 3000 A-WALL\n\
             RECTANGLE 3850 0 4000 3000 A-WALL\n\
             RECTANGLE 0 2850 4000 3000 A-WALL\n\
             OBJECTS\n\
             SAVE {vdb_s}\n"
        ),
    )
    .unwrap();
    std::fs::write(&open, format!("OPEN {vdb_s}\nOBJECTS LIST\n")).unwrap();

    let run = |script: &std::path::Path| -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_aadt"))
            .arg("run")
            .arg(script)
            .current_dir(&root)
            .output()
            .expect("run aadt");
        assert!(out.status.success(), "aadt run failed");
        String::from_utf8(out.stdout).expect("utf8")
    };

    let saved = run(&save);
    assert!(
        saved.contains("promoted 4 object"),
        "promote/save unexpected: {saved}"
    );

    let listed = run(&open);
    assert!(
        !listed.contains("no objects yet") && listed.matches("Room").count() >= 4,
        "Object DB was not carried in the .vdb: {listed}"
    );

    let _ = std::fs::remove_file(&vdb);
    let _ = std::fs::remove_file(&save);
    let _ = std::fs::remove_file(&open);
}
