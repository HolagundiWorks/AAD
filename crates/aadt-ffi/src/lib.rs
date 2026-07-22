//! # aadt-ffi — Thin C ABI over [`aadt_app::Session`]
//!
//! Lets a future native chrome (WinUI, etc.) drive the same command/LISP/undo
//! loop as the egui shell without rewriting the engine. This crate has **no UI**.
//!
//! # Safety
//!
//! All exported functions are `extern "C"`. Callers must:
//! - Treat [`AadtSession`] handles as opaque.
//! - Free strings returned by `aadt_last_feedback` / `aadt_takeoff_summary` with
//!   [`aadt_string_free`].
//! - Not use a handle after [`aadt_session_free`].

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use aadt_app::{Feedback, Session};
use aadt_command::Interpreter;

/// Opaque session handle for C callers.
pub struct AadtSession {
    inner: Session,
    last_feedback: CString,
}

fn feedback_cstring(fb: &Feedback) -> CString {
    let s = match fb {
        Feedback::Ok(m) => m.clone(),
        Feedback::Err(m) => format!("error: {m}"),
        Feedback::Silent => String::new(),
    };
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// Create a session with drawing built-ins only. Register engine commands from
/// the host process if needed (OPEN/SAVE live in the CLI today).
#[no_mangle]
pub extern "C" fn aadt_session_new() -> *mut AadtSession {
    let session = Session::from_interpreter(Interpreter::with_builtins());
    Box::into_raw(Box::new(AadtSession {
        inner: session,
        last_feedback: CString::new("").unwrap(),
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

/// Drawlist primitive count (snapshot length).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_len(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.drawlist().len() as u64
}

/// Allocate a short takeoff summary string. Caller must free with
/// [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_takeoff_summary(session: *mut AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let s = &mut *session;
    let t = s.inner.takeoff();
    let text = format!(
        "walls={} rooms={} wall_len={:.3} room_area={:.3}",
        t.walls.len(),
        t.rooms.len(),
        t.total_wall_length(),
        t.total_room_area()
    );
    CString::new(text.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Free a string returned by [`aadt_takeoff_summary`].
///
/// # Safety
/// `s` must be null or a pointer from [`aadt_takeoff_summary`].
#[no_mangle]
pub unsafe extern "C" fn aadt_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
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
            let summary = aadt_takeoff_summary(session);
            assert!(!summary.is_null());
            aadt_string_free(summary);
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
}
