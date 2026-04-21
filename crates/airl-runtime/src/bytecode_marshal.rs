// crates/airl-runtime/src/bytecode_marshal.rs
//
// Marshals AIRL Value representations of bytecode into BytecodeFunc structs.
//
// Format from AIRL:
//   [(BCFunc "name" arity reg_count capture_count [constants...] [(op dst a b) ...])]
//
// In Rust terms:
//   - A BCFunc is Value::Variant("BCFunc", Box(Value::List([name, arity, reg_count, capture_count, constants_list, instructions_list])))
//   - Each instruction is Value::List([Value::Int(op), Value::Int(dst), Value::Int(a), Value::Int(b)])
//   - Constants are raw Values (Int, Float, Str, Bool, Nil, etc.)

use crate::bytecode::{BytecodeFunc, Instruction, Op};
use crate::value::Value;
use crate::error::RuntimeError;

fn type_err(msg: &str) -> RuntimeError {
    RuntimeError::TypeError(format!("bytecode marshal: {}", msg))
}

/// SEC-15: Validate the AIRL_LINKER_SCRIPT env var if explicitly set.
/// When the env var is not set, returns the default path without validation
/// (the linker will handle missing defaults). When explicitly set, validates
/// that the path exists and is a regular file (not a symlink to an unexpected location).
fn validate_linker_script_env(default: &str) -> Result<String, RuntimeError> {
    match std::env::var("AIRL_LINKER_SCRIPT") {
        Ok(script) if script.is_empty() => Ok(default.into()),
        Ok(script) => {
            let path = std::path::Path::new(&script);
            if !path.exists() {
                return Err(RuntimeError::Custom(format!(
                    "AIRL_LINKER_SCRIPT: path does not exist: {}", script
                )));
            }
            let metadata = std::fs::metadata(&script).map_err(|e| {
                RuntimeError::Custom(format!(
                    "AIRL_LINKER_SCRIPT: cannot read metadata for {}: {}", script, e
                ))
            })?;
            if !metadata.is_file() {
                return Err(RuntimeError::Custom(format!(
                    "AIRL_LINKER_SCRIPT: path is not a regular file: {}", script
                )));
            }
            Ok(script)
        }
        Err(_) => Ok(default.into()),
    }
}

/// Convert a Value::Int to u16, for register/index fields.
fn value_to_u16(val: &Value, field: &str) -> Result<u16, RuntimeError> {
    match val {
        Value::Int(n) if *n < 0 || *n > u16::MAX as i64 => {
            Err(type_err(&format!("{}: value {} out of u16 range", field, n)))
        }
        Value::Int(n) => Ok(*n as u16),
        _ => Err(type_err(&format!("{}: expected Int, got {:?}", field, val))),
    }
}

/// Map an integer discriminant to the Op enum.
/// Order must match the #[repr(u8)] order in bytecode.rs.
fn int_to_op(n: u16) -> Result<Op, RuntimeError> {
    match n {
        0  => Ok(Op::LoadConst),
        1  => Ok(Op::LoadNil),
        2  => Ok(Op::LoadTrue),
        3  => Ok(Op::LoadFalse),
        4  => Ok(Op::Move),
        5  => Ok(Op::Add),
        6  => Ok(Op::Sub),
        7  => Ok(Op::Mul),
        8  => Ok(Op::Div),
        9  => Ok(Op::Mod),
        10 => Ok(Op::Eq),
        11 => Ok(Op::Ne),
        12 => Ok(Op::Lt),
        13 => Ok(Op::Le),
        14 => Ok(Op::Gt),
        15 => Ok(Op::Ge),
        16 => Ok(Op::Not),
        17 => Ok(Op::Neg),
        18 => Ok(Op::Jump),
        19 => Ok(Op::JumpIfFalse),
        20 => Ok(Op::JumpIfTrue),
        21 => Ok(Op::Call),
        22 => Ok(Op::CallBuiltin),
        23 => Ok(Op::CallReg),
        24 => Ok(Op::TailCall),
        25 => Ok(Op::Return),
        26 => Ok(Op::MakeList),
        27 => Ok(Op::MakeVariant),
        28 => Ok(Op::MakeVariant0),
        29 => Ok(Op::MakeClosure),
        30 => Ok(Op::MatchTag),
        31 => Ok(Op::JumpIfNoMatch),
        32 => Ok(Op::MatchWild),
        33 => Ok(Op::TryUnwrap),
        34 => Ok(Op::AssertRequires),
        35 => Ok(Op::AssertEnsures),
        36 => Ok(Op::AssertInvariant),
        37 => Ok(Op::MarkMoved),
        38 => Ok(Op::CheckNotMoved),
        39 => Ok(Op::Release),
        _  => Err(type_err(&format!("unknown opcode: {}", n))),
    }
}

/// Convert a `[op dst a b]` list Value into an Instruction.
/// The `b` field can hold signed jump offsets; we store as u16 and the VM
/// casts back with `as i16`.
fn value_to_instruction(val: &Value) -> Result<Instruction, RuntimeError> {
    match val {
        Value::List(items) => {
            if items.len() < 4 {
                return Err(type_err(&format!(
                    "instruction: expected 4 elements, got {}",
                    items.len()
                )));
            }
            let op_n = value_to_u16(&items[0], "instruction op")?;
            let op   = int_to_op(op_n)?;
            let dst  = value_to_u16(&items[1], "instruction dst")?;
            let a    = value_to_u16(&items[2], "instruction a")?;
            let b    = value_to_u16(&items[3], "instruction b")?;
            Ok(Instruction::new(op, dst, a, b))
        }
        Value::IntList(ints) => {
            if ints.len() < 4 {
                return Err(type_err(&format!(
                    "instruction: expected 4 elements, got {}",
                    ints.len()
                )));
            }
            for (i, &v) in ints[..4].iter().enumerate() {
                if v < 0 || v > u16::MAX as i64 {
                    return Err(type_err(&format!(
                        "instruction field {}: value {} out of u16 range", i, v
                    )));
                }
            }
            let op  = int_to_op(ints[0] as u16)?;
            let dst = ints[1] as u16;
            let a   = ints[2] as u16;
            let b   = ints[3] as u16;
            Ok(Instruction::new(op, dst, a, b))
        }
        _ => Err(type_err("expected instruction as list [op dst a b]")),
    }
}

