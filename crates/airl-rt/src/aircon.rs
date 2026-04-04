//! Container runtime (aircon) IPC stubs — create, start, stop, status, list.
//!
//! AIRLOS: IPC to the container service via message opcodes 0xC00–0xC08.
//! Linux:  All operations return Err("not available on Linux").

#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

use crate::value::{rt_str, rt_variant, RtValue};
#[cfg(target_os = "airlos")]
use crate::value::{rt_int, rt_list, rt_unit};

#[cfg(target_os = "airlos")]
fn ok_variant(inner: *mut RtValue) -> *mut RtValue {
    rt_variant("Ok".into(), inner)
}

fn err_variant(msg: &str) -> *mut RtValue {
    rt_variant("Err".into(), rt_str(msg.into()))
}

// ── AIRLOS IPC implementation ───────────────────────────────────────────────

#[cfg(target_os = "airlos")]
mod airlos_impl {
    use super::*;
    use crate::airlos::{ipc_sendrecv, lookup_service, write_u32_le, read_u32_le};
    use core::sync::atomic::{AtomicI32, Ordering::Relaxed};

    const CONTAINER_CREATE: u32 = 0xC00;
    const CONTAINER_START:  u32 = 0xC02;
    const CONTAINER_STOP:   u32 = 0xC04;
    const CONTAINER_STATUS: u32 = 0xC06;
    const CONTAINER_LIST:   u32 = 0xC08;

    const CONTAINER_OK:  u32 = 0xCF0;
    const CONTAINER_ERR: u32 = 0xCF1;

    /// Max bytes per string field in an IPC message.
    const MAX_STR_FIELD: usize = 200;

    /// Cached container service port.
    fn get_container_port() -> i32 {
        static CON_SVC: AtomicI32 = AtomicI32::new(0);
        let mut svc = CON_SVC.load(Relaxed);
        if svc <= 0 {
            for _ in 0..5000 {
                svc = lookup_service("container");
                if svc > 0 {
                    CON_SVC.store(svc, Relaxed);
                    return svc;
                }
                unsafe { crate::airlos::syscall0(2); } // SYS_YIELD
            }
            return 0;
        }
        svc
    }

    fn container_sendrecv(msg: &[u8], recv_buf: &mut [u8]) -> i32 {
        let svc = get_container_port();
        if svc <= 0 {
            return -1;
        }
        ipc_sendrecv(svc, msg, recv_buf)
    }

    /// Write a string field into msg at offset. Returns new offset.
    fn write_str_field(msg: &mut [u8], offset: usize, s: &str) -> usize {
        let bytes = s.as_bytes();
        let avail = msg.len().saturating_sub(offset + 4);
        let len = bytes.len().min(MAX_STR_FIELD).min(avail);
        write_u32_le(msg, offset, len as u32);
        msg[offset + 4..offset + 4 + len].copy_from_slice(&bytes[..len]);
        offset + 4 + len
    }

    /// Read a string field from buf at offset. Returns (string, new_offset).
    fn read_str_field(buf: &[u8], offset: usize) -> (String, usize) {
        if offset + 4 > buf.len() {
            return (String::new(), offset);
        }
        let len = read_u32_le(buf, offset) as usize;
        let end = (offset + 4 + len).min(buf.len());
        let s = core::str::from_utf8(&buf[offset + 4..end])
            .unwrap_or("")
            .to_string();
        (s, end)
    }

