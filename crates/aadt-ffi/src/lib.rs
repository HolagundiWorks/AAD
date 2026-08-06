//! # aadt-ffi — Thin C ABI over [`aadt_app::Session`]
//!
//! Native chrome (WinUI 3 + Direct2D under `native/aad-winui/`) drives the same
//! command / LISP / undo loop as the CLI without rewriting the engine
//! ([ADR-0017](../../docs/adr/0017-winui-shell-rust-engine.md)). **No UI here.**
//!
//! # Safety
//!
//! All exported functions are `extern "C"`. Callers must:
//! - Treat [`AadtSession`] handles as opaque.
//! - Free owned strings from [`aadt_session_path`] with [`aadt_string_free`].
//! - Not use a handle after [`aadt_session_free`].

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

use aadt_app::{Feedback, Session};
use aadt_cli::register;
use aadt_command::Interpreter;
use aadt_geometry::Point2;
use aadt_render::DrawCommand;

/// Opaque session handle for C callers.
pub struct AadtSession {
    inner: Session,
    last_feedback: CString,
    /// Last path used with OPEN/SAVE (best-effort; host may also track).
    file_path: Option<PathBuf>,
    dirty: bool,
    /// Cached world-space line segments (x0,y0,x1,y1) for Direct2D.
    line_xy: Vec<f32>,
}

fn feedback_cstring(fb: &Feedback) -> CString {
    let s = match fb {
        Feedback::Ok(m) => m.clone(),
        Feedback::Err(m) => format!("error: {m}"),
        Feedback::Silent => String::new(),
    };
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn push_seg(out: &mut Vec<f32>, a: Point2, b: Point2) {
    out.push(a.x as f32);
    out.push(a.y as f32);
    out.push(b.x as f32);
    out.push(b.y as f32);
}

fn tessellate_drawlist(list: &aadt_render::DrawList, out: &mut Vec<f32>) {
    out.clear();
    for cmd in &list.commands {
        match cmd {
            DrawCommand::Polyline { points, closed, .. } => {
                if points.len() < 2 {
                    continue;
                }
                for w in points.windows(2) {
                    push_seg(out, w[0], w[1]);
                }
                if *closed && points.len() > 2 {
                    push_seg(out, *points.last().unwrap(), points[0]);
                }
            }
            DrawCommand::Circle {
                center, radius, ..
            } => {
                let n = 48.max((radius.abs() as usize).saturating_mul(4).min(128));
                let mut prev = Point2 {
                    x: center.x + radius,
                    y: center.y,
                };
                for i in 1..=n {
                    let t = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let p = Point2 {
                        x: center.x + radius * t.cos(),
                        y: center.y + radius * t.sin(),
                    };
                    push_seg(out, prev, p);
                    prev = p;
                }
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                ..
            } => {
                let a0 = *start_angle;
                let mut a1 = *end_angle;
                while a1 < a0 {
                    a1 += 360.0;
                }
                let sweep = a1 - a0;
                let n = 24.max(((sweep.abs() / 360.0) * 64.0) as usize).min(96);
                let mut prev = Point2 {
                    x: center.x + radius * a0.to_radians().cos(),
                    y: center.y + radius * a0.to_radians().sin(),
                };
                for i in 1..=n {
                    let t = a0 + sweep * (i as f64) / (n as f64);
                    let p = Point2 {
                        x: center.x + radius * t.to_radians().cos(),
                        y: center.y + radius * t.to_radians().sin(),
                    };
                    push_seg(out, prev, p);
                    prev = p;
                }
            }
            DrawCommand::Fill { points, .. } => {
                if points.len() < 2 {
                    continue;
                }
                for w in points.windows(2) {
                    push_seg(out, w[0], w[1]);
                }
                if points.len() > 2 {
                    push_seg(out, *points.last().unwrap(), points[0]);
                }
            }
            DrawCommand::Image { corners, .. } => {
                if corners.len() < 2 {
                    continue;
                }
                for w in corners.windows(2) {
                    push_seg(out, w[0], w[1]);
                }
                if corners.len() > 2 {
                    push_seg(out, *corners.last().unwrap(), corners[0]);
                }
            }
            DrawCommand::Text { position, .. } | DrawCommand::Marker { position, .. } => {
                let s = 0.15;
                push_seg(
                    out,
                    Point2 {
                        x: position.x - s,
                        y: position.y,
                    },
                    Point2 {
                        x: position.x + s,
                        y: position.y,
                    },
                );
                push_seg(
                    out,
                    Point2 {
                        x: position.x,
                        y: position.y - s,
                    },
                    Point2 {
                        x: position.x,
                        y: position.y + s,
                    },
                );
            }
        }
    }
}

fn track_open_save(s: &mut AadtSession, line: &str, fb: &Feedback) {
    if fb.is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let Some(head) = parts.next() else {
        return;
    };
    let up = head.to_ascii_uppercase();
    if up == "OPEN" || up == "SAVE" {
        if let Some(path) = parts.next() {
            if !path.is_empty() {
                s.file_path = Some(PathBuf::from(path));
                if up == "SAVE" {
                    s.dirty = false;
                }
            }
        }
    } else if !matches!(up.as_str(), "UNDO" | "REDO" | "LIST" | "DIST" | "ZOOM" | "PAN") {
        // Heuristic: mutating commands dirty the doc (host may refine later).
        if !fb.message().unwrap_or("").is_empty() || matches!(fb, Feedback::Silent) {
            let skip = matches!(
                up.as_str(),
                "SELECT" | "DESELECT" | "LAYER" | "LAYERS" | "UNITS" | "OSNAP" | "ORTHO"
            );
            if !skip {
                s.dirty = true;
            }
        }
    }
}

/// Create a session with drawing built-ins **plus** CLI host commands
/// (`OPEN`/`SAVE`/assist/takeoff/…).
#[no_mangle]
pub extern "C" fn aadt_session_new() -> *mut AadtSession {
    let mut interp = Interpreter::with_builtins();
    register(&mut interp);
    let session = Session::from_interpreter(interp);
    Box::into_raw(Box::new(AadtSession {
        inner: session,
        last_feedback: CString::new("").unwrap(),
        file_path: None,
        dirty: false,
        line_xy: Vec::new(),
    }))
}

/// Free a session created by [`aadt_session_new`].
///
/// # Safety
/// `session` must be null or a pointer from [`aadt_session_new`] not yet freed.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_free(session: *mut AadtSession) {
    if !session.is_null() {
        drop(Box::from_raw(session));
    }
}

