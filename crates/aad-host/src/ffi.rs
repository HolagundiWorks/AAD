//! C ABI for the WinUI shell. Pointers are only dereferenced for the duration of the call.
#![allow(unsafe_code)]

use std::ffi::c_char;
use std::panic::AssertUnwindSafe;

use serde_json::Value;

use crate::{Host, Pointer, Stats};

const TEXT_CAP: usize = 1_048_576;

/// `sample` non-zero opens the sample drawing. Never null.
#[unsafe(no_mangle)]
pub extern "C" fn aad_host_create(sample: i32) -> *mut Host {
    Box::into_raw(Box::new(Host::create(sample != 0)))
}

/// # Safety
/// `host` is null or a pointer returned by [`aad_host_create`] that has not been destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_destroy(host: *mut Host) {
    if !host.is_null() {
        drop(unsafe { Box::from_raw(host) });
    }
}

/// # Safety
/// `host` is live. `command` and `params` are null-terminated UTF-8, each at most 1 MiB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_execute(host: *mut Host, command: *const c_char, params: *const c_char) -> i32 {
    call(host, |host| {
        let command = read_text(command)?;
        let params = read_text(params)?;
        let value: Value = if params.is_empty() { Value::Null } else { serde_json::from_str(&params).map_err(|e| e.to_string())? };
        host.execute(&command, &value)?;
        Ok(())
    })
}

/// # Safety
/// `host` is live. `text` is null-terminated UTF-8, at most 1 MiB. One command-line line.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_cmdline(host: *mut Host, text: *const c_char) -> i32 {
    call(host, |host| {
        let text = read_text(text)?;
        host.submit(&text)
    })
}

/// Remember command-line text that has not been submitted, for Dynamic Input.
///
/// # Safety
/// `host` is live. `text` is null-terminated UTF-8, at most 1 MiB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_draft(host: *mut Host, text: *const c_char) -> i32 {
    call(host, |host| {
        let text = read_text(text)?;
        host.set_draft(&text)
    })
}

/// Tab at a point prompt. Copies the text with the Dynamic Input separator appended.
/// Returns 0 when Tab does not apply. A negative length means `buf` was too small.
///
/// # Safety
/// `host` is live. `text` is null-terminated UTF-8. `buf` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_dyn_tab(host: *mut Host, text: *const c_char, buf: *mut u8, len: u32) -> i32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return 0;
    };
    let text = match read_text(text) {
        Ok(text) => text,
        Err(_) => return 0,
    };
    match host.dyn_tab(&text) {
        Some(next) => copy_bytes(next.as_bytes(), buf, len),
        None => 0,
    }
}

/// The settings dialog for `id`. Returns 0 when that command has no dialog.
///
/// # Safety
/// `host` is live. `id` is null-terminated UTF-8. `buf` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_form(host: *mut Host, id: *const c_char, buf: *mut u8, len: u32) -> i32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return 0;
    };
    let id = match read_text(id) {
        Ok(id) => id,
        Err(_) => return 0,
    };
    match host.form_json(&id) {
        Some(json) => copy_bytes(json.as_bytes(), buf, len),
        None => 0,
    }
}

/// Start a command with the dialog's answers. Points still missing are picked on the drawing.
///
/// # Safety
/// `host` is live. `id` and `preset` are null-terminated UTF-8. `preset` is a JSON object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_start_form(host: *mut Host, id: *const c_char, preset: *const c_char) -> i32 {
    call(host, |host| {
        let id = read_text(id)?;
        let preset = read_text(preset)?;
        let value: Value = if preset.is_empty() { Value::Null } else { serde_json::from_str(&preset).map_err(|e| e.to_string())? };
        host.start_form(&id, &value)
    })
}

/// Cycle the hot grip. Returns 1 when a grip mode changed, 0 when no grip is hot.
///
/// # Safety
/// `host` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_grip_cycle(host: *mut Host) -> i32 {
    let Some(host) = (unsafe { host.as_mut() }) else {
        return 0;
    };
    i32::from(host.grip_cycle())
}

/// Tab to the next object snap under the cursor.
///
/// # Safety
/// `host` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_snap_tab(host: *mut Host) -> i32 {
    call(host, |host| host.snap_tab())
}

/// # Safety
/// `host` is live. Cancels the running command.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_cancel(host: *mut Host) -> i32 {
    call(host, |host| host.cancel_command())
}

/// # Safety
/// `host` is live. Non-zero `dark` is a black drawing area; zero is white.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_theme(host: *mut Host, dark: i32) -> i32 {
    call(host, |host| host.set_dark(dark != 0))
}

/// Copy the current layout as a one-page PDF. Returns the byte length, or a negative length when
/// `buf` is too small and nothing was written. Returns 0 and sets the last error on failure.
///
/// # Safety
/// `host` is live. `buf` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_pdf(host: *mut Host, buf: *mut u8, len: u32) -> i32 {
    let Some(host) = (unsafe { host.as_mut() }) else {
        return 0;
    };
    match std::panic::catch_unwind(AssertUnwindSafe(|| host.pdf_bytes())) {
        Ok(Ok(bytes)) => {
            host.error.clear();
            copy_bytes(&bytes, buf, len)
        }
        Ok(Err(e)) => {
            host.error = e;
            0
        }
        Err(_) => {
            #[cfg(windows)]
            {
                host.in_call = false;
            }
            host.error = "internal error".into();
            0
        }
    }
}

