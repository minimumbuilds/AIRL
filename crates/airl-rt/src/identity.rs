//! Identity IPC stubs — whoami, id, authenticate, switch-user, elevate,
//! create-user, delete-user, set-password.
//!
//! AIRLOS: IPC to the identity service via message opcodes 0xA00–0xA26.
//! Linux:  libc wrappers for whoami/id; others return Err("not available on Linux").

#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

#[cfg(not(target_os = "airlos"))]
use std::collections::HashMap;
#[cfg(target_os = "airlos")]
use alloc::collections::BTreeMap as HashMap;

use crate::value::{rt_int, rt_map, rt_str, rt_variant, RtValue};

#[cfg(target_os = "airlos")]
use crate::value::rt_nil;

fn ok_variant(inner: *mut RtValue) -> *mut RtValue {
    rt_variant("Ok".into(), inner)
}

fn err_variant(msg: &str) -> *mut RtValue {
    rt_variant("Err".into(), rt_str(msg.into()))
}

// ── AIRLOS IPC implementation ────────���──────────────────────────────────────

#[cfg(target_os = "airlos")]
mod airlos_impl {
    use super::*;
    use crate::airlos::{ipc_sendrecv, lookup_service, write_u32_le, read_u32_le};
    use core::sync::atomic::{AtomicI32, Ordering::Relaxed};

    const IDENT_WHOAMI:        u32 = 0xA00;
    const IDENT_ID:            u32 = 0xA08;
    const IDENT_AUTHENTICATE:  u32 = 0xA10;
    const IDENT_ELEVATE:       u32 = 0xA12;
    const IDENT_SWITCH_USER:   u32 = 0xA14;
    const IDENT_CREATE_USER:   u32 = 0xA20;
    const IDENT_DELETE_USER:   u32 = 0xA22;
    const IDENT_SET_PASSWORD:  u32 = 0xA26;

    const IDENT_OK:  u32 = 0xAF0;
    const IDENT_ERR: u32 = 0xAF1;

    /// Max bytes per string field in an IPC message.
    /// With a 256-byte IPC buffer, this allows up to 3 string fields + fixed headers.
    /// (4 opcode + 3×(4 len + 72 data) + 8 uids = 240 < 256)
    const MAX_STR_FIELD: usize = 72;

    /// Cached identity service port.
    fn get_identity_port() -> i32 {
        static IDENT_SVC: AtomicI32 = AtomicI32::new(0);
        let mut svc = IDENT_SVC.load(Relaxed);
        if svc <= 0 {
            for _ in 0..5000 {
                svc = lookup_service("identity");
                if svc > 0 {
                    IDENT_SVC.store(svc, Relaxed);
                    return svc;
                }
                unsafe { crate::airlos::syscall0(2); } // SYS_YIELD
            }
            return 0;
        }
        svc
    }

    fn identity_sendrecv(msg: &[u8], recv_buf: &mut [u8]) -> i32 {
        let svc = get_identity_port();
        if svc <= 0 {
            return -1;
        }
        ipc_sendrecv(svc, msg, recv_buf)
    }

    /// Write a string field into msg at offset. Returns new offset.
    /// Truncates to MAX_STR_FIELD bytes to stay within the 256-byte IPC buffer.
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