/// Convert a `BCFunc` variant Value into a BytecodeFunc.
///
/// Expected shape:
///   (BCFunc "name" arity reg_count capture_count [constants...] [(op dst a b) ...])
///
/// which in Value terms is:
///   Variant("BCFunc", List([Str(name), Int(arity), Int(reg_count), Int(capture_count),
///                            List(constants), List(instructions)]))
pub fn value_to_bytecode_func(val: &Value) -> Result<BytecodeFunc, RuntimeError> {
    match val {
        // Spec 3 phase 2 — native BCFunc path. Convert the Arc<BcFunc> directly
        // to BytecodeFunc with one O(n_consts) constant marshal and an
        // O(n_instrs) opcode decode. No intermediate Value::Variant tree.
        Value::BCFuncNative(bcf) => {
            let mut instructions = Vec::with_capacity(bcf.instructions.len());
            for i in &bcf.instructions {
                let op = int_to_op(i.op as u16)?;
                instructions.push(Instruction { op, dst: i.dst, a: i.a, b: i.b });
            }
            let mut constants = Vec::with_capacity(bcf.constants.len());
            for &c in &bcf.constants {
                constants.push(crate::bytecode_vm::rt_to_value_no_release(c));
            }
            Ok(BytecodeFunc {
                name: bcf.name.clone(),
                arity: bcf.arity,
                register_count: bcf.reg_count,
                capture_count: bcf.capture_count,
                instructions,
                constants,
            })
        }
        Value::Variant(tag, inner) if tag == "BCFunc" => {
            // When a variant constructor is called with N args in AIRL:
            //   N=1 → inner is the single value
            //   N>1 → inner is Value::Tuple([...])
            // So (BCFunc name arity reg_count capture_count consts instrs) produces
            // inner = Value::Tuple([name, arity, reg_count, capture_count, consts, instrs]).
            // We also accept Value::List for programmatic construction.
            let items: &[Value] = match inner.as_ref() {
                Value::Tuple(items) => items,
                Value::List(items) => items,
                _ => return Err(type_err("BCFunc inner value must be a Tuple or List")),
            };
            if items.len() < 6 {
                return Err(type_err(&format!(
                    "BCFunc: expected 6 fields, got {}",
                    items.len()
                )));
            }
            let name = match &items[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(type_err("BCFunc name: expected Str")),
            };
            let arity          = value_to_u16(&items[1], "BCFunc arity")?;
            let register_count = value_to_u16(&items[2], "BCFunc reg_count")?;
            let capture_count  = value_to_u16(&items[3], "BCFunc capture_count")?;

            let constants = match &items[4] {
                Value::List(cs) => cs.clone(),
                _ => return Err(type_err("BCFunc constants: expected List")),
            };

            let instructions = match &items[5] {
                Value::List(is) => is
                    .iter()
                    .map(value_to_instruction)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(type_err("BCFunc instructions: expected List")),
            };

            Ok(BytecodeFunc {
                name,
                arity,
                register_count,
                capture_count,
                instructions,
                constants,
            })
        }
        Value::Variant(tag, _) => Err(type_err(&format!("expected BCFunc variant, got {}", tag))),
        _ => Err(type_err("expected Variant, got non-variant value")),
    }
}

/// Build a BytecodeVm from a slice of BCFunc Values, load all functions, and
/// run `__main__`.
pub fn run_bytecode_program(funcs: &[Value]) -> Result<Value, RuntimeError> {
    let mut vm = crate::bytecode_vm::BytecodeVm::new();
    for f in funcs {
        let func = value_to_bytecode_func(f)?;
        vm.load_function(func);
    }
    vm.exec_main()
}

/// C-ABI entry point for `run-bytecode`.  Takes a single `*mut RtValue`
/// that is a List of BCFunc variants, marshals them into a BytecodeVm,
/// executes `__main__`, and returns the result as a new `*mut RtValue`.
///
/// This allows AOT-compiled native binaries to execute bytecode at runtime
/// (needed for the self-hosting compiler pipeline).
#[no_mangle]
pub extern "C" fn airl_run_bytecode(prog: *mut airl_rt::value::RtValue) -> *mut airl_rt::value::RtValue {
    // Convert the RtValue list → Vec<Value>
    let val = crate::bytecode_vm::rt_to_value(prog);
    let funcs = match &val {
        Value::List(items) => items.clone(),
        _ => {
            eprintln!("airl_run_bytecode: expected list of BCFunc, got {}", val);
            return airl_rt::value::rt_nil();
        }
    };
    match run_bytecode_program(&funcs) {
        Ok(result) => crate::bytecode_vm::value_to_rt(&result),
        Err(e) => {
            eprintln!("Runtime error: {}", e);
            airl_rt::value::rt_nil()
        }
    }
}

/// C-ABI entry point for `compile-bytecode-to-executable`.
/// Takes a list of BCFunc values and an output path string.
/// Delegates to the 3-arg `compile_bytecode_to_executable_with_target` with `target = None`.
#[cfg(feature = "aot")]
#[no_mangle]
pub extern "C" fn airl_compile_bytecode_to_executable(
    funcs_val: *mut airl_rt::value::RtValue,
    output_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    let funcs_value = crate::bytecode_vm::rt_to_value(funcs_val);
    let output_value = crate::bytecode_vm::rt_to_value(output_val);

    let funcs = match &funcs_value {
        Value::List(items) => items.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_to_executable: first arg must be list of BCFunc");
            return airl_rt::value::rt_nil();
        }
    };
    let output_path = match &output_value {
        Value::Str(s) => s.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_to_executable: second arg must be output path string");
            return airl_rt::value::rt_nil();
        }
    };

    match compile_bytecode_to_executable_with_target(&funcs, &output_path, None) {
        Ok(()) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Ok".into(), rt_str(format!("Compiled to {}", output_path)))
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("Compilation error: {}", e)))
        }
    }
}

/// Compile a list of BCFunc values to a native executable via Cranelift AOT.
/// Takes the same BCFunc format as `run-bytecode` but produces a binary instead of executing.
/// Delegates to `compile_bytecode_to_executable_with_target` with `target = None` (host).
#[cfg(feature = "aot")]
pub fn compile_bytecode_to_executable(funcs: &[Value], output_path: &str) -> Result<(), RuntimeError> {
    compile_bytecode_to_executable_with_target(funcs, output_path, None)
}

/// C-ABI entry point for `compile-bytecode-to-executable-with-target`.
/// Takes a list of BCFunc values, an output path string, and a target triple string.
/// Empty target string means host target (same as the 2-arg version).
#[cfg(feature = "aot")]
#[no_mangle]
pub extern "C" fn airl_compile_bytecode_to_executable_with_target(
    funcs_val: *mut airl_rt::value::RtValue,
    output_val: *mut airl_rt::value::RtValue,
    target_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    let funcs_value = crate::bytecode_vm::rt_to_value(funcs_val);
    let output_value = crate::bytecode_vm::rt_to_value(output_val);
    let target_value = crate::bytecode_vm::rt_to_value(target_val);

    let funcs = match &funcs_value {
        Value::List(items) => items.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_to_executable_with_target: first arg must be list of BCFunc");
            return airl_rt::value::rt_nil();
        }
    };
    let output_path = match &output_value {
        Value::Str(s) => s.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_to_executable_with_target: second arg must be output path string");
            return airl_rt::value::rt_nil();
        }
    };
    let target_str = match &target_value {
        Value::Str(s) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    };

    match compile_bytecode_to_executable_with_target(&funcs, &output_path, target_str) {
        Ok(()) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Ok".into(), rt_str(format!("Compiled to {} (target: {})", output_path, target_str.unwrap_or("host"))))
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("Compilation error: {}", e)))
        }
    }
}

