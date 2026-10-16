//! DWG is read by this process, not by the WinUI window. A damaged file can abort the reader;
//! that must not take the open drawings with it. The host starts this program with
//! `--aad-dwg-worker` and passes the file on standard input.

fn main() {
    aad_dwg::isolated::run_worker_if_requested();
}