    pub fn whoami() -> *mut RtValue {
        let mut msg = [0u8; 4];
        write_u32_le(&mut msg, 0, IDENT_WHOAMI);
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg, &mut resp);
        if n < 16 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_ERR {
            let (emsg, _) = read_str_field(&resp, 4);
            return err_variant(&emsg);
        }
        if resp_type != IDENT_OK {
            return err_variant("identity: unexpected response");
        }
        // Response: [type:4][uid:4][gid:4][name_len:4][name:...][group_len:4][group:...]
        let uid = read_u32_le(&resp, 4) as i64;
        let gid = read_u32_le(&resp, 8) as i64;
        let (name, off) = read_str_field(&resp, 12);
        let (group, _) = read_str_field(&resp, off);
        let mut m: HashMap<String, *mut RtValue> = HashMap::new();
        m.insert("uid".into(), rt_int(uid));
        m.insert("name".into(), rt_str(name));
        m.insert("gid".into(), rt_int(gid));
        m.insert("group".into(), rt_str(group));
        ok_variant(rt_map(m))
    }

    pub fn id() -> *mut RtValue {
        let mut msg = [0u8; 4];
        write_u32_le(&mut msg, 0, IDENT_ID);
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg, &mut resp);
        if n < 16 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_ERR {
            let (emsg, _) = read_str_field(&resp, 4);
            return err_variant(&emsg);
        }
        if resp_type != IDENT_OK {
            return err_variant("identity: unexpected response");
        }
        // Response: [type:4][uid:4][gid:4][group_count:4][ [gid:4][name_len:4][name:...]... ]
        let uid = read_u32_le(&resp, 4) as i64;
        let gid = read_u32_le(&resp, 8) as i64;
        let group_count = read_u32_le(&resp, 12) as usize;
        let mut off = 16;
        let mut groups = Vec::new();
        for _ in 0..group_count {
            if off + 4 > n as usize { break; }
            let g_gid = read_u32_le(&resp, off) as i64;
            let (g_name, new_off) = read_str_field(&resp, off + 4);
            off = new_off;
            let mut gm: HashMap<String, *mut RtValue> = HashMap::new();
            gm.insert("gid".into(), rt_int(g_gid));
            gm.insert("name".into(), rt_str(g_name));
            groups.push(rt_map(gm));
        }
        let mut m: HashMap<String, *mut RtValue> = HashMap::new();
        m.insert("uid".into(), rt_int(uid));
        m.insert("gid".into(), rt_int(gid));
        m.insert("groups".into(), crate::value::rt_list(groups));
        ok_variant(rt_map(m))
    }

    fn simple_str_request(opcode: u32, fields: &[&str]) -> *mut RtValue {
        let mut msg = [0u8; 256];
        write_u32_le(&mut msg, 0, opcode);
        let mut off = 4;
        for s in fields {
            off = write_str_field(&mut msg, off, s);
        }
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg[..off], &mut resp);
        if n < 4 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_OK {
            ok_variant(rt_nil())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }

    pub fn authenticate(user: &str, pass: &str) -> *mut RtValue {
        simple_str_request(IDENT_AUTHENTICATE, &[user, pass])
    }

    pub fn switch_user(user: &str, pass: &str) -> *mut RtValue {
        simple_str_request(IDENT_SWITCH_USER, &[user, pass])
    }

    pub fn elevate(user: &str, pass: &str) -> *mut RtValue {
        simple_str_request(IDENT_ELEVATE, &[user, pass])
    }

    pub fn create_user(name: &str, uid: u32, gid: u32, home: &str, shell: &str) -> *mut RtValue {
        let mut msg = [0u8; 256];
        write_u32_le(&mut msg, 0, IDENT_CREATE_USER);
        let mut off = write_str_field(&mut msg, 4, name);
        write_u32_le(&mut msg, off, uid);
        write_u32_le(&mut msg, off + 4, gid);
        off = write_str_field(&mut msg, off + 8, home);
        off = write_str_field(&mut msg, off, shell);
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg[..off], &mut resp);
        if n < 4 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_OK {
            ok_variant(rt_nil())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }

    pub fn delete_user(uid: u32) -> *mut RtValue {
        let mut msg = [0u8; 8];
        write_u32_le(&mut msg, 0, IDENT_DELETE_USER);
        write_u32_le(&mut msg, 4, uid);
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg, &mut resp);
        if n < 4 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_OK {
            ok_variant(rt_nil())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }

    pub fn set_password(uid: u32, old_pass: &str, new_pass: &str) -> *mut RtValue {
        let mut msg = [0u8; 256];
        write_u32_le(&mut msg, 0, IDENT_SET_PASSWORD);
        write_u32_le(&mut msg, 4, uid);
        let mut off = write_str_field(&mut msg, 8, old_pass);
        off = write_str_field(&mut msg, off, new_pass);
        let mut resp = [0u8; 256];
        let n = identity_sendrecv(&msg[..off], &mut resp);
        if n < 4 {
            return err_variant("identity service unavailable");
        }
        let resp_type = read_u32_le(&resp, 0);
        if resp_type == IDENT_OK {
            ok_variant(rt_nil())
        } else {
            let (emsg, _) = read_str_field(&resp, 4);
            err_variant(&emsg)
        }
    }
}

// ── Linux libc implementation ───────────���────────────────────────────────��──

#[cfg(not(target_os = "airlos"))]
mod linux_impl {
    use super::*;