/// Compile a list of BCFunc values to a native executable via Cranelift AOT,
/// with an optional cross-compilation target.
/// `target` of None means host target. Supported targets include
/// "i686-airlos", "x86_64-airlos", "i686", "x86-64", "aarch64".
#[cfg(feature = "aot")]
pub fn compile_bytecode_to_executable_with_target(funcs: &[Value], output_path: &str, target: Option<&str>) -> Result<(), RuntimeError> {
    use crate::bytecode_aot::BytecodeAot;
    use std::collections::HashMap;

    // Unmarshal all BCFunc values
    let mut bc_funcs = Vec::new();
    for f in funcs {
        bc_funcs.push(value_to_bytecode_func(f)?);
    }

    // Dedup by name with first-wins to match AIRL's first-def-wins semantics.
    // Prevents lambda name mismatch when stdlib files are also passed as user files.
    {
        let mut seen = std::collections::HashSet::new();
        bc_funcs.retain(|f| seen.insert(f.name.clone()));
    }

    // Build function map for cross-reference resolution
    let func_map: HashMap<String, BytecodeFunc> = bc_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    // AOT compile bytecode to native object, using the specified target
    let mut aot = BytecodeAot::new_with_target(target)
        .map_err(|e| RuntimeError::Custom(format!("AOT init: {}", e)))?;

    // Register Z3 bridge builtins so g3-compiled code can call them
    // without extern-c declarations.  These symbols live in libairl_rt.a
    // and are linked via -lz3; safe to register unconditionally.
    #[cfg(not(target_os = "airlos"))]
    {
        aot.register_extern_c("airl_z3_mk_config", 0);
        aot.register_extern_c("airl_z3_del_config", 1);
        aot.register_extern_c("airl_z3_mk_context", 1);
        aot.register_extern_c("airl_z3_del_context", 1);
        aot.register_extern_c("airl_z3_mk_solver", 1);
        aot.register_extern_c("airl_z3_del_solver", 2);
        aot.register_extern_c("airl_z3_mk_int_sort", 1);
        aot.register_extern_c("airl_z3_mk_bool_sort", 1);
        aot.register_extern_c("airl_z3_mk_string_symbol", 2);
        aot.register_extern_c("airl_z3_mk_const", 3);
        aot.register_extern_c("airl_z3_mk_int_val", 3);
        aot.register_extern_c("airl_z3_mk_true", 1);
        aot.register_extern_c("airl_z3_mk_false", 1);
        aot.register_extern_c("airl_z3_mk_add2", 3);
        aot.register_extern_c("airl_z3_mk_sub2", 3);
        aot.register_extern_c("airl_z3_mk_mul2", 3);
        aot.register_extern_c("airl_z3_mk_div", 3);
        aot.register_extern_c("airl_z3_mk_mod", 3);
        aot.register_extern_c("airl_z3_mk_lt", 3);
        aot.register_extern_c("airl_z3_mk_le", 3);
        aot.register_extern_c("airl_z3_mk_gt", 3);
        aot.register_extern_c("airl_z3_mk_ge", 3);
        aot.register_extern_c("airl_z3_mk_eq", 3);
        aot.register_extern_c("airl_z3_mk_and2", 3);
        aot.register_extern_c("airl_z3_mk_or2", 3);
        aot.register_extern_c("airl_z3_mk_not", 2);
        aot.register_extern_c("airl_z3_mk_implies", 3);
        aot.register_extern_c("airl_z3_mk_ite", 4);
        aot.register_extern_c("airl_z3_solver_assert", 3);
        aot.register_extern_c("airl_z3_solver_check", 2);
        // Real sort (issue-133)
        aot.register_extern_c("airl_z3_mk_real_sort", 1);
        aot.register_extern_c("airl_z3_mk_real", 3);
        aot.register_extern_c("airl_z3_mk_int2real", 2);
        // String sort (issue-133)
        aot.register_extern_c("airl_z3_mk_string_sort", 1);
        aot.register_extern_c("airl_z3_mk_string_val", 2);
        aot.register_extern_c("airl_z3_mk_seq_sort", 2);
        aot.register_extern_c("airl_z3_mk_seq_unit", 2);
        aot.register_extern_c("airl_z3_mk_seq_length", 2);
        aot.register_extern_c("airl_z3_mk_seq_contains", 3);
        aot.register_extern_c("airl_z3_mk_seq_concat2", 3);
        // Quantifiers (issue-134)
        aot.register_extern_c("airl_z3_mk_forall_const1", 3);
        aot.register_extern_c("airl_z3_mk_exists_const1", 3);
        aot.register_extern_c("airl_z3_mk_forall_const2", 4);
        aot.register_extern_c("airl_z3_mk_exists_const2", 4);
        // Model / counterexample (issue-136)
        aot.register_extern_c("airl_z3_solver_get_model", 2);
        aot.register_extern_c("airl_z3_model_to_string", 2);
        // Uninterpreted functions (issue-140)
        aot.register_extern_c("airl_z3_mk_func_decl1", 4);
        aot.register_extern_c("airl_z3_mk_func_decl2", 5);
        aot.register_extern_c("airl_z3_mk_app1", 3);
        aot.register_extern_c("airl_z3_mk_app2", 4);
    }

    for func in &bc_funcs {
        aot.compile_all(std::slice::from_ref(func), &func_map)
            .map_err(|e| RuntimeError::Custom(format!("AOT compile '{}': {}", func.name, e)))?;
    }
    aot.emit_entry_point()
        .map_err(|e| RuntimeError::Custom(format!("AOT entry point: {}", e)))?;
    let obj_bytes = aot.finish();

    // Write object file
    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| RuntimeError::Custom(format!("write {}: {}", obj_path, e)))?;

    // For freestanding targets, use cross-linker
    if target == Some("i686-airlos") {
        let script = validate_linker_script_env("user.ld")?;
        let mut cmd = std::process::Command::new("i686-elf-ld");
        cmd.arg("-T").arg(&script);
        cmd.arg(&obj_path);
        cmd.arg("-o").arg(output_path);
        if let Ok(rt_path) = std::env::var("AIRL_RT_AIRLOS") {
            cmd.arg(&rt_path);
        }
        let status = cmd.status();
        let _ = std::fs::remove_file(&obj_path);
        return match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(RuntimeError::Custom(format!("cross-linker (i686-elf-ld) failed: {:?}", s.code()))),
            Err(e) => Err(RuntimeError::Custom(format!("cross-linker (i686-elf-ld) not found: {}", e))),
        };
    }

    if target == Some("x86_64-airlos") {
        let script = validate_linker_script_env("user64.ld")?;
        let mut cmd = std::process::Command::new("x86_64-elf-ld");
        cmd.arg("-T").arg(&script);
        cmd.arg(&obj_path);
        cmd.arg("-o").arg(output_path);
        if let Ok(rt_path) = std::env::var("AIRL_RT_AIRLOS_X64") {
            cmd.arg(&rt_path);
        }
        let status = cmd.status();
        let _ = std::fs::remove_file(&obj_path);
        return match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(RuntimeError::Custom(format!("cross-linker (x86_64-elf-ld) failed: {:?}", s.code()))),
            Err(e) => Err(RuntimeError::Custom(format!("cross-linker (x86_64-elf-ld) not found: {}", e))),
        };
    }

    // Non-freestanding targets: link with system cc
    let rt_lib = crate::bytecode_aot::get_or_extract_rt_lib()
        .map_err(|e| RuntimeError::Custom(e))?;

    let needs_compiler = bc_funcs.iter().any(|f| {
        f.constants.iter().any(|c| matches!(c,
            Value::Str(s) if s == "compile-bytecode-to-executable"
                || s == "compile-bytecode-to-executable-with-target"
                || s == "compile-to-executable"
                || s == "run-bytecode"))
    });

    let mut cmd = std::process::Command::new("cc");
    cmd.arg(&obj_path).arg("-o").arg(output_path);

    if needs_compiler {
        let runtime_lib = crate::bytecode_aot::find_lib("airl_runtime");
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
        } else {
            cmd.arg(&rt_lib);
        }
    } else {
        cmd.arg(&rt_lib);
    }

    #[cfg(target_os = "linux")]
    { cmd.arg("-lm").arg("-lpthread").arg("-ldl").arg("-lsqlite3").arg("-lz3"); }
    #[cfg(target_os = "macos")]
    { cmd.arg("-lSystem").arg("-lsqlite3").arg("-lz3"); }

    let status = cmd
        .status()
        .map_err(|e| RuntimeError::Custom(format!("linker: {}", e)))?;

    let _ = std::fs::remove_file(&obj_path);
    // SEC-14: clean up temp runtime library after linking
    if rt_lib.starts_with(&std::env::temp_dir().to_string_lossy().to_string()) {
        let _ = std::fs::remove_file(&rt_lib);
    }

    if status.success() {
        Ok(())
    } else {
        Err(RuntimeError::Custom(format!("linker failed: {:?}", status.code())))
    }
}