    /// Create a container. Returns container ID on success.
    /// Message: [opcode:4][image_len:4][image:...][mem_kb:4][cpu_ms:4]
    /// Response: [type:4][container_id:4]
    pub fn create(image: &str, mem_kb: i64, cpu_ms: i64) -> *mut RtValue {
        let mut msg = [0u8; 256];
        write_u32_le(&mut msg, 0, CONTAINER_CREATE);
        let off = write_str_field(&mut msg, 4, image);
        write_u32_le(&mut msg, off, mem_kb as u32);
        write_u32_le(&mut msg, off + 4, cpu_ms as u32);
        let msg_len = off + 8;

        let mut resp = [0u8; 256];
        let n = container_sendrecv(&msg[..msg_len], &mut resp);
        if n < 8 {
            return err_variant("container service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == CONTAINER_ERR {
            let (emsg, _) = read_str_field(&resp, 4);
            return err_variant(&emsg);
        }
        if resp_type != CONTAINER_OK {
            return err_variant("container: unexpected response");
        }
        let container_id = read_u32_le(&resp, 4) as i64;
        ok_variant(rt_int(container_id))
    }

    /// Start a container by ID.
    /// Message: [opcode:4][id:4]
    pub fn start(id: i64) -> *mut RtValue {
        let mut msg = [0u8; 8];
        write_u32_le(&mut msg, 0, CONTAINER_START);
        write_u32_le(&mut msg, 4, id as u32);
        let mut resp = [0u8; 256];
        let n = container_sendrecv(&msg, &mut resp);
        if n < 4 {
            return err_variant("container service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == CONTAINER_OK {
            ok_variant(rt_unit())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }

    /// Stop a container by ID.
    /// Message: [opcode:4][id:4]
    pub fn stop(id: i64) -> *mut RtValue {
        let mut msg = [0u8; 8];
        write_u32_le(&mut msg, 0, CONTAINER_STOP);
        write_u32_le(&mut msg, 4, id as u32);
        let mut resp = [0u8; 256];
        let n = container_sendrecv(&msg, &mut resp);
        if n < 4 {
            return err_variant("container service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == CONTAINER_OK {
            ok_variant(rt_unit())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }

    /// Query container status by ID. Returns status string.
    /// Message: [opcode:4][id:4]
    /// Response: [type:4][status_code:4]
    pub fn status(id: i64) -> *mut RtValue {
        let mut msg = [0u8; 8];
        write_u32_le(&mut msg, 0, CONTAINER_STATUS);
        write_u32_le(&mut msg, 4, id as u32);
        let mut resp = [0u8; 256];
        let n = container_sendrecv(&msg, &mut resp);
        if n < 8 {
            return err_variant("container service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == CONTAINER_ERR {
            let (emsg, _) = read_str_field(&resp, 4);
            return err_variant(&emsg);
        }
        if resp_type != CONTAINER_OK {
            return err_variant("container: unexpected response");
        }
        let status_code = read_u32_le(&resp, 4);
        let status_str = match status_code {
            0 => "empty",
            1 => "created",
            2 => "running",
            3 => "stopped",
            4 => "failed",
            _ => "unknown",
        };
        ok_variant(rt_str(status_str.into()))
    }

    /// List all containers. Returns a list of container IDs.
    /// Message: [opcode:4]
    /// Response: [type:4][count:4][id1:4][id2:4]...
    pub fn list() -> *mut RtValue {
        let mut msg = [0u8; 4];
        write_u32_le(&mut msg, 0, CONTAINER_LIST);
        let mut resp = [0u8; 256];
        let n = container_sendrecv(&msg, &mut resp);
        if n < 8 {
            return err_variant("container service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == CONTAINER_ERR {
            let (emsg, _) = read_str_field(&resp, 4);
            return err_variant(&emsg);
        }
        if resp_type != CONTAINER_OK {
            return err_variant("container: unexpected response");
        }
        let count = read_u32_le(&resp, 4) as usize;
        let mut ids = Vec::new();
        for i in 0..count {
            let off = 8 + i * 4;
            if off + 4 > n as usize { break; }
            ids.push(rt_int(read_u32_le(&resp, off) as i64));
        }
        ok_variant(rt_list(ids))
    }
}

// ── Extern "C" entry points ─────────────────────────────────────────────────

fn extract_str(v: *mut RtValue) -> String {
    if v.is_null() { return String::new(); }
    let rv = unsafe { &*v };
    match rv.data() {
        crate::value::RtData::Str(s) => s.clone(),
        _ => String::new(),
    }
}

fn extract_int(v: *mut RtValue) -> i64 {
    if v.is_null() { return 0; }
    let rv = unsafe { &*v };
    match rv.data() {
        crate::value::RtData::Int(n) => *n,
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn airl_aircon_create(
    image: *mut RtValue,
    mem_kb: *mut RtValue,
    cpu_ms: *mut RtValue,
) -> *mut RtValue {
    let img = extract_str(image);
    let mem = extract_int(mem_kb);
    let cpu = extract_int(cpu_ms);
    #[cfg(target_os = "airlos")]
    { airlos_impl::create(&img, mem, cpu) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (img, mem, cpu); err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_aircon_start(id: *mut RtValue) -> *mut RtValue {
    let i = extract_int(id);
    #[cfg(target_os = "airlos")]
    { airlos_impl::start(i) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = i; err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_aircon_stop(id: *mut RtValue) -> *mut RtValue {
    let i = extract_int(id);
    #[cfg(target_os = "airlos")]
    { airlos_impl::stop(i) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = i; err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_aircon_status(id: *mut RtValue) -> *mut RtValue {
    let i = extract_int(id);
    #[cfg(target_os = "airlos")]
    { airlos_impl::status(i) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = i; err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_aircon_list() -> *mut RtValue {
    #[cfg(target_os = "airlos")]
    { airlos_impl::list() }
    #[cfg(not(target_os = "airlos"))]
    { err_variant("not available on Linux") }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{rt_int, rt_str, RtData};

    fn unwrap_err_msg(v: *mut RtValue) -> String {
        let rv = unsafe { &*v };
        match rv.data() {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err", "expected Err variant, got {}", tag_name);
                let inner_rv = unsafe { &**inner };
                match inner_rv.data() {
                    RtData::Str(s) => s.clone(),
                    _ => panic!("expected Str inside Err"),
                }
            }
            _ => panic!("expected variant"),
        }
    }

    #[test]
    fn test_aircon_create_returns_err_on_linux() {
        let image = rt_str("test-image".into());
        let mem = rt_int(1024);
        let cpu = rt_int(500);
        let result = airl_aircon_create(image, mem, cpu);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_start_returns_err_on_linux() {
        let id = rt_int(1);
        let result = airl_aircon_start(id);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_stop_returns_err_on_linux() {
        let id = rt_int(1);
        let result = airl_aircon_stop(id);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_status_returns_err_on_linux() {
        let id = rt_int(1);
        let result = airl_aircon_status(id);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_list_returns_err_on_linux() {
        let result = airl_aircon_list();
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_create_null_args() {
        let result = airl_aircon_create(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_start_null_arg() {
        let result = airl_aircon_start(std::ptr::null_mut());
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_aircon_status_null_arg() {
        let result = airl_aircon_status(std::ptr::null_mut());
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }
}
