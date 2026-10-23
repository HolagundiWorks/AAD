//! Windows Job Object memory cap for the DWG worker process.
//!
//! Soft byte limits in [`crate::Limits`] cannot stop an unchecked alloc inside acadrust; a Job
//! Object kills the worker when it exceeds the process memory limit so the parent stays up.

#![allow(unsafe_code)]

use std::os::windows::io::AsRawHandle;
use std::process::Child;

type HANDLE = *mut core::ffi::c_void;
type BOOL = i32;
type DWORD = u32;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: DWORD = 0x0000_0100;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: DWORD = 9;

#[repr(C)]
struct IoCounters {
    read_op: u64,
    write_op: u64,
    other_op: u64,
    read_xfer: u64,
    write_xfer: u64,
    other_xfer: u64,
}

#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: DWORD,
    min_working_set_size: usize,
    max_working_set_size: usize,
    active_process_limit: DWORD,
    affinity: usize,
    priority_class: DWORD,
    scheduling_class: DWORD,
}

#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic: JobObjectBasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attrs: *mut core::ffi::c_void, name: *const u16) -> HANDLE;
    fn SetInformationJobObject(job: HANDLE, class: DWORD, info: *mut core::ffi::c_void, len: DWORD) -> BOOL;
    fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> BOOL;
    fn CloseHandle(handle: HANDLE) -> BOOL;
}

/// Cap the worker's committed memory. On failure the child keeps running uncapped (parent still
/// has the timeout); returns `Err` so callers can log.
pub fn limit_process_memory(child: &Child, max_bytes: usize) -> Result<(), String> {
    if max_bytes == 0 {
        return Ok(());
    }
    // SAFETY: Win32 job APIs; handles are process-local and closed on all paths.
    unsafe {
        let job = CreateJobObjectW(core::ptr::null_mut(), core::ptr::null());
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return Err("DWG: could not create a job object for the reader".into());
        }
        let mut info = JobObjectExtendedLimitInformation {
            basic: JobObjectBasicLimitInformation {
                per_process_user_time_limit: 0,
                per_job_user_time_limit: 0,
                limit_flags: JOB_OBJECT_LIMIT_PROCESS_MEMORY,
                min_working_set_size: 0,
                max_working_set_size: 0,
                active_process_limit: 0,
                affinity: 0,
                priority_class: 0,
                scheduling_class: 0,
            },
            io: IoCounters { read_op: 0, write_op: 0, other_op: 0, read_xfer: 0, write_xfer: 0, other_xfer: 0 },
            process_memory_limit: max_bytes,
            job_memory_limit: 0,
            peak_process_memory_used: 0,
            peak_job_memory_used: 0,
        };
        let ok = SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            (&raw mut info).cast(),
            core::mem::size_of_val(&info) as DWORD,
        );
        if ok == 0 {
            let _ = CloseHandle(job);
            return Err("DWG: could not set the reader memory limit".into());
        }
        let process = child.as_raw_handle() as HANDLE;
        if AssignProcessToJobObject(job, process) == 0 {
            let _ = CloseHandle(job);
            return Err("DWG: could not put the reader in the job object".into());
        }
        // Leak the job handle for the lifetime of the child: closing it would end the job and
        // kill the process. The OS reclaims it when the last process in the job exits.
        let _ = job;
        Ok(())
    }
}