/// Compile a list of BCFunc batches to a native executable via streaming
/// per-file AOT compilation.  Each element of `batches` is a `Value::List`
/// of BCFunc values; each batch is compiled to a separate object file,
/// allowing memory to be freed between files.  A tiny entry-point object
/// is generated last, then all objects are linked into one binary.
///
/// This is the host-only (non-cross-compilation) path used by g3 to avoid
/// OOM when compiling large projects with 40+ source files.
#[cfg(feature = "aot")]
pub fn compile_bytecode_streaming(batches: &[Value], output_path: &str) -> Result<(), RuntimeError> {
    use crate::bytecode_aot::BytecodeAot;
    use std::collections::HashMap;

    // Freestanding targets (airlos) are not supported by the streaming path.
    // They are handled by the single-object compile_bytecode_to_executable_with_target.
    let mut obj_paths: Vec<String> = Vec::new();
    let mut needs_compiler = false;

    let outer = match batches.first() {
        None => {
            return Err(RuntimeError::Custom("compile-bytecode-streaming: empty batch list".into()));
        }
        Some(_) => batches,
    };

    for (i, batch_val) in outer.iter().enumerate() {
        let batch_funcs = match batch_val {
            Value::List(items) => items.clone(),
            _ => {
                return Err(RuntimeError::Custom(format!(
                    "compile-bytecode-streaming: batch {} must be a list of BCFunc", i
                )));
            }
        };

        if batch_funcs.is_empty() {
            continue;
        }

        // Unmarshal and dedup (first-wins to match AIRL's first-def-wins semantics)
        let mut bc_funcs: Vec<crate::bytecode::BytecodeFunc> = Vec::new();
        for f in &batch_funcs {
            bc_funcs.push(value_to_bytecode_func(f)?);
        }
        {
            let mut seen = std::collections::HashSet::new();
            bc_funcs.retain(|f| seen.insert(f.name.clone()));
        }

        // Check if any function in this batch needs the full runtime library
        if !needs_compiler {
            needs_compiler = bc_funcs.iter().any(|f| {
                f.constants.iter().any(|c| matches!(c,
                    Value::Str(s) if s == "compile-bytecode-to-executable"
                        || s == "compile-bytecode-to-executable-with-target"
                        || s == "compile-bytecode-streaming"
                        || s == "compile-to-executable"
                        || s == "run-bytecode"))
            });
        }

        let func_map: HashMap<String, crate::bytecode::BytecodeFunc> = bc_funcs.iter()
            .map(|f| (f.name.clone(), f.clone()))
            .collect();

        let mut aot = BytecodeAot::new_with_target(None)
            .map_err(|e| RuntimeError::Custom(format!("AOT init batch {}: {}", i, e)))?;

        // Register Z3 bridge builtins for this batch's module
        #[cfg(not(target_os = "airlos"))]
        {
            aot.register_extern_c("airl_z3_mk_config", 0);
            aot.register_extern_c("airl_z3_del_config", 1);
            aot.register_extern_c("airl_z3_mk_context", 1);
            aot.register_extern_c("airl_z3_del_context", 1);
            aot.register_extern_c("airl_z3_mk_solver", 1);
            aot.register_extern_c("airl_z3_del_solver", 2);
            aot.register_extern_c("airl_z3_mk_int_sort", 1);
            aot.register_extern_c("airl_z3_mk_bool_sort", 1);
            aot.register_extern_c("airl_z3_mk_string_symbol", 2);
            aot.register_extern_c("airl_z3_mk_const", 3);
            aot.register_extern_c("airl_z3_mk_int_val", 3);
            aot.register_extern_c("airl_z3_mk_true", 1);
            aot.register_extern_c("airl_z3_mk_false", 1);
            aot.register_extern_c("airl_z3_mk_add2", 3);
            aot.register_extern_c("airl_z3_mk_sub2", 3);
            aot.register_extern_c("airl_z3_mk_mul2", 3);
            aot.register_extern_c("airl_z3_mk_div", 3);
            aot.register_extern_c("airl_z3_mk_mod", 3);
            aot.register_extern_c("airl_z3_mk_lt", 3);
            aot.register_extern_c("airl_z3_mk_le", 3);
            aot.register_extern_c("airl_z3_mk_gt", 3);
            aot.register_extern_c("airl_z3_mk_ge", 3);
            aot.register_extern_c("airl_z3_mk_eq", 3);
            aot.register_extern_c("airl_z3_mk_and2", 3);
            aot.register_extern_c("airl_z3_mk_or2", 3);
            aot.register_extern_c("airl_z3_mk_not", 2);
            aot.register_extern_c("airl_z3_mk_implies", 3);
            aot.register_extern_c("airl_z3_mk_ite", 4);
            aot.register_extern_c("airl_z3_solver_assert", 3);
            aot.register_extern_c("airl_z3_solver_check", 2);
            aot.register_extern_c("airl_z3_mk_real_sort", 1);
            aot.register_extern_c("airl_z3_mk_real", 3);
            aot.register_extern_c("airl_z3_mk_int2real", 2);
            aot.register_extern_c("airl_z3_mk_string_sort", 1);
            aot.register_extern_c("airl_z3_mk_string_val", 2);
            aot.register_extern_c("airl_z3_mk_seq_sort", 2);
            aot.register_extern_c("airl_z3_mk_seq_unit", 2);
            aot.register_extern_c("airl_z3_mk_seq_length", 2);
            aot.register_extern_c("airl_z3_mk_seq_contains", 3);
            aot.register_extern_c("airl_z3_mk_seq_concat2", 3);
            aot.register_extern_c("airl_z3_mk_forall_const1", 3);
            aot.register_extern_c("airl_z3_mk_exists_const1", 3);
            aot.register_extern_c("airl_z3_mk_forall_const2", 4);
            aot.register_extern_c("airl_z3_mk_exists_const2", 4);
            aot.register_extern_c("airl_z3_solver_get_model", 2);
            aot.register_extern_c("airl_z3_model_to_string", 2);
            aot.register_extern_c("airl_z3_mk_func_decl1", 4);
            aot.register_extern_c("airl_z3_mk_func_decl2", 5);
            aot.register_extern_c("airl_z3_mk_app1", 3);
            aot.register_extern_c("airl_z3_mk_app2", 4);
        }

        for func in &bc_funcs {
            aot.compile_all(std::slice::from_ref(func), &func_map)
                .map_err(|e| RuntimeError::Custom(format!("AOT compile batch {} '{}': {}", i, func.name, e)))?;
        }

        let obj_bytes = aot.finish_no_entry();
        let obj_path = format!("{}.batch{}.o", output_path, i);
        std::fs::write(&obj_path, &obj_bytes)
            .map_err(|e| RuntimeError::Custom(format!("write {}: {}", obj_path, e)))?;
        obj_paths.push(obj_path);
        // bc_funcs and func_map are dropped here — memory freed before next batch
    }

    if obj_paths.is_empty() {
        return Err(RuntimeError::Custom("compile-bytecode-streaming: no functions to compile".into()));
    }

    // Emit the entry-point object: a minimal module that imports __airl_main_entry__
    // and defines main() calling it.
    let mut entry_aot = BytecodeAot::new_with_target(None)
        .map_err(|e| RuntimeError::Custom(format!("AOT init entry: {}", e)))?;
    entry_aot.emit_entry_point_external()
        .map_err(|e| RuntimeError::Custom(format!("AOT entry point: {}", e)))?;
    let entry_bytes = entry_aot.finish_no_entry();
    let entry_obj_path = format!("{}.entry.o", output_path);
    std::fs::write(&entry_obj_path, &entry_bytes)
        .map_err(|e| RuntimeError::Custom(format!("write {}: {}", entry_obj_path, e)))?;
    obj_paths.push(entry_obj_path);

    // Link all object files into the final binary
    let rt_lib = crate::bytecode_aot::get_or_extract_rt_lib()
        .map_err(|e| RuntimeError::Custom(e))?;

    let mut cmd = std::process::Command::new("cc");
    for p in &obj_paths {
        cmd.arg(p);
    }
    cmd.arg("-o").arg(output_path);

    if needs_compiler {
        let runtime_lib = crate::bytecode_aot::find_lib("airl_runtime");
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
        } else {
            cmd.arg(&rt_lib);
        }
    } else {
        cmd.arg(&rt_lib);
    }

    #[cfg(target_os = "linux")]
    { cmd.arg("-lm").arg("-lpthread").arg("-ldl").arg("-lsqlite3").arg("-lz3"); }
    #[cfg(target_os = "macos")]
    { cmd.arg("-lSystem").arg("-lsqlite3").arg("-lz3"); }

    let status = cmd
        .status()
        .map_err(|e| RuntimeError::Custom(format!("linker: {}", e)))?;

    for p in &obj_paths {
        let _ = std::fs::remove_file(p);
    }
    if rt_lib.starts_with(&std::env::temp_dir().to_string_lossy().to_string()) {
        let _ = std::fs::remove_file(&rt_lib);
    }

    if status.success() {
        Ok(())
    } else {
        Err(RuntimeError::Custom(format!("linker failed: {:?}", status.code())))
    }
}

