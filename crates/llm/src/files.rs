//! Finding and checking the model file. The weights are not part of the source tree: they are a
//! build input of the installer (and of a developer's machine), so nothing here assumes they exist.

use std::path::{Path, PathBuf};

/// The file the installer ships and the app looks for first.
pub const MODEL_FILE: &str = "Qwen3-1.7B-Q4_K_M.gguf";
/// Environment variable naming a model file (overrides every other place).
pub const MODEL_ENV: &str = "AAD_MODEL";

/// What is wrong with a model file, in words fit for the status bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileProblem {
    Missing(PathBuf),
    NotAFile(PathBuf),
    TooSmall { path: PathBuf, bytes: u64 },
    NotGguf(PathBuf),
    Unreadable(PathBuf, String),
}

impl std::fmt::Display for FileProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileProblem::Missing(p) => write!(f, "the model file {} does not exist", p.display()),
            FileProblem::NotAFile(p) => write!(f, "{} is not a file", p.display()),
            FileProblem::TooSmall { path, bytes } => write!(f, "{} is only {bytes} bytes: the download is incomplete", path.display()),
            FileProblem::NotGguf(p) => write!(f, "{} is not a GGUF model file", p.display()),
            FileProblem::Unreadable(p, e) => write!(f, "{}: {e}", p.display()),
        }
    }
}

impl std::error::Error for FileProblem {}

/// The AAD settings folder (`%APPDATA%\AAD` on Windows).
pub fn settings_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    })?;
    Some(base.join("AAD"))
}

/// Where the model is looked for, best first: `AAD_MODEL`, the settings folder's `models`, the
/// `models` folder beside the program (where the installer puts it), then `models` in the working
/// folder.
pub fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os(MODEL_ENV).filter(|p| !p.is_empty()) {
        v.push(PathBuf::from(p));
    }
    if let Some(d) = settings_dir() {
        v.push(d.join("models").join(MODEL_FILE));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        v.push(d.join("models").join(MODEL_FILE));
    }
    v.push(PathBuf::from("models").join(MODEL_FILE));
    v
}

/// The first candidate that is a file.
pub fn find_model() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

/// A model file smaller than this cannot be Qwen3-1.7B at any quantisation (the 4-bit file is
/// about 1.1 GB): it is a failed or partial download.
const MIN_BYTES: u64 = 100 * 1024 * 1024;

/// Check a file before handing it to the runtime: it exists, is large enough, and starts with the
/// GGUF magic. This is what turns a half-finished download into a clear message instead of a crash
/// inside a native library.
pub fn check(path: &Path) -> Result<u64, FileProblem> {
    let meta = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            FileProblem::Missing(path.to_path_buf())
        } else {
            FileProblem::Unreadable(path.to_path_buf(), e.to_string())
        }
    })?;
    if !meta.is_file() {
        return Err(FileProblem::NotAFile(path.to_path_buf()));
    }
    if meta.len() < MIN_BYTES {
        // A small file may still be a valid GGUF of another model; say which problem it is.
        return match magic(path) {
            Ok(true) => Err(FileProblem::TooSmall { path: path.to_path_buf(), bytes: meta.len() }),
            Ok(false) => Err(FileProblem::NotGguf(path.to_path_buf())),
            Err(e) => Err(FileProblem::Unreadable(path.to_path_buf(), e.to_string())),
        };
    }
    match magic(path) {
        Ok(true) => Ok(meta.len()),
        Ok(false) => Err(FileProblem::NotGguf(path.to_path_buf())),
        Err(e) => Err(FileProblem::Unreadable(path.to_path_buf(), e.to_string())),
    }
}

fn magic(path: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let mut head = [0u8; 4];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    Ok(n == 4 && &head == b"GGUF")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aad-llm-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn candidates_end_with_the_working_folder_and_include_the_settings_folder() {
        let c = candidates();
        assert_eq!(c.last().unwrap(), &PathBuf::from("models").join(MODEL_FILE));
        assert!(c.iter().any(|p| p.ends_with(MODEL_FILE)));
    }

    #[test]
    fn a_missing_or_wrong_file_is_named_not_loaded() {
        let d = temp("check");
        assert!(matches!(check(&d.join("nope.gguf")), Err(FileProblem::Missing(_))));
        assert!(matches!(check(&d), Err(FileProblem::NotAFile(_))));
        let text = d.join("notes.gguf");
        std::fs::write(&text, b"hello world").unwrap();
        assert!(matches!(check(&text), Err(FileProblem::NotGguf(_))));
        let partial = d.join("part.gguf");
        std::fs::write(&partial, b"GGUF\x03\x00\x00\x00 partial").unwrap();
        let e = check(&partial).unwrap_err();
        assert!(matches!(e, FileProblem::TooSmall { .. }), "{e}");
        assert!(e.to_string().contains("incomplete"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_large_file_with_the_gguf_magic_passes() {
        let d = temp("big");
        let p = d.join("m.gguf");
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(MIN_BYTES + 1).unwrap();
        drop(f);
        // Sparse zeros have no magic yet.
        assert!(matches!(check(&p), Err(FileProblem::NotGguf(_))));
        std::fs::write(&p, b"GGUF").unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(MIN_BYTES + 1).unwrap();
        drop(f);
        assert_eq!(check(&p), Ok(MIN_BYTES + 1));
        std::fs::remove_dir_all(&d).ok();
    }
}
