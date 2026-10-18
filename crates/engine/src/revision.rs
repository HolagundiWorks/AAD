//! Drawing revisions: what a saved drawing file was, so other applications (quantities, costing,
//! documents) can say exactly which drawing they used.
//!
//! A save writes `<file>.aadrev.json` beside the drawing: the SHA-256 of the saved bytes, their
//! count, the save time and the number of entities. The sidecar is advisory. If it cannot be
//! written the save still succeeds; if it is missing or stale, [`check`] says so. Nothing is
//! stored in the DXF/DWG itself, so other programs read the file unchanged.

use serde_json::{Value, json};

/// Bytes read or hashed from one file at most; a larger file is not hashed.
const MAX_HASHED: usize = 2 << 30;
const SCHEMA: u32 = 1;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data` (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut block = |chunk: &[u8]| {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate().take(16) {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    };
    let mut chunks = data.chunks_exact(64);
    for chunk in &mut chunks {
        block(chunk);
    }
    // Padding: 0x80, zeros, then the bit length as a big-endian u64.
    let rest = chunks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let used = if rest.len() < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[used - 8..used].copy_from_slice(&bits.to_be_bytes());
    for chunk in tail[..used].chunks_exact(64) {
        block(chunk);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// What a save recorded about a drawing file.
#[derive(Clone, Debug, PartialEq)]
pub struct Revision {
    /// File name without its folder.
    pub file: String,
    pub sha256: String,
    pub bytes: usize,
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    pub entities: usize,
}

impl Revision {
    pub fn to_json(&self) -> Value {
        json!({ "schema": SCHEMA, "file": self.file, "sha256": self.sha256, "bytes": self.bytes, "savedAt": self.saved_at, "entities": self.entities })
    }

    fn from_json(v: &Value) -> Option<Revision> {
        if v.get("schema")?.as_u64()? != u64::from(SCHEMA) {
            return None;
        }
        let sha = v.get("sha256")?.as_str()?;
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Revision {
            file: v.get("file")?.as_str()?.chars().take(260).collect(),
            sha256: sha.to_ascii_lowercase(),
            bytes: usize::try_from(v.get("bytes")?.as_u64()?).ok()?,
            saved_at: v.get("savedAt")?.as_u64()?,
            entities: usize::try_from(v.get("entities")?.as_u64()?).ok()?,
        })
    }
}

/// The sidecar that records the revision of `path`.
pub fn sidecar_path(path: &str) -> String {
    format!("{path}.aadrev.json")
}

#[cfg(not(target_arch = "wasm32"))]
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Record the revision of a drawing just written to `path` from `bytes`. Best effort: returns the
/// revision, or the reason it could not be written.
#[cfg(not(target_arch = "wasm32"))]
pub fn record(path: &str, bytes: &[u8], entities: usize) -> Result<Revision, String> {
    let file = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rev = Revision { file, sha256: sha256_hex(bytes), bytes: bytes.len(), saved_at: now(), entities };
    let sidecar = sidecar_path(path);
    let tmp = format!("{sidecar}.tmp");
    let text = serde_json::to_string_pretty(&rev.to_json()).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("{sidecar}: {e}"))?;
    std::fs::rename(&tmp, &sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    Ok(rev)
}

/// The revision recorded for `path`, if a readable sidecar of the right schema exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: &str) -> Option<Revision> {
    let sidecar = sidecar_path(path);
    // A sidecar is a few hundred bytes; anything large is not one.
    if std::fs::metadata(&sidecar).ok()?.len() > 16 * 1024 {
        return None;
    }
    let text = std::fs::read_to_string(sidecar).ok()?;
    Revision::from_json(&serde_json::from_str(&text).ok()?)
}

/// How the file on disk compares with what was recorded.
#[cfg(not(target_arch = "wasm32"))]
pub fn check(path: &str) -> Result<Value, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() as usize > MAX_HASHED {
        return Err(format!("{path}: too large to hash"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let current = sha256_hex(&bytes);
    let recorded = read(path);
    let status = match &recorded {
        None => "unrecorded",
        Some(r) if r.sha256 == current => "current",
        Some(_) => "changed",
    };
    Ok(json!({
        "path": path,
        "status": status,
        "sha256": current,
        "bytes": bytes.len(),
        "recorded": recorded.as_ref().map(Revision::to_json),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let million_a = vec![b'a'; 1_000_000];
        assert_eq!(sha256_hex(&million_a), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn padding_edges_do_not_panic_and_differ() {
        let mut seen = std::collections::HashSet::new();
        for n in 0..200usize {
            assert!(seen.insert(sha256_hex(&vec![7u8; n])), "length {n}");
        }
    }

    #[test]
    fn a_sidecar_round_trips_and_a_changed_file_is_noticed() {
        let dir = std::env::temp_dir().join(format!("aad-rev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.dxf").to_string_lossy().to_string();
        std::fs::write(&path, b"first").unwrap();
        assert_eq!(check(&path).unwrap()["status"], json!("unrecorded"));
        let rev = record(&path, b"first", 3).unwrap();
        assert_eq!(read(&path), Some(rev.clone()));
        assert_eq!(rev.file, "plan.dxf");
        assert_eq!(check(&path).unwrap()["status"], json!("current"));
        std::fs::write(&path, b"second").unwrap();
        assert_eq!(check(&path).unwrap()["status"], json!("changed"));
        assert!(check(&dir.join("missing.dxf").to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_or_foreign_sidecar_is_ignored() {
        let dir = std::env::temp_dir().join(format!("aad-rev-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.dxf").to_string_lossy().to_string();
        for text in [
            "",
            "not json",
            "{}",
            r#"{"schema":2,"file":"a","sha256":"00","bytes":1,"savedAt":1,"entities":1}"#,
            r#"{"schema":1,"file":"a","sha256":"zz","bytes":1,"savedAt":1,"entities":1}"#,
        ] {
            std::fs::write(sidecar_path(&path), text).unwrap();
            assert_eq!(read(&path), None, "{text}");
        }
        std::fs::write(sidecar_path(&path), vec![b' '; 20_000]).unwrap();
        assert_eq!(read(&path), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