/// Compile a single batch of BCFuncs to a single .o file on disk.
///
/// Returns `needs_compiler`: whether any function in this batch references
/// the runtime-library-dependent builtins (run-bytecode, compile-*). Callers
/// must OR-accumulate this across all batches and pass the result to
/// `link_objs_to_binary` so the linker includes airl-runtime only when needed.
///
/// The in-memory BCFunc list is dropped on return — nothing is retained
/// across calls. This is the per-file-emit primitive that lets AIRL-level
/// drivers (g3_compiler.airl) run compile + emit + free in a loop rather
/// than accumulating all bytecode before emitting.
#[cfg(feature = "aot")]
pub fn compile_batch_to_obj(batch: &[Value], obj_path: &str) -> Result<bool, RuntimeError> {
    use crate::bytecode_aot::BytecodeAot;
    use std::collections::HashMap;

    if batch.is_empty() {
        return Err(RuntimeError::Custom("compile-batch-to-obj: empty batch".into()));
    }

    // Unmarshal and dedup (first-wins to match AIRL's first-def-wins semantics)
    let mut bc_funcs: Vec<crate::bytecode::BytecodeFunc> = Vec::new();
    for f in batch {
        bc_funcs.push(value_to_bytecode_func(f)?);
    }
    {
        let mut seen = std::collections::HashSet::new();
        bc_funcs.retain(|f| seen.insert(f.name.clone()));
    }

    let needs_compiler = bc_funcs.iter().any(|f| {
        f.constants.iter().any(|c| matches!(c,
            Value::Str(s) if s == "compile-bytecode-to-executable"
                || s == "compile-bytecode-to-executable-with-target"
                || s == "compile-bytecode-streaming"
                || s == "compile-batch-to-obj"
                || s == "link-objs-to-binary"
                || s == "compile-to-executable"
                || s == "run-bytecode"))
    });

    let func_map: HashMap<String, crate::bytecode::BytecodeFunc> = bc_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new_with_target(None)
        .map_err(|e| RuntimeError::Custom(format!("AOT init: {}", e)))?;

    // Register Z3 bridge builtins so cross-batch AIRL -> Z3 calls resolve
    // against libz3 at final link time.
    #[cfg(not(target_os = "airlos"))]
    {
        aot.register_extern_c("airl_z3_mk_config", 0);
        aot.register_extern_c("airl_z3_del_config", 1);
        aot.register_extern_c("airl_z3_mk_context", 1);
        aot.register_extern_c("airl_z3_del_context", 1);
        aot.register_extern_c("airl_z3_mk_solver", 1);
        aot.register_extern_c("airl_z3_del_solver", 2);
        aot.register_extern_c("airl_z3_mk_int_sort", 1);
        aot.register_extern_c("airl_z3_mk_bool_sort", 1);
        aot.register_extern_c("airl_z3_mk_string_symbol", 2);
        aot.register_extern_c("airl_z3_mk_const", 3);
        aot.register_extern_c("airl_z3_mk_int_val", 3);
        aot.register_extern_c("airl_z3_mk_true", 1);
        aot.register_extern_c("airl_z3_mk_false", 1);
        aot.register_extern_c("airl_z3_mk_add2", 3);
        aot.register_extern_c("airl_z3_mk_sub2", 3);
        aot.register_extern_c("airl_z3_mk_mul2", 3);
        aot.register_extern_c("airl_z3_mk_div", 3);
        aot.register_extern_c("airl_z3_mk_mod", 3);
        aot.register_extern_c("airl_z3_mk_lt", 3);
        aot.register_extern_c("airl_z3_mk_le", 3);
        aot.register_extern_c("airl_z3_mk_gt", 3);
        aot.register_extern_c("airl_z3_mk_ge", 3);
        aot.register_extern_c("airl_z3_mk_eq", 3);
        aot.register_extern_c("airl_z3_mk_and2", 3);
        aot.register_extern_c("airl_z3_mk_or2", 3);
        aot.register_extern_c("airl_z3_mk_not", 2);
        aot.register_extern_c("airl_z3_mk_implies", 3);
        aot.register_extern_c("airl_z3_mk_ite", 4);
        aot.register_extern_c("airl_z3_solver_assert", 3);
        aot.register_extern_c("airl_z3_solver_check", 2);
        aot.register_extern_c("airl_z3_mk_real_sort", 1);
        aot.register_extern_c("airl_z3_mk_real", 3);
        aot.register_extern_c("airl_z3_mk_int2real", 2);
        aot.register_extern_c("airl_z3_mk_string_sort", 1);
        aot.register_extern_c("airl_z3_mk_string_val", 2);
        aot.register_extern_c("airl_z3_mk_seq_sort", 2);
        aot.register_extern_c("airl_z3_mk_seq_unit", 2);
        aot.register_extern_c("airl_z3_mk_seq_length", 2);
        aot.register_extern_c("airl_z3_mk_seq_contains", 3);
        aot.register_extern_c("airl_z3_mk_seq_concat2", 3);
        aot.register_extern_c("airl_z3_mk_forall_const1", 3);
        aot.register_extern_c("airl_z3_mk_exists_const1", 3);
        aot.register_extern_c("airl_z3_mk_forall_const2", 4);
        aot.register_extern_c("airl_z3_mk_exists_const2", 4);
        aot.register_extern_c("airl_z3_solver_get_model", 2);
        aot.register_extern_c("airl_z3_model_to_string", 2);
        aot.register_extern_c("airl_z3_mk_func_decl1", 4);
        aot.register_extern_c("airl_z3_mk_func_decl2", 5);
        aot.register_extern_c("airl_z3_mk_app1", 3);
        aot.register_extern_c("airl_z3_mk_app2", 4);
    }

    for func in &bc_funcs {
        aot.compile_all(std::slice::from_ref(func), &func_map)
            .map_err(|e| RuntimeError::Custom(format!("AOT compile '{}': {}", func.name, e)))?;
    }

    let obj_bytes = aot.finish_no_entry();
    std::fs::write(obj_path, &obj_bytes)
        .map_err(|e| RuntimeError::Custom(format!("write {}: {}", obj_path, e)))?;
    Ok(needs_compiler)
}