/// Copy a UTF-8 snapshot. `slot` 0 is the menu tree, 1 is the command-line prompt and history,
/// 2 is the properties, layers and blocks snapshot.
/// Returns the byte length, or a negative length when `buf` is too small and nothing was written.
///
/// # Safety
/// `host` is live. `buf` is either null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_text(host: *mut Host, slot: i32, buf: *mut u8, len: u32) -> i32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return 0;
    };
    let text = std::panic::catch_unwind(AssertUnwindSafe(|| match slot {
        0 => host.menu_json(),
        1 => host.chrome_json(),
        2 => host.panels_json(),
        _ => String::new(),
    }));
    let text = text.unwrap_or_default();
    copy_bytes(text.as_bytes(), buf, len)
}

/// # Safety
/// `host` is live and `ev` points at a [`Pointer`] for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_pointer(host: *mut Host, ev: *const Pointer) -> i32 {
    call(host, |host| {
        if ev.is_null() {
            return Err("missing pointer event".into());
        }
        host.pointer(unsafe { *ev })
    })
}

/// # Safety
/// `host` is live and `out` points at a [`Stats`] for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_stats(host: *mut Host, out: *mut Stats) -> i32 {
    call(host, |host| {
        if out.is_null() {
            return Err("missing stats".into());
        }
        unsafe { *out = host.stats() };
        Ok(())
    })
}

/// # Safety
/// `host` is live. `parent` is a Win32 window handle. The child is positioned in physical pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_embed(host: *mut Host, parent: isize, x: i32, y: i32, width: u32, height: u32) -> i32 {
    call(host, |host| host.embed(parent, x, y, width, height))
}

/// # Safety
/// `host` is live and already embedded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_move(host: *mut Host, x: i32, y: i32, width: u32, height: u32) -> i32 {
    call(host, |host| host.move_child(x, y, width, height))
}

/// # Safety
/// `host` is live. Applies queued pointer events and presents one frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_pump(host: *mut Host) -> i32 {
    call(host, |host| host.pump())
}

/// 1 when the child drawing window exists.
///
/// # Safety
/// `host` is null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_embedded(host: *mut Host) -> i32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return 0;
    };
    i32::from(host.hwnd != 0)
}

/// # Safety
/// `host` is live. `buf` receives UTF-8 without a terminator. Returns the byte length, or a
/// negative length when `buf` is too small and nothing was written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aad_host_last_error(host: *mut Host, buf: *mut u8, len: u32) -> i32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return 0;
    };
    copy_bytes(host.error.as_bytes(), buf, len)
}

#[cfg(test)]
pub(crate) fn create(sample: i32) -> *mut Host {
    aad_host_create(sample)
}

/// # Safety
/// Same as [`aad_host_destroy`].
#[cfg(test)]
pub(crate) unsafe fn destroy(host: *mut Host) {
    unsafe { aad_host_destroy(host) };
}

/// # Safety
/// Same as [`aad_host_execute`].
#[cfg(test)]
pub(crate) unsafe fn execute(host: *mut Host, command: *const c_char, params: *const c_char) -> i32 {
    unsafe { aad_host_execute(host, command, params) }
}

/// # Safety
/// Same as [`aad_host_stats`].
#[cfg(test)]
pub(crate) unsafe fn stats(host: *mut Host, out: *mut Stats) -> i32 {
    unsafe { aad_host_stats(host, out) }
}

#[cfg(test)]
pub(crate) fn error_text(host: *mut Host) -> String {
    let mut buf = vec![0u8; 512];
    let n = unsafe { aad_host_last_error(host, buf.as_mut_ptr(), buf.len() as u32) };
    if n <= 0 {
        return String::new();
    }
    let n = usize::try_from(n).unwrap_or(0).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn call(host: *mut Host, f: impl FnOnce(&mut Host) -> Result<(), String>) -> i32 {
    let Some(host) = (unsafe { host.as_mut() }) else {
        return 1;
    };
    match std::panic::catch_unwind(AssertUnwindSafe(|| f(host))) {
        Ok(Ok(())) => {
            host.error.clear();
            0
        }
        Ok(Err(e)) => {
            host.error = e;
            1
        }
        Err(_) => {
            #[cfg(windows)]
            {
                host.in_call = false;
            }
            host.error = "internal error".into();
            1
        }
    }
}

fn read_text(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("missing text".into());
    }
    let mut n = 0usize;
    while n < TEXT_CAP {
        if unsafe { *ptr.add(n) } == 0 {
            break;
        }
        n += 1;
    }
    if n == TEXT_CAP {
        return Err("text is too long".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), n) };
    String::from_utf8(bytes.to_vec()).map_err(|_| "text is not utf-8".into())
}

fn copy_bytes(src: &[u8], buf: *mut u8, len: u32) -> i32 {
    let needed = i32::try_from(src.len()).unwrap_or(i32::MAX);
    if buf.is_null() || u32::try_from(src.len()).unwrap_or(u32::MAX) > len {
        return -needed;
    }
    if !src.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, src.len()) };
    }
    needed
}
