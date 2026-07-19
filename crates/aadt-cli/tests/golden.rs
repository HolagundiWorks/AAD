//! End-to-end golden test for the M0 vertical slice.
//!
//! Runs the actual `aadt` binary on `fixtures/sample-floorplan.dxf` and asserts the
//! emitted CSV byte-matches `fixtures/expected-takeoff.csv`. This is the executable
//! proof of the milestone: DXF import → detection → deterministic takeoff → CSV,
//! with no GUI and no AI (spec §8 reproducibility).

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
fn takeoff_matches_golden_csv() {
    let root = workspace_root();
    let fixture = root.join("fixtures/sample-floorplan.dxf");
    let expected = std::fs::read_to_string(root.join("fixtures/expected-takeoff.csv"))
        .expect("read expected CSV");

    let out_path = std::env::temp_dir().join("aadt-golden-out.csv");
    let _ = std::fs::remove_file(&out_path);

    let status = Command::new(env!("CARGO_BIN_EXE_aadt"))
        .arg("takeoff")
        .arg(&fixture)
        .arg("--csv")
        .arg(&out_path)
        .status()
        .expect("run aadt binary");
    assert!(status.success(), "aadt exited with failure");

    let actual = std::fs::read_to_string(&out_path).expect("read produced CSV");
    assert_eq!(actual, expected, "takeoff CSV drifted from the golden file");
}

#[test]
fn takeoff_is_reproducible() {
    let root = workspace_root();
    let fixture = root.join("fixtures/sample-floorplan.dxf");

    let run = || {
        let out = std::env::temp_dir().join(format!("aadt-repro-{}.csv", std::process::id()));
        Command::new(env!("CARGO_BIN_EXE_aadt"))
            .arg("takeoff")
            .arg(&fixture)
            .arg("--csv")
            .arg(&out)
            .status()
            .expect("run aadt binary");
        std::fs::read_to_string(&out).expect("read CSV")
    };

    assert_eq!(run(), run(), "same input produced different output");
}