/// Link a list of .o files into the final executable, emitting the cross-batch
/// entry-point .o automatically. Called once after all batches have been
/// emitted via `compile_batch_to_obj`.
///
/// `needs_compiler` should be the OR-accumulation of the bool returned by each
/// `compile_batch_to_obj` call — determines whether to link against the full
/// `airl-runtime` static library.
#[cfg(feature = "aot")]
pub fn link_objs_to_binary(
    obj_paths: &[String],
    output_path: &str,
    needs_compiler: bool,
) -> Result<(), RuntimeError> {
    use crate::bytecode_aot::BytecodeAot;

    if obj_paths.is_empty() {
        return Err(RuntimeError::Custom("link-objs-to-binary: no object files".into()));
    }

    // Emit entry-point .o
    let mut entry_aot = BytecodeAot::new_with_target(None)
        .map_err(|e| RuntimeError::Custom(format!("AOT init entry: {}", e)))?;
    entry_aot.emit_entry_point_external()
        .map_err(|e| RuntimeError::Custom(format!("AOT entry point: {}", e)))?;
    let entry_bytes = entry_aot.finish_no_entry();
    let entry_obj_path = format!("{}.entry.o", output_path);
    std::fs::write(&entry_obj_path, &entry_bytes)
        .map_err(|e| RuntimeError::Custom(format!("write {}: {}", entry_obj_path, e)))?;

    // Build linker command
    let rt_lib = crate::bytecode_aot::get_or_extract_rt_lib()
        .map_err(|e| RuntimeError::Custom(e))?;

    let mut cmd = std::process::Command::new("cc");
    for p in obj_paths {
        cmd.arg(p);
    }
    cmd.arg(&entry_obj_path);
    cmd.arg("-o").arg(output_path);

    if needs_compiler {
        let runtime_lib = crate::bytecode_aot::find_lib("airl_runtime");
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
        } else {
            cmd.arg(&rt_lib);
        }
    } else {
        cmd.arg(&rt_lib);
    }

    #[cfg(target_os = "linux")]
    { cmd.arg("-lm").arg("-lpthread").arg("-ldl").arg("-lsqlite3").arg("-lz3"); }
    #[cfg(target_os = "macos")]
    { cmd.arg("-lSystem").arg("-lsqlite3").arg("-lz3"); }

    let status = cmd
        .status()
        .map_err(|e| RuntimeError::Custom(format!("linker: {}", e)))?;

    // Cleanup .o files (caller-provided + entry)
    for p in obj_paths {
        let _ = std::fs::remove_file(p);
    }
    let _ = std::fs::remove_file(&entry_obj_path);
    if rt_lib.starts_with(&std::env::temp_dir().to_string_lossy().to_string()) {
        let _ = std::fs::remove_file(&rt_lib);
    }

    if status.success() {
        Ok(())
    } else {
        Err(RuntimeError::Custom(format!("linker failed: {:?}", status.code())))
    }
}