    pub fn whoami() -> *mut RtValue {
        unsafe {
            let uid = libc::getuid() as i64;

            // Use getpwuid_r (thread-safe) instead of getpwuid
            let mut pwd: libc::passwd = std::mem::zeroed();
            let mut pwd_buf = [0u8; 1024];
            let mut pwd_result: *mut libc::passwd = std::ptr::null_mut();
            let rc = libc::getpwuid_r(
                uid as libc::uid_t,
                &mut pwd,
                pwd_buf.as_mut_ptr() as *mut libc::c_char,
                pwd_buf.len(),
                &mut pwd_result,
            );
            if rc != 0 || pwd_result.is_null() {
                return err_variant("getpwuid_r failed");
            }
            let name = std::ffi::CStr::from_ptr(pwd.pw_name).to_string_lossy().into_owned();
            let gid = pwd.pw_gid as i64;

            // Use getgrgid_r (thread-safe) instead of getgrgid
            let mut grp: libc::group = std::mem::zeroed();
            let mut grp_buf = [0u8; 1024];
            let mut grp_result: *mut libc::group = std::ptr::null_mut();
            let rc = libc::getgrgid_r(
                gid as libc::gid_t,
                &mut grp,
                grp_buf.as_mut_ptr() as *mut libc::c_char,
                grp_buf.len(),
                &mut grp_result,
            );
            let group = if rc != 0 || grp_result.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(grp.gr_name).to_string_lossy().into_owned()
            };

            let mut m: HashMap<String, *mut RtValue> = HashMap::new();
            m.insert("uid".into(), rt_int(uid));
            m.insert("name".into(), rt_str(name));
            m.insert("gid".into(), rt_int(gid));
            m.insert("group".into(), rt_str(group));
            ok_variant(rt_map(m))
        }
    }

    pub fn id() -> *mut RtValue {
        unsafe {
            let uid = libc::getuid() as i64;
            let gid = libc::getgid() as i64;
            let mut buf = [0 as libc::gid_t; 64];
            let ngroups = libc::getgroups(64, buf.as_mut_ptr());
            let mut groups = Vec::new();
            if ngroups > 0 {
                for i in 0..ngroups as usize {
                    let g_gid = buf[i] as i64;
                    // Use getgrgid_r (thread-safe)
                    let mut grp: libc::group = std::mem::zeroed();
                    let mut grp_buf = [0u8; 1024];
                    let mut grp_result: *mut libc::group = std::ptr::null_mut();
                    let rc = libc::getgrgid_r(
                        buf[i],
                        &mut grp,
                        grp_buf.as_mut_ptr() as *mut libc::c_char,
                        grp_buf.len(),
                        &mut grp_result,
                    );
                    let g_name = if rc != 0 || grp_result.is_null() {
                        String::new()
                    } else {
                        std::ffi::CStr::from_ptr(grp.gr_name).to_string_lossy().into_owned()
                    };
                    let mut gm: HashMap<String, *mut RtValue> = HashMap::new();
                    gm.insert("gid".into(), rt_int(g_gid));
                    gm.insert("name".into(), rt_str(g_name));
                    groups.push(rt_map(gm));
                }
            }
            let mut m: HashMap<String, *mut RtValue> = HashMap::new();
            m.insert("uid".into(), rt_int(uid));
            m.insert("gid".into(), rt_int(gid));
            m.insert("groups".into(), crate::value::rt_list(groups));
            ok_variant(rt_map(m))
        }
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
pub extern "C" fn airl_whoami() -> *mut RtValue {
    #[cfg(target_os = "airlos")]
    { airlos_impl::whoami() }
    #[cfg(not(target_os = "airlos"))]
    { linux_impl::whoami() }
}

#[no_mangle]
pub extern "C" fn airl_id() -> *mut RtValue {
    #[cfg(target_os = "airlos")]
    { airlos_impl::id() }
    #[cfg(not(target_os = "airlos"))]
    { linux_impl::id() }
}

#[no_mangle]
pub extern "C" fn airl_authenticate(user: *mut RtValue, pass: *mut RtValue) -> *mut RtValue {
    let u = extract_str(user);
    let p = extract_str(pass);
    #[cfg(target_os = "airlos")]
    { airlos_impl::authenticate(&u, &p) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (u, p); err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_switch_user(user: *mut RtValue, pass: *mut RtValue) -> *mut RtValue {
    let u = extract_str(user);
    let p = extract_str(pass);
    #[cfg(target_os = "airlos")]
    { airlos_impl::switch_user(&u, &p) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (u, p); err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_elevate(user: *mut RtValue, pass: *mut RtValue) -> *mut RtValue {
    let u = extract_str(user);
    let p = extract_str(pass);
    #[cfg(target_os = "airlos")]
    { airlos_impl::elevate(&u, &p) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (u, p); err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_create_user(
    name: *mut RtValue,
    uid: *mut RtValue,
    gid: *mut RtValue,
    home: *mut RtValue,
    shell: *mut RtValue,
) -> *mut RtValue {
    let n = extract_str(name);
    let u = extract_int(uid) as u32;
    let g = extract_int(gid) as u32;
    let h = extract_str(home);
    let s = extract_str(shell);
    #[cfg(target_os = "airlos")]
    { airlos_impl::create_user(&n, u, g, &h, &s) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (n, u, g, h, s); err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_delete_user(uid: *mut RtValue) -> *mut RtValue {
    let u = extract_int(uid) as u32;
    #[cfg(target_os = "airlos")]
    { airlos_impl::delete_user(u) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = u; err_variant("not available on Linux") }
}

#[no_mangle]
pub extern "C" fn airl_set_password(
    uid: *mut RtValue,
    old_pass: *mut RtValue,
    new_pass: *mut RtValue,
) -> *mut RtValue {
    let u = extract_int(uid) as u32;
    let o = extract_str(old_pass);
    let n = extract_str(new_pass);
    #[cfg(target_os = "airlos")]
    { airlos_impl::set_password(u, &o, &n) }
    #[cfg(not(target_os = "airlos"))]
    { let _ = (u, o, n); err_variant("not available on Linux") }
}

// ── Tests ────────────────────────────────────��──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::RtData;

    fn unwrap_ok(v: *mut RtValue) -> *mut RtValue {
        let rv = unsafe { &*v };
        match rv.data() {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Ok", "expected Ok variant, got {}", tag_name);
                *inner
            }
            _ => panic!("expected Ok/Err variant"),
        }
    }

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

    fn map_get_int(map: *mut RtValue, key: &str) -> i64 {
        let rv = unsafe { &*map };
        match rv.data() {
            RtData::Map(m) => {
                let val = m.get(key).unwrap_or_else(|| panic!("missing key: {}", key));
                let inner = unsafe { &**val };
                inner.try_as_int().unwrap_or_else(|| panic!("expected int for key {}", key))
            }
            _ => panic!("expected map"),
        }
    }

    fn map_get_str(map: *mut RtValue, key: &str) -> String {
        let rv = unsafe { &*map };
        match rv.data() {
            RtData::Map(m) => {
                let val = m.get(key).unwrap_or_else(|| panic!("missing key: {}", key));
                let inner = unsafe { &**val };
                inner.try_as_str().unwrap_or_else(|| panic!("expected str for key {}", key)).to_string()
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn test_whoami_returns_ok_map() {
        let result = airl_whoami();
        let inner = unwrap_ok(result);
        let uid = map_get_int(inner, "uid");
        assert!(uid >= 0, "uid should be non-negative");
        let name = map_get_str(inner, "name");
        assert!(!name.is_empty(), "name should not be empty");
        // gid and group should be present
        let gid = map_get_int(inner, "gid");
        assert!(gid >= 0, "gid should be non-negative");
        let _group = map_get_str(inner, "group");
    }

    #[test]
    fn test_id_returns_ok_map_with_groups() {
        let result = airl_id();
        let inner = unwrap_ok(result);
        let uid = map_get_int(inner, "uid");
        assert!(uid >= 0);
        let gid = map_get_int(inner, "gid");
        assert!(gid >= 0);
        // groups should be a list
        let rv = unsafe { &*inner };
        match rv.data() {
            RtData::Map(m) => {
                let groups_val = m.get("groups").expect("missing groups key");
                let groups_rv = unsafe { &**groups_val };
                match groups_rv.data() {
                    RtData::List { .. } => {}
                    _ => panic!("groups should be a list"),
                }
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn test_authenticate_returns_err_on_linux() {
        let user = rt_str("testuser".into());
        let pass = rt_str("testpass".into());
        let result = airl_authenticate(user, pass);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_switch_user_returns_err_on_linux() {
        let user = rt_str("testuser".into());
        let pass = rt_str("testpass".into());
        let result = airl_switch_user(user, pass);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_elevate_returns_err_on_linux() {
        let user = rt_str("testuser".into());
        let pass = rt_str("testpass".into());
        let result = airl_elevate(user, pass);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_create_user_returns_err_on_linux() {
        let name = rt_str("newuser".into());
        let uid = rt_int(1000);
        let gid = rt_int(1000);
        let home = rt_str("/home/newuser".into());
        let shell = rt_str("/bin/sh".into());
        let result = airl_create_user(name, uid, gid, home, shell);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_delete_user_returns_err_on_linux() {
        let uid = rt_int(1000);
        let result = airl_delete_user(uid);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_set_password_returns_err_on_linux() {
        let uid = rt_int(1000);
        let old = rt_str("old".into());
        let new = rt_str("new".into());
        let result = airl_set_password(uid, old, new);
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }

    #[test]
    fn test_whoami_null_safety() {
        // whoami takes no args, just verify it doesn't panic
        let result = airl_whoami();
        let _ = unwrap_ok(result);
    }

    #[test]
    fn test_authenticate_null_args() {
        let result = airl_authenticate(std::ptr::null_mut(), std::ptr::null_mut());
        let msg = unwrap_err_msg(result);
        assert_eq!(msg, "not available on Linux");
    }
}
