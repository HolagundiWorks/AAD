//! wgpu surface for the child drawing window.
#![allow(unsafe_code)]

use std::num::NonZeroIsize;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};

/// # Safety
/// `hwnd` is a live Win32 window and outlives the returned surface.
pub fn from_hwnd(instance: &wgpu::Instance, hwnd: isize) -> Result<wgpu::Surface<'static>, String> {
    let hwnd = NonZeroIsize::new(hwnd).ok_or("missing window handle")?;
    let mut window = Win32WindowHandle::new(hwnd);
    let module = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) };
    if let Some(module) = NonZeroIsize::new(module as isize) {
        window.hinstance = Some(module);
    }
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
        raw_window_handle: RawWindowHandle::Win32(window),
    };
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
}