/// C-ABI entry point for `compile-batch-to-obj`.
/// Returns Ok(needs_compiler : Bool) or Err(msg : String).
#[cfg(feature = "aot")]
#[no_mangle]
pub extern "C" fn airl_compile_batch_to_obj(
    batch_val: *mut airl_rt::value::RtValue,
    obj_path_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    let batch_value = crate::bytecode_vm::rt_to_value(batch_val);
    let obj_path_value = crate::bytecode_vm::rt_to_value(obj_path_val);

    let batch = match &batch_value {
        Value::List(items) => items.clone(),
        _ => {
            use airl_rt::value::{rt_variant, rt_str};
            return rt_variant("Err".into(), rt_str("compile-batch-to-obj: first arg must be list of BCFunc".into()));
        }
    };
    let obj_path = match &obj_path_value {
        Value::Str(s) => s.clone(),
        _ => {
            use airl_rt::value::{rt_variant, rt_str};
            return rt_variant("Err".into(), rt_str("compile-batch-to-obj: second arg must be path string".into()));
        }
    };

    match compile_batch_to_obj(&batch, &obj_path) {
        Ok(needs_compiler) => {
            use airl_rt::value::{rt_variant, rt_bool};
            rt_variant("Ok".into(), rt_bool(needs_compiler))
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("{}", e)))
        }
    }
}

/// C-ABI entry point for `link-objs-to-binary`.
/// Returns Ok(output_path : String) or Err(msg : String).
#[cfg(feature = "aot")]
#[no_mangle]
pub extern "C" fn airl_link_objs_to_binary(
    paths_val: *mut airl_rt::value::RtValue,
    output_val: *mut airl_rt::value::RtValue,
    needs_compiler_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    let paths_value = crate::bytecode_vm::rt_to_value(paths_val);
    let output_value = crate::bytecode_vm::rt_to_value(output_val);
    let needs_compiler_value = crate::bytecode_vm::rt_to_value(needs_compiler_val);

    let paths: Vec<String> = match &paths_value {
        Value::List(items) => {
            let mut v = Vec::with_capacity(items.len());
            for it in items {
                match it {
                    Value::Str(s) => v.push(s.clone()),
                    _ => {
                        use airl_rt::value::{rt_variant, rt_str};
                        return rt_variant("Err".into(), rt_str("link-objs-to-binary: paths list must contain strings".into()));
                    }
                }
            }
            v
        }
        _ => {
            use airl_rt::value::{rt_variant, rt_str};
            return rt_variant("Err".into(), rt_str("link-objs-to-binary: first arg must be list of path strings".into()));
        }
    };
    let output_path = match &output_value {
        Value::Str(s) => s.clone(),
        _ => {
            use airl_rt::value::{rt_variant, rt_str};
            return rt_variant("Err".into(), rt_str("link-objs-to-binary: second arg must be output path string".into()));
        }
    };
    let needs_compiler = match &needs_compiler_value {
        Value::Bool(b) => *b,
        _ => {
            use airl_rt::value::{rt_variant, rt_str};
            return rt_variant("Err".into(), rt_str("link-objs-to-binary: third arg must be bool".into()));
        }
    };

    match link_objs_to_binary(&paths, &output_path, needs_compiler) {
        Ok(()) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Ok".into(), rt_str(output_path))
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("{}", e)))
        }
    }
}

