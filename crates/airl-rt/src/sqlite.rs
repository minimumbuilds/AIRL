//! SQLite stubs for airl-rt.
//! Handles are opaque i64 values (raw pointer casts on 64-bit).
//! All functions take/return *mut RtValue for interpreter compatibility.
//! Non-target_os-airlos only.

#[cfg(not(target_os = "airlos"))]
use crate::value::{rt_int, rt_nil, rt_str, RtData, RtValue};

// We link against the system libsqlite3 via libsqlite3-sys.
#[cfg(not(target_os = "airlos"))]
use libsqlite3_sys as ffi;

#[cfg(not(target_os = "airlos"))]
use std::ffi::{CStr, CString};
#[cfg(not(target_os = "airlos"))]
use std::os::raw::{c_char, c_int};

/// Open a SQLite database file.
/// Arg: path (*mut RtValue of Str).
/// Returns: i64 handle (non-zero on success, 0 on failure).
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_open(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => return rt_int(0),
        }
    };
    let c_path = match CString::new(path_str) {
        Ok(s) => s,
        Err(_) => return rt_int(0),
    };
    let mut db: *mut ffi::sqlite3 = std::ptr::null_mut();
    let rc = unsafe { ffi::sqlite3_open(c_path.as_ptr(), &mut db) };
    if rc == ffi::SQLITE_OK {
        rt_int(db as i64)
    } else {
        rt_int(0)
    }
}

/// Prepare a SQL statement.
/// Args: db handle (*mut RtValue of i64), sql (*mut RtValue of Str).
/// Returns: i64 stmt handle (non-zero on success, 0 on failure).
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_prepare(db: *mut RtValue, sql: *mut RtValue) -> *mut RtValue {
    let db_ptr = unsafe {
        match &(*db).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3,
            _ => return rt_int(0),
        }
    };
    let sql_str = unsafe {
        match &(*sql).data {
            RtData::Str(s) => s.clone(),
            _ => return rt_int(0),
        }
    };
    let c_sql = match CString::new(sql_str) {
        Ok(s) => s,
        Err(_) => return rt_int(0),
    };
    let mut stmt: *mut ffi::sqlite3_stmt = std::ptr::null_mut();
    let rc = unsafe {
        ffi::sqlite3_prepare_v2(db_ptr, c_sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut())
    };
    if rc == ffi::SQLITE_OK {
        rt_int(stmt as i64)
    } else {
        rt_int(0)
    }
}

/// Step a prepared statement.
/// Arg: stmt handle (*mut RtValue of i64).
/// Returns: i64 result code (100=SQLITE_ROW, 101=SQLITE_DONE).
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_step(stmt: *mut RtValue) -> *mut RtValue {
    let stmt_ptr = unsafe {
        match &(*stmt).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3_stmt,
            _ => return rt_int(0),
        }
    };
    let rc = unsafe { ffi::sqlite3_step(stmt_ptr) };
    rt_int(rc as i64)
}

/// Get the text value of column `col` (0-based) from the current row.
/// Args: stmt handle (*mut RtValue of i64), col (*mut RtValue of i64).
/// Returns: String value (empty if NULL).
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_column_text(stmt: *mut RtValue, col: *mut RtValue) -> *mut RtValue {
    let stmt_ptr = unsafe {
        match &(*stmt).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3_stmt,
            _ => return rt_str(String::new()),
        }
    };
    let col_idx = unsafe {
        match &(*col).data {
            RtData::Int(n) => *n as c_int,
            _ => return rt_str(String::new()),
        }
    };
    let text_ptr = unsafe { ffi::sqlite3_column_text(stmt_ptr, col_idx) };
    let s = if text_ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text_ptr as *const c_char) }
            .to_string_lossy()
            .into_owned()
    };
    rt_str(s)
}

/// Get the integer value of column `col` (0-based) from the current row.
/// Args: stmt handle (*mut RtValue of i64), col (*mut RtValue of i64).
/// Returns: i64 value.
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_column_int(stmt: *mut RtValue, col: *mut RtValue) -> *mut RtValue {
    let stmt_ptr = unsafe {
        match &(*stmt).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3_stmt,
            _ => return rt_int(0),
        }
    };
    let col_idx = unsafe {
        match &(*col).data {
            RtData::Int(n) => *n as c_int,
            _ => return rt_int(0),
        }
    };
    let val = unsafe { ffi::sqlite3_column_int64(stmt_ptr, col_idx) };
    rt_int(val)
}

/// Get the number of columns in the result set of the prepared statement.
/// Arg: stmt handle (*mut RtValue of i64).
/// Returns: i64 column count.
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_column_count(stmt: *mut RtValue) -> *mut RtValue {
    let stmt_ptr = unsafe {
        match &(*stmt).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3_stmt,
            _ => return rt_int(0),
        }
    };
    let count = unsafe { ffi::sqlite3_column_count(stmt_ptr) };
    rt_int(count as i64)
}

/// Finalize (destroy) a prepared statement.
/// Arg: stmt handle (*mut RtValue of i64).
/// Returns: Nil (SQLITE_OK on success, ignored).
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_finalize(stmt: *mut RtValue) -> *mut RtValue {
    let stmt_ptr = unsafe {
        match &(*stmt).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3_stmt,
            _ => return rt_nil(),
        }
    };
    unsafe { ffi::sqlite3_finalize(stmt_ptr) };
    rt_nil()
}

/// Close a database connection.
/// Arg: db handle (*mut RtValue of i64).
/// Returns: Nil.
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_close(db: *mut RtValue) -> *mut RtValue {
    let db_ptr = unsafe {
        match &(*db).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3,
            _ => return rt_nil(),
        }
    };
    unsafe { ffi::sqlite3_close(db_ptr) };
    rt_nil()
}

/// Get the last error message for the database connection.
/// Arg: db handle (*mut RtValue of i64).
/// Returns: String error message.
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_sqlite_errmsg(db: *mut RtValue) -> *mut RtValue {
    let db_ptr = unsafe {
        match &(*db).data {
            RtData::Int(n) => *n as *mut ffi::sqlite3,
            _ => return rt_str(String::new()),
        }
    };
    let msg_ptr = unsafe { ffi::sqlite3_errmsg(db_ptr) };
    let s = if msg_ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(msg_ptr) }
            .to_string_lossy()
            .into_owned()
    };
    rt_str(s)
}
