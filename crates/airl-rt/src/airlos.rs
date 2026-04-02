//! AIRLOS syscall interface and IPC helpers.
//!
//! This module provides thin wrappers around AIRLOS kernel syscalls
//! (invoked via `int 0x80` on x86) and IPC service lookup/messaging.
//! Only compiled when `target_os = "airlos"`.

use core::arch::asm;

// ── Syscall numbers ──

pub const SYS_EXIT: u32 = 0;
pub const SYS_SEND: u32 = 3;
pub const SYS_RECV: u32 = 4;
pub const SYS_LOOKUP_SERVICE: u32 = 21;
pub const SYS_GET_TICKS: u32 = 30;
pub const SYS_READ_FILE: u32 = 32;
pub const SYS_LIST_FILES: u32 = 36;

// ── Raw syscall wrappers ──

#[inline(always)]
pub unsafe fn syscall0(num: u32) -> i32 {
    let ret: i32;
    asm!(
        "int 0x80",
        in("eax") num,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall1(num: u32, arg1: u32) -> i32 {
    let ret: i32;
    asm!(
        "int 0x80",
        in("eax") num,
        in("ebx") arg1,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(num: u32, arg1: u32, arg2: u32) -> i32 {
    let ret: i32;
    asm!(
        "int 0x80",
        in("eax") num,
        in("ebx") arg1,
        in("ecx") arg2,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(num: u32, arg1: u32, arg2: u32, arg3: u32) -> i32 {
    let ret: i32;
    asm!(
        "int 0x80",
        in("eax") num,
        in("ebx") arg1,
        in("ecx") arg2,
        in("edx") arg3,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

// ── High-level helpers ──

/// Exit the process with the given status code.
pub fn exit(code: i32) -> ! {
    unsafe { syscall1(SYS_EXIT, code as u32); }
    // Should never return, but loop just in case
    loop {}
}

/// Get the current tick count from the kernel (monotonic, milliseconds).
pub fn get_ticks() -> u64 {
    unsafe { syscall0(SYS_GET_TICKS) as u64 }
}

/// Look up a service by name. Returns the service ID (>0) or negative error.
pub fn lookup_service(name: &str) -> i32 {
    unsafe {
        syscall2(
            SYS_LOOKUP_SERVICE,
            name.as_ptr() as u32,
            name.len() as u32,
        )
    }
}

/// Send an IPC message to a service. Returns 0 on success or negative error.
pub fn ipc_send(service_id: i32, data: &[u8]) -> i32 {
    unsafe {
        syscall3(
            SYS_SEND,
            service_id as u32,
            data.as_ptr() as u32,
            data.len() as u32,
        )
    }
}

/// Receive an IPC message (blocking). Writes into `buf`, returns bytes received or negative error.
pub fn ipc_recv(service_id: i32, buf: &mut [u8]) -> i32 {
    unsafe {
        syscall3(
            SYS_RECV,
            service_id as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    }
}

/// Read a file from the ramdisk. Returns the file contents or an error string.
pub fn read_file(path: &str) -> Result<Vec<u8>, &'static str> {
    // First call with a reasonably large buffer
    let mut buf = vec![0u8; 65536];
    let ret = unsafe {
        syscall3(
            SYS_READ_FILE,
            path.as_ptr() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if ret < 0 {
        Err("file not found")
    } else {
        buf.truncate(ret as usize);
        Ok(buf)
    }
}

/// Check if a file exists in the ramdisk.
pub fn file_exists(path: &str) -> bool {
    // Try to read with a zero-length buffer; if the file exists the syscall
    // returns 0 (truncated) rather than a negative error code.
    let ret = unsafe {
        syscall3(
            SYS_READ_FILE,
            path.as_ptr() as u32,
            core::ptr::null_mut::<u8>() as u32,
            0,
        )
    };
    ret >= 0
}

/// List files in the ramdisk. Returns filenames as a Vec<String>.
pub fn list_files() -> Vec<String> {
    let mut buf = vec![0u8; 8192];
    let count = unsafe {
        syscall2(
            SYS_LIST_FILES,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if count <= 0 {
        return Vec::new();
    }
    // Buffer contains null-terminated strings packed sequentially
    let mut names = Vec::new();
    let mut start = 0;
    for i in 0..buf.len() {
        if buf[i] == 0 {
            if i > start {
                if let Ok(s) = core::str::from_utf8(&buf[start..i]) {
                    names.push(s.to_string());
                }
            }
            start = i + 1;
            if names.len() >= count as usize {
                break;
            }
        }
    }
    names
}

/// Send a string to the VGA service for display.
pub fn vga_print(s: &str) {
    let svc = lookup_service("vga");
    if svc > 0 {
        ipc_send(svc, s.as_bytes());
    }
}

/// Read a line from the keyboard service (blocking).
/// Returns the line without trailing newline, or None on error.
pub fn keyboard_read_line() -> Option<String> {
    let svc = lookup_service("keyboard");
    if svc <= 0 {
        return None;
    }
    // Send empty message to request input
    ipc_send(svc, &[]);
    // Receive response
    let mut buf = [0u8; 1024];
    let n = ipc_recv(svc, &mut buf);
    if n <= 0 {
        return None;
    }
    let s = core::str::from_utf8(&buf[..n as usize]).unwrap_or("").to_string();
    // Strip trailing newline
    let s = s.trim_end_matches('\n').trim_end_matches('\r').to_string();
    Some(s)
}