/// C-ABI entry point for `compile-bytecode-streaming`.
/// Takes a list-of-lists of BCFunc values and an output path string.
#[cfg(feature = "aot")]
#[no_mangle]
pub extern "C" fn airl_compile_bytecode_streaming(
    batches_val: *mut airl_rt::value::RtValue,
    output_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    let batches_value = crate::bytecode_vm::rt_to_value(batches_val);
    let output_value = crate::bytecode_vm::rt_to_value(output_val);

    let batches = match &batches_value {
        Value::List(items) => items.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_streaming: first arg must be list of lists of BCFunc");
            return airl_rt::value::rt_nil();
        }
    };
    let output_path = match &output_value {
        Value::Str(s) => s.clone(),
        _ => {
            eprintln!("airl_compile_bytecode_streaming: second arg must be output path string");
            return airl_rt::value::rt_nil();
        }
    };

    match compile_bytecode_streaming(&batches, &output_path) {
        Ok(()) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Ok".into(), rt_str(format!("Compiled to {}", output_path)))
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("Compilation error: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::Op;

    fn make_bcfunc(name: &str, arity: i64, reg_count: i64, capture_count: i64,
                   constants: Vec<Value>, instrs: Vec<Value>) -> Value {
        Value::Variant(
            "BCFunc".into(),
            Box::new(Value::List(vec![
                Value::Str(name.into()),
                Value::Int(arity),
                Value::Int(reg_count),
                Value::Int(capture_count),
                Value::List(constants),
                Value::List(instrs),
            ])),
        )
    }

    fn instr(op: i64, dst: i64, a: i64, b: i64) -> Value {
        Value::List(vec![
            Value::Int(op),
            Value::Int(dst),
            Value::Int(a),
            Value::Int(b),
        ])
    }

    #[test]
    fn test_marshal_simple_func() {
        // LoadConst r1 ← 42; Return r1
        let func_val = make_bcfunc(
            "__main__", 0, 2, 0,
            vec![Value::Int(42)],
            vec![
                instr(0, 1, 0, 0),  // LoadConst dst=1 const_idx=0
                instr(25, 0, 1, 0), // Return src=1
            ],
        );
        let func = value_to_bytecode_func(&func_val).unwrap();
        assert_eq!(func.name, "__main__");
        assert_eq!(func.arity, 0);
        assert_eq!(func.register_count, 2);
        assert_eq!(func.capture_count, 0);
        assert_eq!(func.constants, vec![Value::Int(42)]);
        assert_eq!(func.instructions.len(), 2);
        assert_eq!(func.instructions[0].op, Op::LoadConst);
        assert_eq!(func.instructions[0].dst, 1);
        assert_eq!(func.instructions[0].a, 0);
        assert_eq!(func.instructions[1].op, Op::Return);
        assert_eq!(func.instructions[1].a, 1);
    }

    #[test]
    fn test_int_to_op_roundtrip() {
        // Spot-check a few opcodes
        assert_eq!(int_to_op(0).unwrap(), Op::LoadConst);
        assert_eq!(int_to_op(5).unwrap(), Op::Add);
        assert_eq!(int_to_op(25).unwrap(), Op::Return);
        assert_eq!(int_to_op(33).unwrap(), Op::TryUnwrap);
        assert_eq!(int_to_op(34).unwrap(), Op::AssertRequires);
        assert_eq!(int_to_op(35).unwrap(), Op::AssertEnsures);
        assert_eq!(int_to_op(36).unwrap(), Op::AssertInvariant);
        assert_eq!(int_to_op(37).unwrap(), Op::MarkMoved);
        assert_eq!(int_to_op(38).unwrap(), Op::CheckNotMoved);
        assert_eq!(int_to_op(39).unwrap(), Op::Release);
        assert!(int_to_op(40).is_err());
    }

    #[test]
    fn test_run_bytecode_add() {
        // __main__: r1 = 2, r2 = 3, r3 = r1+r2, return r3
        let func_val = make_bcfunc(
            "__main__", 0, 4, 0,
            vec![Value::Int(2), Value::Int(3)],
            vec![
                instr(0, 1, 0, 0),  // LoadConst r1 ← consts[0] = 2
                instr(0, 2, 1, 0),  // LoadConst r2 ← consts[1] = 3
                instr(5, 3, 1, 2),  // Add r3 = r1 + r2
                instr(25, 0, 3, 0), // Return r3
            ],
        );
        let result = run_bytecode_program(&[func_val]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_bad_variant_tag() {
        let bad = Value::Variant("NotAFunc".into(), Box::new(Value::List(vec![])));
        assert!(value_to_bytecode_func(&bad).is_err());
    }

    #[test]
    fn test_unknown_opcode() {
        assert!(int_to_op(99).is_err());
    }

    // --- SEC-15 tests: linker script validation ---

    #[test]
    fn test_validate_linker_script_env_default_when_unset() {
        // When AIRL_LINKER_SCRIPT is not set, should return the default
        std::env::remove_var("AIRL_LINKER_SCRIPT");
        let result = validate_linker_script_env("user.ld");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "user.ld");
    }

    #[test]
    fn test_validate_linker_script_env_nonexistent_path() {
        // When AIRL_LINKER_SCRIPT points to a nonexistent file, should error
        std::env::set_var("AIRL_LINKER_SCRIPT", "/tmp/nonexistent_linker_script_42.ld");
        let result = validate_linker_script_env("user.ld");
        std::env::remove_var("AIRL_LINKER_SCRIPT");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("does not exist"), "error should mention 'does not exist', got: {}", msg);
    }

    #[test]
    fn test_validate_linker_script_env_directory_not_file() {
        // When AIRL_LINKER_SCRIPT points to a directory, should error
        std::env::set_var("AIRL_LINKER_SCRIPT", "/tmp");
        let result = validate_linker_script_env("user.ld");
        std::env::remove_var("AIRL_LINKER_SCRIPT");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("not a regular file"), "error should mention 'not a regular file', got: {}", msg);
    }

    #[test]
    fn test_validate_linker_script_env_valid_file() {
        // When AIRL_LINKER_SCRIPT points to an existing regular file, should succeed
        let test_path = "/tmp/test_linker_script_sec15.ld";
        std::fs::write(test_path, "/* test linker script */").unwrap();
        std::env::set_var("AIRL_LINKER_SCRIPT", test_path);
        let result = validate_linker_script_env("user.ld");
        std::env::remove_var("AIRL_LINKER_SCRIPT");
        let _ = std::fs::remove_file(test_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_path);
    }


    #[cfg(feature = "aot")]
    #[test]
    fn test_compile_with_target_none_uses_host() {
        // A minimal program: __main__ returns nil
        let func_val = make_bcfunc(
            "__main__", 0, 2, 0,
            vec![],
            vec![
                instr(1, 1, 0, 0),  // LoadNil r1
                instr(25, 0, 1, 0), // Return r1
            ],
        );
        let result = compile_bytecode_to_executable_with_target(
            &[func_val], "/tmp/test-host-target", None,
        );
        // Should either succeed or fail with a linking error (NOT "unsupported target")
        match result {
            Ok(()) => {
                let _ = std::fs::remove_file("/tmp/test-host-target");
            }
            Err(e) => {
                let msg = format!("{}", e);
                assert!(!msg.contains("unsupported target"),
                    "target=None should use host path, got: {}", msg);
                assert!(!msg.contains("cross-linker"),
                    "target=None should not use cross-linker, got: {}", msg);
            }
        }
    }

    #[cfg(feature = "aot")]
    #[test]
    fn test_compile_with_invalid_target_returns_err() {
        let func_val = make_bcfunc(
            "__main__", 0, 2, 0,
            vec![],
            vec![
                instr(1, 1, 0, 0),  // LoadNil r1
                instr(25, 0, 1, 0), // Return r1
            ],
        );
        let result = compile_bytecode_to_executable_with_target(
            &[func_val], "/tmp/test-bad-target", Some("fake-unsupported-arch"),
        );
        assert!(result.is_err(), "invalid target should return Err");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.to_lowercase().contains("unsupported") || msg.to_lowercase().contains("unrecognized") || msg.to_lowercase().contains("target"),
            "error should mention unsupported/unrecognized target, got: {}", msg);
    }

    #[cfg(feature = "aot")]
    #[test]
    fn test_compile_with_x86_64_airlos_uses_cross_linker() {
        let func_val = make_bcfunc(
            "__main__", 0, 2, 0,
            vec![],
            vec![
                instr(1, 1, 0, 0),  // LoadNil r1
                instr(25, 0, 1, 0), // Return r1
            ],
        );
        let result = compile_bytecode_to_executable_with_target(
            &[func_val], "/tmp/test-airlos", Some("x86_64-airlos"),
        );
        // In CI without cross-linker, this should fail mentioning x86_64-elf-ld
        // (proving it routed to the cross-linker path, not the host linker)
        match result {
            Ok(()) => {
                // Cross-linker is installed; clean up
                let _ = std::fs::remove_file("/tmp/test-airlos");
            }
            Err(e) => {
                let msg = format!("{}", e);
                assert!(msg.contains("x86_64-elf-ld"),
                    "x86_64-airlos target should route to x86_64-elf-ld cross-linker, got: {}", msg);
            }
        }
    }
}
