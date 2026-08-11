//! Optional load of `aadt_bridge.dll` (Aorms.Bridge NativeAOT) for firm.db SSO.
//! Activate stays in AORMS Connect.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub loaded: bool,
    pub bound: bool,
    pub sync_ready: bool,
    pub hub_url: String,
    pub firm_db: String,
    pub note: String,
}

type EnsureFn = unsafe extern "C" fn() -> c_int;
type StatusFn = unsafe extern "C" fn(
    *mut c_char,
    c_int,
    *mut c_char,
    c_int,
    *mut c_char,
    c_int,
    *mut c_int,
) -> c_int;

/// Try `AADT_BRIDGE_DLL`, then beside current exe, then PATH.
pub fn try_ensure_status() -> BridgeStatus {
    let mut st = BridgeStatus {
        loaded: false,
        bound: false,
        sync_ready: false,
        hub_url: String::new(),
        firm_db: String::new(),
        note: "aadt_bridge.dll not loaded — run bridge\\build-bridge.cmd".into(),
    };

    let path = resolve_dll_path();
    let Ok(lib) = (unsafe { libloading::Library::new(&path) }) else {
        return st;
    };

    let ensure: libloading::Symbol<EnsureFn> = match unsafe { lib.get(b"aadt_bridge_ensure\0") } {
        Ok(s) => s,
        Err(_) => return st,
    };
    let status: libloading::Symbol<StatusFn> = match unsafe { lib.get(b"aadt_bridge_status\0") } {
        Ok(s) => s,
        Err(_) => return st,
    };

    unsafe {
        let _ = ensure();
        let mut hub = [0i8; 512];
        let mut firm = [0i8; 1024];
        let mut note = [0i8; 512];
        let mut sync_ready: c_int = 0;
        let rc = status(
            hub.as_mut_ptr(),
            hub.len() as c_int,
            firm.as_mut_ptr(),
            firm.len() as c_int,
            note.as_mut_ptr(),
            note.len() as c_int,
            &mut sync_ready,
        );
        st.loaded = true;
        st.bound = rc == 1;
        st.sync_ready = sync_ready != 0;
        st.hub_url = cbuf_to_string(&hub);
        st.firm_db = cbuf_to_string(&firm);
        st.note = cbuf_to_string(&note);
        if st.note.is_empty() {
            st.note = if st.bound {
                "Licence in firm.db (Aorms.Bridge)".into()
            } else {
                "Unbound — Activate in AORMS Connect".into()
            };
        }
    }
    // Keep lib loaded for process lifetime (drop would unload).
    std::mem::forget(lib);
    st
}

fn resolve_dll_path() -> PathBuf {
    if let Ok(p) = std::env::var("AADT_BRIDGE_DLL") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("aadt_bridge.dll");
            if cand.is_file() {
                return cand;
            }
        }
    }
    PathBuf::from("aadt_bridge.dll")
}

fn cbuf_to_string(buf: &[i8]) -> String {
    let ptr = buf.as_ptr();
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
