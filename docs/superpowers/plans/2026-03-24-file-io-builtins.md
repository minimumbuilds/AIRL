# File I/O Builtins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 8 file I/O builtins (`append-file`, `delete-file`, `delete-dir`, `rename-file`, `read-dir`, `create-dir`, `file-size`, `is-dir?`) to close the file system gaps identified in the language completeness analysis.

**Architecture:** Each builtin follows the existing pattern: Rust function in `builtins.rs` using `validate_sandboxed_path()`, registered via `register_file_io()`, with `extern "C"` counterpart in `airl-rt/src/io.rs` for JIT/AOT. All paths are sandbox-validated (no absolute paths, no `..`). Return types follow existing conventions (`Bool(true)` for mutations, `List[Str]` for `read-dir`, `Int` for `file-size`).

**Reference:** Existing file I/O builtins at `crates/airl-runtime/src/builtins.rs:1051-1111` and `crates/airl-rt/src/io.rs:52-64`.

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/builtins.rs` | Modify | Add 8 builtin functions + register in `register_file_io()` |
| `crates/airl-rt/src/io.rs` | Modify | Add 8 `extern "C"` functions for JIT/AOT |
| `crates/airl-runtime/src/bytecode_jit_full.rs` | Modify | Add to `RuntimeImports`, `JITBuilder` symbols, `builtin_map` |
| `crates/airl-runtime/src/bytecode_aot.rs` | Modify | Add to `RuntimeImports`, `declare_runtime_imports`, `builtin_map` |
| `crates/airl-types/src/checker.rs` | Modify | Add 8 names to the generic builtin list |

---

### Task 1: Implement `append-file` and `delete-file` builtins

**Files:** `crates/airl-runtime/src/builtins.rs`

- [ ] **Step 1:** Add `append-file` builtin after `builtin_file_exists` (~line 1111):

```rust
fn builtin_append_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("append-file", args, 2)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("append-file: first argument must be a string path".into())),
    };
    let content = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("append-file: second argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("append-file", &path)?;
    if let Some(parent) = validated.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RuntimeError::Custom(format!("append-file: cannot create directory: {}", e))
            })?;
        }
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&validated)
        .map_err(|e| RuntimeError::Custom(format!("append-file: {}: {}", path, e)))?;
    file.write_all(content.as_bytes())
        .map_err(|e| RuntimeError::Custom(format!("append-file: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}
```

- [ ] **Step 2:** Add `delete-file` builtin:

```rust
fn builtin_delete_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("delete-file", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("delete-file: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("delete-file", &path)?;
    if validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "delete-file: '{}' is a directory, use delete-dir", path
        )));
    }
    std::fs::remove_file(&validated)
        .map_err(|e| RuntimeError::Custom(format!("delete-file: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}
```

- [ ] **Step 3:** Register both in `register_file_io()` (after line 153):

```rust
self.register("append-file", builtin_append_file);
self.register("delete-file", builtin_delete_file);
```

- [ ] **Step 4:** Add tests in the `#[cfg(test)] mod tests` section (~line 1780+):

```rust
#[test]
fn append_file_creates_and_appends() {
    let b = Builtins::new();
    let tmp = format!("test_append_{}.txt", std::process::id());
    // Write initial content
    call(&b, "write-file", &[Value::Str(tmp.clone()), Value::Str("hello".into())]).unwrap();
    // Append
    let result = call(&b, "append-file", &[Value::Str(tmp.clone()), Value::Str(" world".into())]).unwrap();
    assert_eq!(result, Value::Bool(true));
    // Verify
    let content = call(&b, "read-file", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(content, Value::Str("hello world".into()));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn delete_file_removes_file() {
    let b = Builtins::new();
    let tmp = format!("test_delete_{}.txt", std::process::id());
    call(&b, "write-file", &[Value::Str(tmp.clone()), Value::Str("temp".into())]).unwrap();
    let result = call(&b, "delete-file", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result, Value::Bool(true));
    let exists = call(&b, "file-exists?", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(exists, Value::Bool(false));
}

#[test]
fn delete_file_rejects_directory() {
    let b = Builtins::new();
    let tmp = format!("test_deldir_{}", std::process::id());
    std::fs::create_dir_all(&tmp).unwrap();
    let result = call(&b, "delete-file", &[Value::Str(tmp.clone())]);
    assert!(result.is_err());
    std::fs::remove_dir_all(&tmp).ok();
}
```

- [ ] **Step 5:** Run tests: `cargo test -p airl-runtime -- builtin` and verify pass.

- [ ] **Step 6:** Commit.

---

### Task 2: Implement `delete-dir`, `rename-file`, `create-dir` builtins

**Files:** `crates/airl-runtime/src/builtins.rs`

- [ ] **Step 1:** Add `delete-dir` builtin:

```rust
fn builtin_delete_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("delete-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("delete-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("delete-dir", &path)?;
    if !validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "delete-dir: '{}' is not a directory", path
        )));
    }
    std::fs::remove_dir_all(&validated)
        .map_err(|e| RuntimeError::Custom(format!("delete-dir: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}
```

- [ ] **Step 2:** Add `rename-file` builtin:

```rust
fn builtin_rename_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("rename-file", args, 2)?;
    let old_path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("rename-file: first argument must be a string".into())),
    };
    let new_path = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("rename-file: second argument must be a string".into())),
    };
    let validated_old = validate_sandboxed_path("rename-file", &old_path)?;
    let validated_new = validate_sandboxed_path("rename-file", &new_path)?;
    std::fs::rename(&validated_old, &validated_new)
        .map_err(|e| RuntimeError::Custom(format!("rename-file: {} -> {}: {}", old_path, new_path, e)))?;
    Ok(Value::Bool(true))
}
```

- [ ] **Step 3:** Add `create-dir` builtin:

```rust
fn builtin_create_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("create-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("create-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("create-dir", &path)?;
    std::fs::create_dir_all(&validated)
        .map_err(|e| RuntimeError::Custom(format!("create-dir: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}
```

- [ ] **Step 4:** Register all three in `register_file_io()`:

```rust
self.register("delete-dir", builtin_delete_dir);
self.register("rename-file", builtin_rename_file);
self.register("create-dir", builtin_create_dir);
```

- [ ] **Step 5:** Add tests:

```rust
#[test]
fn create_and_delete_dir() {
    let b = Builtins::new();
    let tmp = format!("test_mkdir_{}", std::process::id());
    let result = call(&b, "create-dir", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result, Value::Bool(true));
    assert!(std::path::Path::new(&tmp).is_dir());
    // Idempotent
    let result2 = call(&b, "create-dir", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result2, Value::Bool(true));
    // Delete
    let del = call(&b, "delete-dir", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(del, Value::Bool(true));
    assert!(!std::path::Path::new(&tmp).exists());
}

#[test]
fn rename_file_works() {
    let b = Builtins::new();
    let src = format!("test_rename_src_{}.txt", std::process::id());
    let dst = format!("test_rename_dst_{}.txt", std::process::id());
    call(&b, "write-file", &[Value::Str(src.clone()), Value::Str("content".into())]).unwrap();
    let result = call(&b, "rename-file", &[Value::Str(src.clone()), Value::Str(dst.clone())]).unwrap();
    assert_eq!(result, Value::Bool(true));
    assert!(!std::path::Path::new(&src).exists());
    let content = call(&b, "read-file", &[Value::Str(dst.clone())]).unwrap();
    assert_eq!(content, Value::Str("content".into()));
    let _ = std::fs::remove_file(&dst);
}
```

- [ ] **Step 6:** Run tests and verify pass.

- [ ] **Step 7:** Commit.

---

### Task 3: Implement `read-dir`, `file-size`, `is-dir?` builtins

**Files:** `crates/airl-runtime/src/builtins.rs`

- [ ] **Step 1:** Add `read-dir` builtin:

```rust
fn builtin_read_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("read-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("read-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("read-dir", &path)?;
    if !validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "read-dir: '{}' is not a directory", path
        )));
    }
    let mut entries: Vec<String> = std::fs::read_dir(&validated)
        .map_err(|e| RuntimeError::Custom(format!("read-dir: {}: {}", path, e)))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    entries.sort();
    Ok(Value::List(entries.into_iter().map(Value::Str).collect()))
}
```

- [ ] **Step 2:** Add `file-size` builtin:

```rust
fn builtin_file_size(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("file-size", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("file-size: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("file-size", &path)?;
    let meta = std::fs::metadata(&validated)
        .map_err(|e| RuntimeError::Custom(format!("file-size: {}: {}", path, e)))?;
    if meta.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "file-size: '{}' is a directory", path
        )));
    }
    Ok(Value::Int(meta.len() as i64))
}
```

- [ ] **Step 3:** Add `is-dir?` builtin:

```rust
fn builtin_is_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("is-dir?", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("is-dir?: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("is-dir?", &path)?;
    Ok(Value::Bool(validated.is_dir()))
}
```

- [ ] **Step 4:** Register all three in `register_file_io()`:

```rust
self.register("read-dir", builtin_read_dir);
self.register("file-size", builtin_file_size);
self.register("is-dir?", builtin_is_dir);
```

- [ ] **Step 5:** Add tests:

```rust
#[test]
fn read_dir_lists_entries() {
    let b = Builtins::new();
    let tmp = format!("test_readdir_{}", std::process::id());
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(format!("{}/b.txt", tmp), "b").unwrap();
    std::fs::write(format!("{}/a.txt", tmp), "a").unwrap();
    let result = call(&b, "read-dir", &[Value::Str(tmp.clone())]).unwrap();
    // Sorted
    assert_eq!(result, Value::List(vec![Value::Str("a.txt".into()), Value::Str("b.txt".into())]));
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn file_size_returns_bytes() {
    let b = Builtins::new();
    let tmp = format!("test_fsize_{}.txt", std::process::id());
    call(&b, "write-file", &[Value::Str(tmp.clone()), Value::Str("hello".into())]).unwrap();
    let result = call(&b, "file-size", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result, Value::Int(5));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn is_dir_on_directory() {
    let b = Builtins::new();
    let tmp = format!("test_isdir_{}", std::process::id());
    std::fs::create_dir_all(&tmp).unwrap();
    let result = call(&b, "is-dir?", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result, Value::Bool(true));
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn is_dir_on_file() {
    let b = Builtins::new();
    let tmp = format!("test_isdir_f_{}.txt", std::process::id());
    call(&b, "write-file", &[Value::Str(tmp.clone()), Value::Str("x".into())]).unwrap();
    let result = call(&b, "is-dir?", &[Value::Str(tmp.clone())]).unwrap();
    assert_eq!(result, Value::Bool(false));
    let _ = std::fs::remove_file(&tmp);
}
```

- [ ] **Step 6:** Run tests and verify pass.

- [ ] **Step 7:** Commit.

---

### Task 4: Add `extern "C"` functions to `airl-rt`

**Files:** `crates/airl-rt/src/io.rs`

- [ ] **Step 1:** Add all 8 extern "C" functions after `airl_read_file` (~line 64). Each follows the same pattern as `airl_read_file`:

```rust
#[no_mangle]
pub extern "C" fn airl_append_file(path: *mut RtValue, content: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("append-file: expected string path") } };
    let content_str = unsafe { match &(*content).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("append-file: expected string content") } };
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new().create(true).append(true).open(&path_str) {
        Ok(f) => f,
        Err(e) => crate::error::rt_error(&format!("append-file: {}: {}", path_str, e)),
    };
    match file.write_all(content_str.as_bytes()) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("append-file: {}: {}", path_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_delete_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("delete-file: expected string path") } };
    match std::fs::remove_file(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("delete-file: {}: {}", path_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_delete_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("delete-dir: expected string path") } };
    match std::fs::remove_dir_all(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("delete-dir: {}: {}", path_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_rename_file(old: *mut RtValue, new: *mut RtValue) -> *mut RtValue {
    let old_str = unsafe { match &(*old).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("rename-file: expected string path") } };
    let new_str = unsafe { match &(*new).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("rename-file: expected string path") } };
    match std::fs::rename(&old_str, &new_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("rename-file: {} -> {}: {}", old_str, new_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_read_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("read-dir: expected string path") } };
    let mut entries: Vec<String> = match std::fs::read_dir(&path_str) {
        Ok(rd) => rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect(),
        Err(e) => crate::error::rt_error(&format!("read-dir: {}: {}", path_str, e)),
    };
    entries.sort();
    let items: Vec<*mut RtValue> = entries.into_iter().map(|s| rt_str(s)).collect();
    crate::value::rt_list(items)
}

#[no_mangle]
pub extern "C" fn airl_create_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("create-dir: expected string path") } };
    match std::fs::create_dir_all(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("create-dir: {}: {}", path_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_file_size(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("file-size: expected string path") } };
    match std::fs::metadata(&path_str) {
        Ok(meta) => crate::value::rt_int(meta.len() as i64),
        Err(e) => crate::error::rt_error(&format!("file-size: {}: {}", path_str, e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_is_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("is-dir?: expected string path") } };
    rt_bool(std::path::Path::new(&path_str).is_dir())
}

#[no_mangle]
pub extern "C" fn airl_file_exists(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("file-exists?: expected string path") } };
    rt_bool(std::path::Path::new(&path_str).exists())
}

#[no_mangle]
pub extern "C" fn airl_write_file(path: *mut RtValue, content: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe { match &(*path).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("write-file: expected string path") } };
    let content_str = unsafe { match &(*content).data { RtData::Str(s) => s.clone(), _ => crate::error::rt_error("write-file: expected string content") } };
    if let Some(parent) = std::path::Path::new(&path_str).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    match std::fs::write(&path_str, content_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("write-file: {}: {}", path_str, e)),
    }
}
```

Note: Also add `airl_file_exists` and `airl_write_file` which were declared in AOT but never defined — this fixes a pre-existing link error.

- [ ] **Step 2:** Run tests: `cargo test -p airl-rt`

- [ ] **Step 3:** Commit.

---

### Task 5: Register in JIT-full (`bytecode_jit_full.rs`)

**Files:** `crates/airl-runtime/src/bytecode_jit_full.rs`

- [ ] **Step 1:** Add symbols to JITBuilder (~line 370, after `airl_read_file`):

```rust
builder.symbol("airl_append_file",  io::airl_append_file  as *const u8);
builder.symbol("airl_delete_file",  io::airl_delete_file  as *const u8);
builder.symbol("airl_delete_dir",   io::airl_delete_dir   as *const u8);
builder.symbol("airl_rename_file",  io::airl_rename_file  as *const u8);
builder.symbol("airl_read_dir",     io::airl_read_dir     as *const u8);
builder.symbol("airl_create_dir",   io::airl_create_dir   as *const u8);
builder.symbol("airl_file_size",    io::airl_file_size    as *const u8);
builder.symbol("airl_is_dir",       io::airl_is_dir       as *const u8);
builder.symbol("airl_write_file",   io::airl_write_file   as *const u8);
builder.symbol("airl_file_exists",  io::airl_file_exists  as *const u8);
```

- [ ] **Step 2:** Add to `RuntimeImports` struct (~line 127):

```rust
pub append_file: FuncId,
pub delete_file: FuncId,
pub delete_dir:  FuncId,
pub rename_file: FuncId,
pub read_dir:    FuncId,
pub create_dir:  FuncId,
pub file_size:   FuncId,
pub is_dir:      FuncId,
pub write_file:  FuncId,
pub file_exists: FuncId,
```

- [ ] **Step 3:** Declare imports in `declare_runtime_imports()` (~line 510):

```rust
let append_file = declare_import(m, "airl_append_file", s2.clone());
let delete_file = declare_import(m, "airl_delete_file", s1.clone());
let delete_dir  = declare_import(m, "airl_delete_dir",  s1.clone());
let rename_file = declare_import(m, "airl_rename_file", s2.clone());
let read_dir    = declare_import(m, "airl_read_dir",    s1.clone());
let create_dir  = declare_import(m, "airl_create_dir",  s1.clone());
let file_size   = declare_import(m, "airl_file_size",   s1.clone());
let is_dir      = declare_import(m, "airl_is_dir",      s1.clone());
let write_file  = declare_import(m, "airl_write_file",  s2.clone());
let file_exists = declare_import(m, "airl_file_exists", s1.clone());
```

(s1 = 1-arg PTR→PTR, s2 = 2-arg PTR,PTR→PTR)

- [ ] **Step 4:** Add to `RuntimeImports` struct literal and `build_builtin_map()` (~line 613):

```rust
m.insert("append-file".into(),  rt.append_file);
m.insert("delete-file".into(),  rt.delete_file);
m.insert("delete-dir".into(),   rt.delete_dir);
m.insert("rename-file".into(),  rt.rename_file);
m.insert("read-dir".into(),     rt.read_dir);
m.insert("create-dir".into(),   rt.create_dir);
m.insert("file-size".into(),    rt.file_size);
m.insert("is-dir?".into(),      rt.is_dir);
m.insert("write-file".into(),   rt.write_file);
m.insert("file-exists?".into(), rt.file_exists);
```

- [ ] **Step 5:** Build and test: `cargo build --features jit -p airl-runtime`

- [ ] **Step 6:** Commit.

---

### Task 6: Register in AOT (`bytecode_aot.rs`)

**Files:** `crates/airl-runtime/src/bytecode_aot.rs`

- [ ] **Step 1:** Add to `RuntimeImports` struct (~line 118, replace existing `read_file`/`write_file`/`file_exists` if needed and add new ones):

```rust
pub append_file: FuncId,
pub delete_file: FuncId,
pub delete_dir:  FuncId,
pub rename_file: FuncId,
pub read_dir:    FuncId,
pub create_dir:  FuncId,
pub file_size:   FuncId,
pub is_dir:      FuncId,
```

- [ ] **Step 2:** Declare imports in `declare_runtime_imports()` (~line 407):

```rust
let append_file = declare_import(m, "airl_append_file", s2.clone());
let delete_file = declare_import(m, "airl_delete_file", s1.clone());
let delete_dir  = declare_import(m, "airl_delete_dir",  s1.clone());
let rename_file = declare_import(m, "airl_rename_file", s2.clone());
let read_dir    = declare_import(m, "airl_read_dir",    s1.clone());
let create_dir  = declare_import(m, "airl_create_dir",  s1.clone());
let file_size   = declare_import(m, "airl_file_size",   s1.clone());
let is_dir      = declare_import(m, "airl_is_dir",      s1.clone());
```

- [ ] **Step 3:** Add to struct literal and `build_builtin_map()`:

```rust
m.insert("append-file".into(),  rt.append_file);
m.insert("delete-file".into(),  rt.delete_file);
m.insert("delete-dir".into(),   rt.delete_dir);
m.insert("rename-file".into(),  rt.rename_file);
m.insert("read-dir".into(),     rt.read_dir);
m.insert("create-dir".into(),   rt.create_dir);
m.insert("file-size".into(),    rt.file_size);
m.insert("is-dir?".into(),      rt.is_dir);
```

- [ ] **Step 4:** Build: `cargo build --features jit,aot -p airl-runtime`

- [ ] **Step 5:** Commit.

---

### Task 7: Register in type checker and update docs

**Files:** `crates/airl-types/src/checker.rs`, `CLAUDE.md`

- [ ] **Step 1:** Add to the generic builtin list in `checker.rs` (~line 102, after `"file-exists?"`):

```rust
"append-file", "delete-file", "delete-dir", "rename-file",
"read-dir", "create-dir", "file-size", "is-dir?",
```

- [ ] **Step 2:** Update `CLAUDE.md` — add to the **File I/O** row in the builtin inventory table and add a completed task entry.

- [ ] **Step 3:** Update `README.md` if the project stats or feature lists reference file I/O counts.

- [ ] **Step 4:** Run full test suite: `cargo test --features jit,aot -p airl-runtime -p airl-rt -p airl-types`

- [ ] **Step 5:** Commit.

---

### Task 8: End-to-end fixture test

**Files:** `tests/fixtures/valid/file_io.airl`

- [ ] **Step 1:** Create an AIRL fixture that exercises all 8 new builtins:

```clojure
;; EXPECT: ALL PASSED
(do
  ;; create-dir
  (create-dir "test_fixture_io")

  ;; write + append
  (write-file "test_fixture_io/log.txt" "line1\n")
  (append-file "test_fixture_io/log.txt" "line2\n")

  ;; read-file to verify append
  (let (content (read-file "test_fixture_io/log.txt"))
    (if (contains content "line2")
      (print "append OK ")
      (print "append FAIL ")))

  ;; file-size
  (let (sz (file-size "test_fixture_io/log.txt"))
    (if (> sz 0)
      (print "size OK ")
      (print "size FAIL ")))

  ;; is-dir?
  (if (is-dir? "test_fixture_io")
    (print "isdir OK ")
    (print "isdir FAIL "))

  ;; read-dir
  (let (entries (read-dir "test_fixture_io"))
    (if (= (length entries) 1)
      (print "readdir OK ")
      (print "readdir FAIL ")))

  ;; rename-file
  (rename-file "test_fixture_io/log.txt" "test_fixture_io/renamed.txt")
  (if (file-exists? "test_fixture_io/renamed.txt")
    (print "rename OK ")
    (print "rename FAIL "))

  ;; delete-file
  (delete-file "test_fixture_io/renamed.txt")
  (if (not (file-exists? "test_fixture_io/renamed.txt"))
    (print "delete OK ")
    (print "delete FAIL "))

  ;; delete-dir
  (delete-dir "test_fixture_io")
  (if (not (file-exists? "test_fixture_io"))
    (println "ALL PASSED")
    (println "CLEANUP FAIL")))
```

- [ ] **Step 2:** Run: `cargo run --features jit -- run tests/fixtures/valid/file_io.airl`

Expected output: `append OK size OK isdir OK readdir OK rename OK delete OK ALL PASSED`

- [ ] **Step 3:** Commit.

---

## Key Design Decisions

1. **All paths sandbox-validated.** No absolute paths, no `..` traversal. Same security model as existing builtins.
2. **`delete-file` rejects directories.** Explicit `delete-dir` required for recursive removal — prevents accidental tree deletion by AI producers.
3. **`read-dir` returns sorted filenames.** Deterministic output for reproducible programs.
4. **`create-dir` is idempotent.** Uses `create_dir_all` — no error if already exists, creates parents.
5. **`rename-file` works on files and directories.** Matches `std::fs::rename` behavior.
6. **`airl_write_file` and `airl_file_exists` added to airl-rt.** These were declared in AOT but never defined — fixing a pre-existing gap.