/// Feed one command line (UTF-8). Returns 0 on success / silent, 1 on error.
///
/// # Safety
/// `session` must be valid. `line` must be a non-null NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_feed(session: *mut AadtSession, line: *const c_char) -> i32 {
    if session.is_null() || line.is_null() {
        return 1;
    }
    let s = &mut *session;
    let Ok(text) = CStr::from_ptr(line).to_str() else {
        return 1;
    };
    let fb = s.inner.feed(text);
    track_open_save(s, text, &fb);
    let code = i32::from(fb.is_err());
    s.last_feedback = feedback_cstring(&fb);
    code
}

/// Last feedback message from [`aadt_session_feed`] (empty if silent).
/// Borrowed; valid until the next feed/free. Do not free this pointer.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_last_feedback(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).last_feedback.as_ptr()
}

/// Entity count in the working document.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_entity_count(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.document().len() as u64
}

/// Selection count (last SELECT).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_selection_count(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.selection().len() as u64
}

/// Document revision (increments on mutating edits).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_document_revision(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.document().revision()
}

/// 1 if the host should treat the drawing as modified since last SAVE.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_is_dirty(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).dirty)
}

/// Heap UTF-8 path from last successful OPEN/SAVE, or null. Free with
/// [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_path(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let Some(p) = &(*session).file_path else {
        return ptr::null_mut();
    };
    let s = p.to_string_lossy();
    CString::new(s.as_ref().replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Drawlist primitive count (snapshot length).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_len(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.drawlist().len() as u64
}

/// Rebuild the tessellated line cache from the current drawlist. Returns the
/// number of floats in the cache (always a multiple of 4: x0,y0,x1,y1 per seg).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_rebuild_lines(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    let list = s.inner.drawlist().clone();
    tessellate_drawlist(&list, &mut s.line_xy);
    s.line_xy.len() as u64
}

/// Copy up to `max_floats` values from the line cache into `out_xy`.
/// Returns floats written. Call [`aadt_drawlist_rebuild_lines`] first.
///
/// # Safety
/// `session` must be valid. `out_xy` must point to at least `max_floats` f32s when non-null.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_copy_lines(
    session: *const AadtSession,
    out_xy: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out_xy.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).line_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    // Round down to whole segments.
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out_xy, n);
    n as u64
}

/// Axis-aligned bounds of the line cache (world units). Returns 0 if empty.
/// Writes min_x, min_y, max_x, max_y into `out4` (4 floats).
///
/// # Safety
/// `session` and `out4` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_bounds(session: *const AadtSession, out4: *mut f32) -> i32 {
    if session.is_null() || out4.is_null() {
        return 0;
    }
    let xy = &(*session).line_xy;
    if xy.len() < 4 {
        return 0;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for chunk in xy.chunks_exact(2) {
        min_x = min_x.min(chunk[0]);
        min_y = min_y.min(chunk[1]);
        max_x = max_x.max(chunk[0]);
        max_y = max_y.max(chunk[1]);
    }
    *out4 = min_x;
    *out4.add(1) = min_y;
    *out4.add(2) = max_x;
    *out4.add(3) = max_y;
    1
}

/// Free a string returned by this ABI that was allocated with `CString::into_raw`
/// (e.g. [`aadt_session_path`]). Do **not** free [`aadt_last_feedback`].
///
/// # Safety
/// `s` must be null or a pointer previously returned for the caller to free.
#[no_mangle]
pub unsafe extern "C" fn aadt_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// ABI version (bump when breaking C layouts).
#[no_mangle]
pub extern "C" fn aadt_ffi_version() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn feed_line_and_count_entities() {
        let session = aadt_session_new();
        assert!(!session.is_null());
        let line = CString::new("LINE 0 0 10 0").unwrap();
        unsafe {
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            assert_eq!(aadt_entity_count(session), 1);
            assert!(aadt_drawlist_len(session) >= 1);
            let n = aadt_drawlist_rebuild_lines(session);
            assert!(n >= 4, "expected at least one segment, got {n}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn lisp_via_feed() {
        let session = aadt_session_new();
        let line = CString::new("(+ 2 3)").unwrap();
        unsafe {
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            let fb = CStr::from_ptr(aadt_last_feedback(session))
                .to_str()
                .unwrap();
            assert!(fb.contains('5'), "{fb}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn host_commands_registered() {
        let session = aadt_session_new();
        // UNDO is a built-in; OPEN should exist (error on missing file, not unknown cmd).
        let line = CString::new("OPEN __no_such_file_aadt__.dxf").unwrap();
        unsafe {
            let code = aadt_session_feed(session, line.as_ptr());
            assert_eq!(code, 1);
            let fb = CStr::from_ptr(aadt_last_feedback(session))
                .to_str()
                .unwrap()
                .to_ascii_lowercase();
            assert!(
                !fb.contains("unknown"),
                "OPEN should be registered, got: {fb}"
            );
            aadt_session_free(session);
        }
    }
}
