//! AIRLOS syscall interface and IPC helpers.
//!
//! This module provides thin wrappers around AIRLOS kernel syscalls
//! (invoked via `int 0x80` on x86) and IPC service lookup/messaging.
//! Only compiled when `target_os = "airlos"`.

// ── Freestanding memory intrinsics (no libc) ──

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.add(i) = c as u8;
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            return a as i32 - b as i32;
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dest as usize) < (src as usize) {
        memcpy(dest, src, n);
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn bcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    memcmp(s1, s2, n)
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

use crate::nostd_prelude::*;
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

// Note: ebx is reserved by LLVM on x86_64, so we use a temp register
// and mov to/from ebx inside the asm block.

#[inline(always)]
pub unsafe fn syscall1(num: u32, arg1: u32) -> i32 {
    let ret: i32;
    asm!(
        "xchg rbx, {tmp}",
        "int 0x80",
        "xchg rbx, {tmp}",
        tmp = in(reg) arg1 as u64,
        in("eax") num,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(num: u32, arg1: u32, arg2: u32) -> i32 {
    let ret: i32;
    asm!(
        "xchg rbx, {tmp}",
        "int 0x80",
        "xchg rbx, {tmp}",
        tmp = in(reg) arg1 as u64,
        in("eax") num,
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
        "xchg rbx, {tmp}",
        "int 0x80",
        "xchg rbx, {tmp}",
        tmp = in(reg) arg1 as u64,
        in("eax") num,
        in("ecx") arg2,
        in("edx") arg3,
        lateout("eax") ret,
        options(nostack),
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall5(num: u32, arg1: u32, arg2: u32, arg3: u32, arg4: u32, arg5: u32) -> i32 {
    let ret: i32;
    asm!(
        "xchg rbx, {tmp}",
        "int 0x80",
        "xchg rbx, {tmp}",
        tmp = in(reg) arg1 as u64,
        in("eax") num,
        in("ecx") arg2,
        in("edx") arg3,
        in("esi") arg4,
        in("edi") arg5,
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
/// Note: the kernel expects a null-terminated C string, not a Rust &str.
pub fn lookup_service(name: &str) -> i32 {
    // Create null-terminated copy on stack
    let mut buf = [0u8; 64];
    let len = name.len().min(63);
    buf[..len].copy_from_slice(&name.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        syscall1(SYS_LOOKUP_SERVICE, buf.as_ptr() as u32)
    }
}

/// Send an IPC message (fire-and-forget). Returns 0 on success or negative error.
pub fn ipc_send(service_id: i32, data: &[u8]) -> i32 {
    let len = data.len().min(256);
    unsafe {
        syscall3(
            SYS_SEND,
            service_id as u32,
            data.as_ptr() as u32,
            len as u32,
        )
    }
}

/// Send an IPC message and receive the reply into `recv_buf`.
/// Uses SYS_SENDRECV which overwrites the send buffer with the reply.
/// Returns the number of bytes received, or a negative error code.
pub fn ipc_sendrecv(service_id: i32, send_data: &[u8], recv_buf: &mut [u8]) -> i32 {
    const SYS_SENDRECV: u32 = 12;
    let mut buf = [0u8; 256];
    let send_len = send_data.len().min(256);
    buf[..send_len].copy_from_slice(&send_data[..send_len]);
    let r = unsafe {
        syscall3(SYS_SENDRECV, service_id as u32, buf.as_mut_ptr() as u32, send_len as u32)
    };
    if r > 0 {
        let n = (r as usize).min(recv_buf.len()).min(256);
        recv_buf[..n].copy_from_slice(&buf[..n]);
    }
    r
}

/// Receive an IPC message (blocking). Writes into `buf`, returns bytes received or negative error.
pub fn ipc_recv(service_id: i32, buf: &mut [u8]) -> i32 {
    let mut src_id: u32 = service_id as u32;
    unsafe {
        syscall3(
            SYS_RECV,
            &mut src_id as *mut u32 as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    }
}

// ── VFS IPC constants ──

const FS_OPEN: u32 = 0x600;
const FS_CLOSE: u32 = 0x601;
const FS_READ: u32 = 0x602;
const FS_WRITE: u32 = 0x603;
const FS_STAT: u32 = 0x604;
const FS_READDIR: u32 = 0x605;
const FS_MKDIR: u32 = 0x606;
const FS_UNLINK: u32 = 0x607;
const FS_OK: u32 = 0x6F0;
const FS_ERR: u32 = 0x6F1;

const O_READ: u32 = 0x01;
const O_WRITE: u32 = 0x02;
const O_CREATE: u32 = 0x08;

const VFS_CHUNK_SIZE: usize = 240;

// ── VFS IPC helpers ──

fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
}

/// Look up the VFS ("fs") service, caching the result.
/// Returns the service ID (>0) or 0 if not available.
fn get_fs_port() -> i32 {
    static FS_SVC: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
    let mut svc = FS_SVC.load(core::sync::atomic::Ordering::Relaxed);
    if svc <= 0 {
        for _ in 0..5000 {
            svc = lookup_service("fs");
            if svc > 0 {
                FS_SVC.store(svc, core::sync::atomic::Ordering::Relaxed);
                return svc;
            }
            unsafe { syscall0(2); } // SYS_YIELD
        }
        return 0;
    }
    svc
}

/// Send a VFS request and receive the response. Returns bytes received or negative error.
fn vfs_sendrecv(msg: &[u8], recv_buf: &mut [u8]) -> i32 {
    let fs = get_fs_port();
    if fs <= 0 {
        return -1;
    }
    ipc_sendrecv(fs, msg, recv_buf)
}

/// Resolve special paths. AIRLOS has no per-process CWD, so "." maps to "/".
fn resolve_path(path: &str) -> &str {
    if path == "." || path.is_empty() {
        "/"
    } else {
        path
    }
}

/// Open a file via the VFS. Returns the handle_id or negative error.
fn vfs_open(path: &str, flags: u32) -> i32 {
    let path = resolve_path(path);
    let path_bytes = path.as_bytes();
    let path_len = path_bytes.len().min(VFS_CHUNK_SIZE);
    let msg_len = 12 + path_len;
    let mut msg = [0u8; 256];
    write_u32_le(&mut msg, 0, FS_OPEN);
    write_u32_le(&mut msg, 4, flags);
    write_u32_le(&mut msg, 8, path_len as u32);
    msg[12..12 + path_len].copy_from_slice(&path_bytes[..path_len]);

    let mut resp = [0u8; 256];
    let n = vfs_sendrecv(&msg[..msg_len], &mut resp);
    if n < 8 {
        return -1;
    }
    let resp_type = read_u32_le(&resp, 0);
    if resp_type != FS_OK {
        return -1;
    }
    read_u32_le(&resp, 4) as i32
}

/// Close a VFS file handle.
fn vfs_close(handle: u32) {
    let mut msg = [0u8; 8];
    write_u32_le(&mut msg, 0, FS_CLOSE);
    write_u32_le(&mut msg, 4, handle);
    let mut resp = [0u8; 256];
    vfs_sendrecv(&msg, &mut resp);
}

// ── File I/O via VFS ──

/// Read a file via the VFS server. Falls back to ramdisk if VFS is unavailable.
pub fn read_file(path: &str) -> Result<Vec<u8>, &'static str> {
    // Try VFS first
    let handle = vfs_open(path, O_READ);
    if handle < 0 {
        // Fallback to ramdisk syscall (graceful degradation during early boot)
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
            return Err("file not found");
        }
        buf.truncate(ret as usize);
        return Ok(buf);
    }

    let mut data = Vec::new();
    let mut msg = [0u8; 12];
    write_u32_le(&mut msg, 0, FS_READ);
    write_u32_le(&mut msg, 4, handle as u32);
    write_u32_le(&mut msg, 8, VFS_CHUNK_SIZE as u32);

    loop {
        let mut resp = [0u8; 256];
        let n = vfs_sendrecv(&msg, &mut resp);
        if n < 8 {
            break;
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type != FS_OK {
            break;
        }
        let bytes_read = read_u32_le(&resp, 4) as usize;
        if bytes_read == 0 {
            break;
        }
        let end = (8 + bytes_read).min(n as usize);
        data.extend_from_slice(&resp[8..end]);
        if bytes_read < VFS_CHUNK_SIZE {
            break;
        }
    }

    vfs_close(handle as u32);
    Ok(data)
}

/// Write a file via the VFS server.
pub fn write_file(path: &str, content: &[u8]) -> Result<(), &'static str> {
    let handle = vfs_open(path, O_WRITE | O_CREATE);
    if handle < 0 {
        return Err("failed to open file for writing");
    }

    let mut offset = 0;
    while offset < content.len() {
        let chunk_len = (content.len() - offset).min(VFS_CHUNK_SIZE);
        let msg_len = 12 + chunk_len;
        let mut msg = [0u8; 256];
        write_u32_le(&mut msg, 0, FS_WRITE);
        write_u32_le(&mut msg, 4, handle as u32);
        write_u32_le(&mut msg, 8, chunk_len as u32);
        msg[12..12 + chunk_len].copy_from_slice(&content[offset..offset + chunk_len]);

        let mut resp = [0u8; 256];
        let n = vfs_sendrecv(&msg[..msg_len], &mut resp);
        if n < 8 || read_u32_le(&resp, 0) != FS_OK {
            vfs_close(handle as u32);
            return Err("write failed");
        }
        let bytes_written = read_u32_le(&resp, 4) as usize;
        offset += bytes_written;
    }

    vfs_close(handle as u32);
    Ok(())
}

/// Send FS_STAT for a path. Returns (file_type, size) or None on error.
fn vfs_stat(path: &str) -> Option<(u32, u32)> {
    let path = resolve_path(path);
    let path_bytes = path.as_bytes();
    let path_len = path_bytes.len().min(VFS_CHUNK_SIZE);
    let msg_len = 8 + path_len;
    let mut msg = [0u8; 256];
    write_u32_le(&mut msg, 0, FS_STAT);
    write_u32_le(&mut msg, 4, path_len as u32);
    msg[8..8 + path_len].copy_from_slice(&path_bytes[..path_len]);

    let mut resp = [0u8; 256];
    let n = vfs_sendrecv(&msg[..msg_len], &mut resp);
    if n < 12 {
        return None;
    }
    if read_u32_le(&resp, 0) != FS_OK {
        return None;
    }
    let file_type = read_u32_le(&resp, 4);
    let size = read_u32_le(&resp, 8);
    Some((file_type, size))
}

/// Check if a file exists via the VFS server.
pub fn file_exists(path: &str) -> bool {
    vfs_stat(path).is_some()
}

/// Get the size of a file in bytes.
pub fn file_size(path: &str) -> Option<u32> {
    vfs_stat(path).map(|(_, size)| size)
}

/// Check if a path is a directory.
pub fn is_dir(path: &str) -> bool {
    vfs_stat(path).map_or(false, |(file_type, _)| file_type == 1)
}

/// Read directory entries via the VFS server.
pub fn read_dir(path: &str) -> Vec<String> {
    let handle = vfs_open(path, O_READ);
    if handle < 0 {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut index: u32 = 0;
    loop {
        let mut msg = [0u8; 12];
        write_u32_le(&mut msg, 0, FS_READDIR);
        write_u32_le(&mut msg, 4, handle as u32);
        write_u32_le(&mut msg, 8, index);

        let mut resp = [0u8; 256];
        let n = vfs_sendrecv(&msg, &mut resp);
        if n < 4 {
            break;
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == FS_ERR {
            break; // E_EOF or other error
        }
        if resp_type != FS_OK || n < 16 {
            break;
        }
        // resp: [type:4][file_type:4][file_size:4][name_length:4][name:...]
        let name_length = read_u32_le(&resp, 12) as usize;
        let name_end = (16 + name_length).min(n as usize);
        if let Ok(name) = core::str::from_utf8(&resp[16..name_end]) {
            entries.push(name.to_string());
        }
        index += 1;
    }

    vfs_close(handle as u32);
    entries
}

/// Create a directory via the VFS server.
pub fn create_dir(path: &str) -> Result<(), &'static str> {
    let path_bytes = path.as_bytes();
    let path_len = path_bytes.len().min(VFS_CHUNK_SIZE);
    let msg_len = 8 + path_len;
    let mut msg = [0u8; 256];
    write_u32_le(&mut msg, 0, FS_MKDIR);
    write_u32_le(&mut msg, 4, path_len as u32);
    msg[8..8 + path_len].copy_from_slice(&path_bytes[..path_len]);

    let mut resp = [0u8; 256];
    let n = vfs_sendrecv(&msg[..msg_len], &mut resp);
    if n < 4 || read_u32_le(&resp, 0) != FS_OK {
        return Err("mkdir failed");
    }
    Ok(())
}

/// Delete a file via the VFS server.
pub fn delete_file(path: &str) -> Result<(), &'static str> {
    let path_bytes = path.as_bytes();
    let path_len = path_bytes.len().min(VFS_CHUNK_SIZE);
    let msg_len = 8 + path_len;
    let mut msg = [0u8; 256];
    write_u32_le(&mut msg, 0, FS_UNLINK);
    write_u32_le(&mut msg, 4, path_len as u32);
    msg[8..8 + path_len].copy_from_slice(&path_bytes[..path_len]);

    let mut resp = [0u8; 256];
    let n = vfs_sendrecv(&msg[..msg_len], &mut resp);
    if n < 4 || read_u32_le(&resp, 0) != FS_OK {
        return Err("unlink failed");
    }
    Ok(())
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
/// Write to serial port via SYS_WRITE_BUF (syscall 7) — always works, no IPC needed.
pub fn serial_print(s: &str) {
    const SYS_WRITE_BUF: u32 = 7;
    unsafe {
        syscall2(SYS_WRITE_BUF, s.as_ptr() as u32, s.len() as u32);
    }
}

pub fn vga_print(s: &str) {
    // Also echo to serial for debugging
    serial_print(s);
    // Cache VGA service ID after first successful lookup
    static VGA_SVC: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
    let mut svc = VGA_SVC.load(core::sync::atomic::Ordering::Relaxed);
    if svc <= 0 {
        // Wait for VGA service to register
        for _ in 0..50000 {
            svc = lookup_service("vga");
            if svc > 0 {
                VGA_SVC.store(svc, core::sync::atomic::Ordering::Relaxed);
                serial_print("VGA_FOUND\n");
                break;
            }
            unsafe { syscall0(2); } // SYS_YIELD
        }
    }
    if svc > 0 {
        let mut ack = [0u8; 1];
        ipc_sendrecv(svc, s.as_bytes(), &mut ack);
    }
}

/// Read a line from the keyboard service (blocking).
/// Reads characters one at a time via IPC, echoes each to VGA,
/// and accumulates into a line buffer until Enter is pressed.
/// Supports backspace editing and Ctrl-C cancellation.
pub fn keyboard_read_line() -> Option<String> {
    let mut svc = lookup_service("keyboard");
    let mut retries = 0;
    while svc <= 0 && retries < 10000 {
        unsafe { syscall0(2); } // SYS_YIELD
        svc = lookup_service("keyboard");
        retries += 1;
    }
    if svc <= 0 {
        return None;
    }
    let mut line = String::new();
    let mut buf = [0u8; 1024];
    loop {
        // sendrecv: request characters and receive the reply in one syscall
        let n = ipc_sendrecv(svc, &[], &mut buf);
        if n <= 0 {
            // No characters buffered yet — yield and retry (blocking read).
            // This is the normal case when no key has been pressed.
            unsafe { syscall0(2); } // SYS_YIELD
            continue;
        }
        let chunk = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");
        for c in chunk.chars() {
            match c {
                '\n' | '\r' => {
                    vga_print("\n");
                    return Some(line);
                }
                '\x08' | '\x7f' => {
                    // Backspace: erase last character
                    if !line.is_empty() {
                        line.pop();
                        vga_print("\x08 \x08");
                    }
                }
                '\x03' => {
                    // Ctrl-C: cancel current line
                    vga_print("^C\n");
                    return Some(String::new());
                }
                c if c >= ' ' => {
                    // Printable character: echo to VGA and append
                    let mut b = [0u8; 4];
                    let s = c.encode_utf8(&mut b);
                    vga_print(s);
                    line.push(c);
                }
                _ => {} // ignore other control characters
            }
        }
    }
    Some(line)
}
