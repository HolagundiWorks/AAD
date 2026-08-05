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

/// Read the banner `aadt serve` prints and return the port it bound.
///
/// The test asks for port 0 so the OS picks a free one — a hard-coded port
/// makes the test fail whenever a developer happens to be running the server.
fn serve_port(stdout: &mut impl std::io::BufRead) -> u16 {
    for _ in 0..20 {
        let mut line = String::new();
        if stdout.read_line(&mut line).expect("read serve banner") == 0 {
            break;
        }
        if let Some(rest) = line.split("http://127.0.0.1:").nth(1) {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(port) = digits.parse::<u16>() {
                return port;
            }
        }
    }
    panic!("`aadt serve` never announced a port");
}

/// Send one request and return the whole response, headers included.
fn http(port: u16, request: &str) -> String {
    use std::io::{Read, Write};
    let mut sock = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to the API");
    sock.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    sock.write_all(request.as_bytes()).expect("send request");
    sock.flush().unwrap();
    let mut response = String::new();
    sock.read_to_string(&mut response).expect("read response");
    response
}

fn post(port: u16, path: &str, body: &str) -> String {
    http(
        port,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer golden\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
}

/// `aadt serve` must expose the CLI's *whole* command set, not a second thinner
/// one: the API is only "everything the command line can do, scriptable" if a
/// command the CLI registers itself (`OBJECTS`) is reachable through it.
#[test]
fn serve_exposes_the_cli_command_set_over_http() {
    use std::io::BufReader;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_aadt"))
        .args(["serve", "--port", "0", "--token", "golden"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aadt serve");
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let port = serve_port(&mut stdout);

    let drawn = post(
        port,
        "/v1/command",
        r#"{"line":"RECTANGLE 0 0 4000 3000 A-WALL"}"#,
    );
    assert!(drawn.starts_with("HTTP/1.1 200 OK"), "{drawn}");
    assert!(drawn.contains(r#""changed":true"#), "{drawn}");

    // OBJECTS lives in the CLI's registration, not in the interpreter's
    // built-ins — reaching it proves the server drives the real session.
    let objects = post(port, "/v1/command", r#"{"line":"OBJECTS"}"#);
    assert!(
        objects.contains(r#""status":"ok""#),
        "OBJECTS not reachable through the API: {objects}"
    );

    let listed = http(
        port,
        "GET /v1/entities HTTP/1.1\r\nAuthorization: Bearer golden\r\n\r\n",
    );
    assert!(listed.contains(r#""kind":"polyline""#), "{listed}");

    assert!(post(port, "/v1/shutdown", "").contains(r#""status":"stopping""#));
    let status = child.wait().expect("aadt serve exits after shutdown");
    assert!(status.success(), "serve exited with {status}");
}

/// `--once` answers a single request and exits — the shape a script uses when
/// it wants one answer without owning a server's lifetime.
#[test]
fn serve_once_answers_one_request_and_exits() {
    use std::io::BufReader;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_aadt"))
        .args(["serve", "--port", "0", "--token", "golden", "--once"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aadt serve --once");
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let port = serve_port(&mut stdout);

    let health = http(
        port,
        "GET /v1/health HTTP/1.1\r\nAuthorization: Bearer golden\r\n\r\n",
    );
    assert!(health.contains(r#""status":"ok""#), "{health}");
    assert!(child.wait().expect("wait").success());
}
