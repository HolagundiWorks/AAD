//! Site photo / report ingest helpers (M14.5).
//!
//! Photos and binary PDFs become **text chunks** in the corpus (captions,
//! extracted PDF strings, path metadata). Geometry `VectorDb` is untouched —
//! retrieval stays `ASKDOCS` / `FIND`.

use std::path::Path;

/// Build an indexable text blob for a site photo (no OCR — caption + metadata).
pub fn photo_text(path: &str, caption: Option<&str>) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    let mut parts = vec![format!("Site photo: {name}"), format!("Path: {path}")];
    if let Some(c) = caption.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("Caption: {c}"));
    } else {
        parts.push("Caption: (none — supply --caption for searchable description)".into());
    }
    parts.join("\n\n")
}

/// Extract readable text from a PDF byte stream (best-effort literal strings).
///
/// Scans for PDF string literals `(…)` outside of stream binary noise filters.
/// Enough for title-block / report PDFs that embed plain text; not a full
/// content-stream interpreter. Returns empty when nothing useful is found.
pub fn extract_pdf_text(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            i += 1;
            let mut s = String::new();
            let mut escaped = false;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if escaped {
                    match b {
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        other => s.push(other as char),
                    }
                    escaped = false;
                    continue;
                }
                if b == b'\\' {
                    escaped = true;
                    continue;
                }
                if b == b')' {
                    break;
                }
                if b.is_ascii_graphic() || b.is_ascii_whitespace() {
                    s.push(b as char);
                }
            }
            let trimmed = s.trim();
            if trimmed.len() >= 3 && trimmed.chars().any(|c| c.is_alphabetic()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(trimmed);
            }
            continue;
        }
        i += 1;
    }
    out
}

/// Indexable text for a site report / PDF path.
pub fn report_text(path: &str, bytes: &[u8]) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    let extracted = if path.to_ascii_lowercase().ends_with(".pdf") {
        extract_pdf_text(bytes)
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        String::new()
    };
    if extracted.trim().is_empty() {
        format!("Site report: {name}\nPath: {path}\n(no extractable text)")
    } else {
        format!("Site report: {name}\nPath: {path}\n\n{extracted}")
    }
}

/// Route a path to a corpus source label.
pub fn source_label(kind: &str, path: &str) -> String {
    format!("{kind}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_includes_caption() {
        let t = photo_text("site/wall.jpg", Some("north elevation crack"));
        assert!(t.contains("wall.jpg"));
        assert!(t.contains("north elevation crack"));
    }

    #[test]
    fn pdf_extracts_literal_strings() {
        let pdf = b"%PDF-1.4\nBT (Hello Site Report) Tj ET\n";
        let t = extract_pdf_text(pdf);
        assert!(t.contains("Hello Site Report"));
    }
}
