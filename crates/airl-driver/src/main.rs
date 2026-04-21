use airl_driver::pipeline::{check_file, format_diagnostic_with_source, PipelineError};
use airl_driver::fmt::format_source;
use airl_agent::transport::Transport;

fn main() {
    // Spawn with larger stack to support deeply nested AIRL evaluation
    // and Cranelift AOT compilation of large programs.
    let stack_size = std::env::var("RUST_MIN_STACK")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(256 * 1024 * 1024); // 256MB default
    let builder = std::thread::Builder::new().stack_size(stack_size);
    let handler = builder.spawn(|| {
        let args: Vec<String> = std::env::args().collect();
        if args.contains(&"--list-builtins".to_string()) {
            print_builtins_json();
            std::process::exit(0);
        }
        match args.get(1).map(|s| s.as_str()) {
            Some("run") => cmd_run(&args[2..]),
            Some("compile") => cmd_compile(&args[2..]),
            Some("check") => cmd_check(&args[2..]),
            Some("repl") => cmd_repl(),
            Some("agent") => cmd_agent(&args[2..]),
            Some("call") => cmd_call(&args[2..]),
            Some("fmt") => cmd_fmt(&args[2..]),
            Some("--version") | Some("-V") => {
                println!("airl {} (built {})", env!("CARGO_PKG_VERSION"), env!("AIRL_BUILD_TIME"));
            }
            _ => print_usage(),
        }
    }).expect("failed to spawn main thread");
    handler.join().expect("main thread panicked");
}

fn cmd_run(args: &[String]) {
    #[cfg(not(feature = "aot"))]
    {
        let _ = args;
        eprintln!("airl run requires AOT compilation support: rebuild with --features aot");
        std::process::exit(1);
    }
    #[cfg(feature = "aot")]
    {
        if args.is_empty() {
            eprintln!("Usage: airl run [--load module.airl ...] [--no-z3-cache] <file.airl> [-- args...]");
            std::process::exit(1);
        }

        // Parse flags: --load, --jit-full (ignored), --no-z3-cache, main file, -- user args
        let mut preloads: Vec<String> = Vec::new();
        let mut main_file: Option<String> = None;
        let mut user_args: Vec<String> = Vec::new();
        let mut past_separator = false;
        let mut i = 0;
        while i < args.len() {
            if past_separator {
                user_args.push(args[i].clone());
                i += 1;
                continue;
            }
            match args[i].as_str() {
                "--load" => {
                    if i + 1 >= args.len() { eprintln!("--load requires a file path"); std::process::exit(1); }
                    preloads.push(args[i + 1].clone());
                    i += 2;
                }
                "--bytecode" => {
                    eprintln!("warning: --bytecode is deprecated and has no effect; airl run is AOT-only");
                    i += 1;
                }
                "--no-z3-cache" => {
                    std::env::set_var("AIRL_NO_Z3_CACHE", "1");
                    i += 1;
                }
                "--strict" => {
                    std::env::set_var("AIRL_STRICT_VERIFY", "1");
                    i += 1;
                }
                "--" => { past_separator = true; user_args.push("--".to_string()); i += 1; }
                _ => {
                    if main_file.is_none() {
                        main_file = Some(args[i].clone());
                    }
                    i += 1;
                }
            }
        }

        let main = match main_file {
            Some(p) => p,
            None => { eprintln!("No input file specified"); std::process::exit(1); }
        };

        fn temp_bin_path() -> std::path::PathBuf {
            use std::time::SystemTime;
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            std::env::temp_dir().join(format!("airl_run_{}_{}", std::process::id(), ts))
        }

        // If --load is used, compile all preloads + main to a temp binary, execute, clean up.
        if !preloads.is_empty() {
            let temp_bin = temp_bin_path();
            let temp_str = temp_bin.to_string_lossy().to_string();

            let mut compile_args: Vec<String> = preloads.clone();
            compile_args.push(main);
            compile_args.push("-o".to_string());
            compile_args.push(temp_str.clone());

            cmd_compile(&compile_args);

            let status = std::process::Command::new(&temp_bin)
                .args(&user_args)
                .status();

            let _ = std::fs::remove_file(&temp_bin);

            match status {
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("error running compiled binary: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // Read source once for import detection and compilation
        let source = std::fs::read_to_string(&main).unwrap_or_else(|e| {
            eprintln!("error: cannot read {}: {}", main, e);
            std::process::exit(1);
        });

        // Check if file uses imports — compile via import-aware AOT path
        if source.contains("(import ") {
            use airl_driver::pipeline::compile_to_object_with_imports;

            let temp_bin = temp_bin_path();
            let temp_str = temp_bin.to_string_lossy().to_string();

            let obj_bytes = match compile_to_object_with_imports(&main, None) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Compilation error: {}", e);
                    std::process::exit(1);
                }
            };

            link_object_to_binary(&obj_bytes, &temp_str, &[main], None);

            let status = std::process::Command::new(&temp_bin)
                .args(&user_args)
                .status();

            let _ = std::fs::remove_file(&temp_bin);

            match status {
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("error running compiled binary: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // No preloads, no imports: compile to temp binary, execute, clean up
        let temp_bin = temp_bin_path();
        let temp_str = temp_bin.to_string_lossy().to_string();

        let mut compile_args: Vec<String> = vec![main];
        compile_args.push("-o".to_string());
        compile_args.push(temp_str.clone());

        cmd_compile(&compile_args);

        // Execute the compiled binary with user args
        let status = std::process::Command::new(&temp_bin)
            .args(&user_args)
            .status();

        let _ = std::fs::remove_file(&temp_bin);

        match status {
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("error running compiled binary: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn cmd_compile(args: &[String]) {
    #[cfg(not(feature = "aot"))]
    {
        let _ = args;
        eprintln!("AOT compilation not available: rebuild with --features aot");
        std::process::exit(1);
    }
    #[cfg(feature = "aot")]
    {
        use airl_driver::pipeline::compile_to_object;

        if args.is_empty() {
            eprintln!("Usage: airl compile [--no-z3-cache] <file.airl ...> [-o output] [--target target]");
            std::process::exit(1);
        }

        // Parse args: files, -o flag, --target flag, --no-z3-cache flag
        let mut files: Vec<String> = Vec::new();
        let mut output = String::from("a.out");
        let mut target: Option<String> = None;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "-o" {
                if i + 1 < args.len() {
                    output = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("-o requires an argument");
                    std::process::exit(1);
                }
            } else if args[i] == "--target" {
                if i + 1 < args.len() {
                    target = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("--target requires an argument (x86-64, i686, i686-airlos, x86_64-airlos, aarch64)");
                    std::process::exit(1);
                }
            } else if args[i] == "--no-z3-cache" {
                std::env::set_var("AIRL_NO_Z3_CACHE", "1");
                i += 1;
            } else if args[i] == "--strict" {
                std::env::set_var("AIRL_STRICT_VERIFY", "1");
                i += 1;
            } else {
                files.push(args[i].clone());
                i += 1;
            }
        }

        if files.is_empty() {
            eprintln!("No input files");
            std::process::exit(1);
        }

        // Check if single file with imports — use import-aware AOT path
        let obj_bytes = if files.len() == 1 {
            let source_check = std::fs::read_to_string(&files[0]).unwrap_or_default();
            if source_check.contains("(import ") {
                use airl_driver::pipeline::compile_to_object_with_imports;
                match compile_to_object_with_imports(&files[0], target.as_deref()) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Compilation error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match compile_to_object(&files, target.as_deref()) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Compilation error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            match compile_to_object(&files, target.as_deref()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Compilation error: {}", e);
                    std::process::exit(1);
                }
            }
        };

        // Link object bytes to final binary
        link_object_to_binary(&obj_bytes, &output, &files, target.as_deref());
    }
}

/// Write object bytes to disk, link with system cc (or cross-linker), produce final binary.
#[cfg(feature = "aot")]
fn link_object_to_binary(obj_bytes: &[u8], output: &str, source_files: &[String], target: Option<&str>) {
    let obj_path = format!("{}.o", output);
    std::fs::write(&obj_path, obj_bytes).unwrap_or_else(|e| {
        eprintln!("Failed to write {}: {}", obj_path, e);
        std::process::exit(1);
    });

    // For freestanding targets, use cross-linker
    if target == Some("i686-airlos") {
        let mut cmd = std::process::Command::new("i686-elf-ld");
        cmd.arg("-T").arg("user.ld");
        cmd.arg(&obj_path);
        cmd.arg("-o").arg(output);
        if let Ok(rt_path) = std::env::var("AIRL_RT_AIRLOS") {
            cmd.arg(&rt_path);
        }
        let status = cmd.status();
        let _ = std::fs::remove_file(&obj_path);
        match status {
            Ok(s) if s.success() => eprintln!("Cross-compiled to {} (i686-airlos)", output),
            Ok(s) => { eprintln!("Cross-linker failed: {:?}", s.code()); std::process::exit(1); }
            Err(e) => { eprintln!("Cross-linker (i686-elf-ld) not found: {}", e); std::process::exit(1); }
        }
        return;
    }

    if target == Some("x86_64-airlos") {
        let mut cmd = std::process::Command::new("x86_64-elf-ld");
        cmd.arg("-T").arg("user64.ld");
        cmd.arg(&obj_path);
        cmd.arg("-o").arg(output);
        if let Ok(rt_path) = std::env::var("AIRL_RT_AIRLOS_X64") {
            cmd.arg(&rt_path);
        }
        let status = cmd.status();
        let _ = std::fs::remove_file(&obj_path);
        match status {
            Ok(s) if s.success() => eprintln!("Cross-compiled to {} (x86_64-airlos)", output),
            Ok(s) => { eprintln!("Cross-linker failed: {:?}", s.code()); std::process::exit(1); }
            Err(e) => { eprintln!("Cross-linker (x86_64-elf-ld) not found: {}", e); std::process::exit(1); }
        }
        return;
    }

    // Find airl-rt static library (embedded or on disk)
    let (rt_lib, runtime_lib) = find_airl_libs();
    let rt_lib = if rt_lib == "-lairl_rt" {
        match airl_runtime::bytecode_aot::extract_embedded_rt() {
            Some(path) => path,
            None => rt_lib,
        }
    } else {
        rt_lib
    };

    // Check if program needs the full runtime (uses run-bytecode or compile-to-executable)
    let needs_runtime = source_files.iter().any(|f| {
        std::fs::read_to_string(f).ok()
            .map(|s| s.contains("run-bytecode") || s.contains("compile-to-executable") || s.contains("run-compiled-bc"))
            .unwrap_or(false)
    });

    // Link with system cc
    let mut cmd = std::process::Command::new("cc");
    cmd.arg(&obj_path)
        .arg("-o")
        .arg(output)
        .arg(&rt_lib);
    // For 32-bit hosted targets, add -m32
    if target == Some("i686") {
        cmd.arg("-m32");
    }
    if needs_runtime && !runtime_lib.is_empty() {
        cmd.arg(&runtime_lib);
        cmd.arg(&rt_lib);
    }
    #[cfg(target_os = "linux")]
    { cmd.arg("-lm").arg("-lpthread").arg("-ldl").arg("-lcurl").arg("-lsqlite3").arg("-lz3"); }
    #[cfg(target_os = "macos")]
    { cmd.arg("-lSystem").arg("-lcurl").arg("-lsqlite3").arg("-lz3"); }

    let status = cmd.status();

    let _ = std::fs::remove_file(&obj_path);

    match status {
        Ok(s) if s.success() => {
            eprintln!("Compiled to {}", output);
        }
        Ok(s) => {
            eprintln!("Linker failed with exit code {:?}", s.code());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run linker (cc): {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "aot")]
fn find_airl_libs() -> (String, String) {
    fn find_lib(name: &str) -> String {
        let candidates = [
            format!("target/release/lib{}.a", name),
            format!("target/debug/lib{}.a", name),
            format!("../target/release/lib{}.a", name),
            format!("../target/debug/lib{}.a", name),
        ];
        for c in &candidates {
            if std::path::Path::new(c).exists() {
                return c.to_string();
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let lib = dir.join(format!("lib{}.a", name));
                if lib.exists() {
                    return lib.to_string_lossy().to_string();
                }
            }
        }
        String::new()
    }

    let rt = find_lib("airl_rt");
    let runtime = find_lib("airl_runtime");
    (
        if rt.is_empty() { "-lairl_rt".to_string() } else { rt },
        runtime,
    )
}

fn cmd_check(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl check [--no-z3-cache] [--strict] <file.airl>");
        std::process::exit(1);
    }

    // Parse flags: --no-z3-cache, --strict
    let mut path_idx = 0;
    for (i, arg) in args.iter().enumerate() {
        if arg == "--no-z3-cache" {
            std::env::set_var("AIRL_NO_Z3_CACHE", "1");
        } else if arg == "--strict" {
            std::env::set_var("AIRL_STRICT_VERIFY", "1");
        } else {
            path_idx = i;
            break;
        }
    }
    let path = &args[path_idx];
    match check_file(path) {
        Ok(()) => println!("OK: {}", path),
        Err(e) => {
            print_pipeline_error(&e, path);
            std::process::exit(1);
        }
    }
}

fn cmd_repl() {
    airl_driver::repl::run_repl();
}

fn cmd_agent(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl agent <file.airl> --listen <endpoint>");
        std::process::exit(1);
    }

    let module_path = &args[0];
    let endpoint_str = find_flag(args, "--listen").unwrap_or_else(|| {
        eprintln!("error: --listen <endpoint> required (e.g., --listen tcp:127.0.0.1:9001)");
        std::process::exit(1);
    });

    let endpoint = airl_agent::runtime::parse_endpoint(&endpoint_str).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    });

    if let Err(e) = airl_agent::runtime::run_agent_loop(module_path, &endpoint) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_call(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: airl call <endpoint> <function> [args...]");
        std::process::exit(1);
    }

    let endpoint_str = &args[0];
    let fn_name = &args[1];
    let fn_args = &args[2..];

    let endpoint = airl_agent::runtime::parse_endpoint(endpoint_str).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    });

    // Parse CLI args to Values
    let arg_values: Vec<airl_runtime::value::Value> = fn_args.iter().map(|s| {
        parse_cli_arg(s)
    }).collect();

    // Build task message
    let task = airl_agent::protocol::TaskMessage {
        id: "call-0".into(),
        from: "cli".into(),
        call: fn_name.clone(),
        args: arg_values,
    };
    let task_str = airl_agent::protocol::serialize_task(&task);

    // Connect and send
    match endpoint {
        airl_agent::identity::Endpoint::Tcp(addr) => {
            let mut transport = airl_agent::tcp_transport::TcpTransport::connect(addr)
                .unwrap_or_else(|e| {
                    eprintln!("error: cannot connect to {}: {}", addr, e);
                    std::process::exit(1);
                });
            transport.send_message(&task_str).unwrap_or_else(|e| {
                eprintln!("error: send failed: {}", e);
                std::process::exit(1);
            });
            let response = transport.recv_message().unwrap_or_else(|e| {
                eprintln!("error: recv failed: {}", e);
                std::process::exit(1);
            });
            transport.close().ok();

            // Parse and display result
            match airl_agent::protocol::parse_result(&response) {
                Ok(result) => {
                    if result.success {
                        if let Some(payload) = result.payload {
                            println!("{}", payload);
                        }
                    } else {
                        eprintln!("error: {}", result.error.unwrap_or_default());
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: bad response: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("error: only TCP endpoints supported for `airl call`");
            std::process::exit(1);
        }
    }
}

fn parse_cli_arg(s: &str) -> airl_runtime::value::Value {
    if let Ok(i) = s.parse::<i64>() {
        return airl_runtime::value::Value::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return airl_runtime::value::Value::Float(f);
    }
    match s {
        "true" => airl_runtime::value::Value::Bool(true),
        "false" => airl_runtime::value::Value::Bool(false),
        "nil" => airl_runtime::value::Value::Nil,
        _ => {
            // Strip quotes if present
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                airl_runtime::value::Value::Str(s[1..s.len()-1].to_string())
            } else {
                airl_runtime::value::Value::Str(s.to_string())
            }
        }
    }
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(i + 1).cloned();
        }
    }
    None
}

fn cmd_fmt(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl fmt <file.airl>");
        std::process::exit(1);
    }
    let path = &args[0];
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {}", path, e);
            std::process::exit(1);
        }
    };
    match format_source(&source) {
        Ok(formatted) => print!("{}", formatted),
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn print_pipeline_error(err: &PipelineError, path: &str) {
    match err {
        PipelineError::Syntax(diag) => {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            eprint!("{}", format_diagnostic_with_source(diag, &source, path));
        }
        PipelineError::Parse(diags) => {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            for diag in diags.errors() {
                eprint!("{}", format_diagnostic_with_source(diag, &source, path));
            }
        }
        PipelineError::TypeCheck(diags) => {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            for diag in diags.errors() {
                eprint!("{}", format_diagnostic_with_source(diag, &source, path));
            }
        }
        PipelineError::Runtime(ref runtime_err) => {
            // For contract violations, show source context
            if let airl_runtime::error::RuntimeError::ContractViolation(ref cv) = runtime_err {
                let source = std::fs::read_to_string(path).unwrap_or_default();
                let diag = airl_syntax::diagnostic::Diagnostic::error(
                    format!("{}", cv),
                    cv.span,
                );
                eprint!("{}", format_diagnostic_with_source(&diag, &source, path));
            } else if let airl_runtime::error::RuntimeError::UseAfterMove { ref name, span } = runtime_err {
                let source = std::fs::read_to_string(path).unwrap_or_default();
                let diag = airl_syntax::diagnostic::Diagnostic::error(
                    format!("use of moved value `{}`", name),
                    *span,
                );
                eprint!("{}", format_diagnostic_with_source(&diag, &source, path));
            } else {
                eprintln!("Runtime error: {}", runtime_err);
            }
        }
        _ => eprintln!("{}", err),
    }
}

fn print_usage() {
    println!(
        "airl {} — The AIRL Language

Usage: airl <command> [args]

Commands:
  run <file>       Compile and run an AIRL source file
  check <file>     Parse and check a file without running
  repl             Start the interactive REPL
  agent <file>     Run an agent worker (--listen <endpoint>)
  call <ep> <fn>   Call a remote agent function
  fmt <file>       Pretty-print an AIRL source file

Options:
  --version, -V    Show version
  --list-builtins  Print JSON description of all registered builtins",
        env!("CARGO_PKG_VERSION")
    );
}

struct BuiltinMeta {
    name: &'static str,
    sig: &'static str,
    doc: &'static str,
    category: &'static str,
}

fn all_builtins() -> Vec<BuiltinMeta> {
    vec![
        // ── Arithmetic ──────────────────────────────────────────────────────
        BuiltinMeta { name: "+", sig: "(a : Int|Float) (b : Int|Float) -> Int|Float", doc: "Add two numbers or concatenate two strings.", category: "math" },
        BuiltinMeta { name: "-", sig: "(a : Int|Float) (b : Int|Float) -> Int|Float", doc: "Subtract b from a.", category: "math" },
        BuiltinMeta { name: "*", sig: "(a : Int|Float) (b : Int|Float) -> Int|Float", doc: "Multiply two numbers.", category: "math" },
        BuiltinMeta { name: "/", sig: "(a : Int|Float) (b : Int|Float) -> Int|Float", doc: "Divide a by b. Integer division truncates toward zero.", category: "math" },
        BuiltinMeta { name: "%", sig: "(a : Int) (b : Int) -> Int", doc: "Integer remainder of a divided by b.", category: "math" },

        // ── Comparison ──────────────────────────────────────────────────────
        BuiltinMeta { name: "=", sig: "(a : Any) (b : Any) -> Bool", doc: "Structural equality: true if a and b are equal.", category: "type" },
        BuiltinMeta { name: "!=", sig: "(a : Any) (b : Any) -> Bool", doc: "Structural inequality: true if a and b differ.", category: "type" },
        BuiltinMeta { name: "<", sig: "(a : Int|Float) (b : Int|Float) -> Bool", doc: "True if a is strictly less than b.", category: "math" },
        BuiltinMeta { name: ">", sig: "(a : Int|Float) (b : Int|Float) -> Bool", doc: "True if a is strictly greater than b.", category: "math" },
        BuiltinMeta { name: "<=", sig: "(a : Int|Float) (b : Int|Float) -> Bool", doc: "True if a is less than or equal to b.", category: "math" },
        BuiltinMeta { name: ">=", sig: "(a : Int|Float) (b : Int|Float) -> Bool", doc: "True if a is greater than or equal to b.", category: "math" },

        // ── Logic ───────────────────────────────────────────────────────────
        BuiltinMeta { name: "and", sig: "(a : Bool) (b : Bool) -> Bool", doc: "Logical AND of two booleans.", category: "type" },
        BuiltinMeta { name: "or", sig: "(a : Bool) (b : Bool) -> Bool", doc: "Logical OR of two booleans.", category: "type" },
        BuiltinMeta { name: "not", sig: "(a : Bool) -> Bool", doc: "Logical NOT of a boolean.", category: "type" },
        BuiltinMeta { name: "xor", sig: "(a : Bool) (b : Bool) -> Bool", doc: "Logical XOR of two booleans.", category: "type" },

        // ── List ────────────────────────────────────────────────────────────
        BuiltinMeta { name: "length", sig: "(list : List|Bytes) -> Int", doc: "Number of elements in a list or bytes buffer.", category: "list" },
        BuiltinMeta { name: "at", sig: "(list : List) (index : Int) -> Any", doc: "Element at zero-based index. Panics if out of bounds.", category: "list" },
        BuiltinMeta { name: "at-or", sig: "(list : List) (index : Int) (default : Any) -> Any", doc: "Element at index, or default if index is out of bounds.", category: "list" },
        BuiltinMeta { name: "set-at", sig: "(list : List) (index : Int) (value : Any) -> List", doc: "Return a new list with the element at index replaced by value.", category: "list" },
        BuiltinMeta { name: "append", sig: "(list : List) (item : Any) -> List", doc: "Return a new list with item appended to the end.", category: "list" },
        BuiltinMeta { name: "head", sig: "(list : List) -> Any", doc: "First element of a list. Panics on empty list.", category: "list" },
        BuiltinMeta { name: "tail", sig: "(list : List) -> List", doc: "All elements except the first. Panics on empty list.", category: "list" },
        BuiltinMeta { name: "empty?", sig: "(list : List) -> Bool", doc: "True if the list has no elements.", category: "list" },
        BuiltinMeta { name: "cons", sig: "(item : Any) (list : List) -> List", doc: "Prepend item to the front of list.", category: "list" },
        BuiltinMeta { name: "list-contains?", sig: "(list : List) (item : Any) -> Bool", doc: "True if list contains an element structurally equal to item.", category: "list" },

        // ── String ──────────────────────────────────────────────────────────
        BuiltinMeta { name: "split", sig: "(s : String) (sep : String) -> List", doc: "Split string s on separator sep, returning a list of substrings.", category: "string" },
        BuiltinMeta { name: "join", sig: "(list : List) (sep : String) -> String", doc: "Join a list of strings with sep as the separator.", category: "string" },
        BuiltinMeta { name: "substring", sig: "(s : String) (start : Int) (end : Int) -> String", doc: "Substring of s from byte offset start (inclusive) to end (exclusive).", category: "string" },
        BuiltinMeta { name: "replace", sig: "(s : String) (from : String) (to : String) -> String", doc: "Replace the first occurrence of from in s with to.", category: "string" },
        BuiltinMeta { name: "char-at", sig: "(s : String) (index : Int) -> String", doc: "Single-character string at the given index (character position, not byte offset).", category: "string" },
        BuiltinMeta { name: "char-count", sig: "(s : String) -> Int", doc: "Number of Unicode characters (code points) in s.", category: "string" },
        BuiltinMeta { name: "char-code", sig: "(s : String) -> Int", doc: "Unicode code point of the first character of a single-character string.", category: "string" },
        BuiltinMeta { name: "char-from-code", sig: "(code : Int) -> String", doc: "Single-character string from a Unicode code point.", category: "string" },
        BuiltinMeta { name: "chars", sig: "(s : String) -> List", doc: "Explode string into a list of single-character strings.", category: "string" },
        BuiltinMeta { name: "char-upper?", sig: "(c : String) -> Bool", doc: "True if the single-character string c is an uppercase letter.", category: "string" },
        BuiltinMeta { name: "char-lower?", sig: "(c : String) -> Bool", doc: "True if the single-character string c is a lowercase letter.", category: "string" },
        BuiltinMeta { name: "string-ci=?", sig: "(a : String) (b : String) -> Bool", doc: "Case-insensitive string equality.", category: "string" },
        BuiltinMeta { name: "str", sig: "(args : Any...) -> String", doc: "Convert one or more values to their string representations and concatenate.", category: "string" },
        BuiltinMeta { name: "format", sig: "(template : String) (args : Any...) -> String", doc: "Printf-style string formatting. Supports %s, %d, %f, %x, %b, %%.", category: "string" },

        // ── Map ─────────────────────────────────────────────────────────────
        BuiltinMeta { name: "map-new", sig: "() -> Map", doc: "Create a new empty map.", category: "map" },
        BuiltinMeta { name: "map-get", sig: "(map : Map) (key : Any) -> Any", doc: "Get the value for key in map. Returns nil if the key is absent.", category: "map" },
        BuiltinMeta { name: "map-set", sig: "(map : Map) (key : Any) (value : Any) -> Map", doc: "Return a new map with key bound to value.", category: "map" },
        BuiltinMeta { name: "map-has", sig: "(map : Map) (key : Any) -> Bool", doc: "True if map contains the given key.", category: "map" },
        BuiltinMeta { name: "map-remove", sig: "(map : Map) (key : Any) -> Map", doc: "Return a new map with the given key removed.", category: "map" },
        BuiltinMeta { name: "map-keys", sig: "(map : Map) -> List", doc: "Return a list of all keys in map.", category: "map" },

        // ── Math ────────────────────────────────────────────────────────────
        BuiltinMeta { name: "sqrt", sig: "(x : Float) -> Float", doc: "Square root of x.", category: "math" },
        BuiltinMeta { name: "sin", sig: "(x : Float) -> Float", doc: "Sine of x (radians).", category: "math" },
        BuiltinMeta { name: "cos", sig: "(x : Float) -> Float", doc: "Cosine of x (radians).", category: "math" },
        BuiltinMeta { name: "tan", sig: "(x : Float) -> Float", doc: "Tangent of x (radians).", category: "math" },
        BuiltinMeta { name: "log", sig: "(x : Float) -> Float", doc: "Natural logarithm of x.", category: "math" },
        BuiltinMeta { name: "exp", sig: "(x : Float) -> Float", doc: "e raised to the power x.", category: "math" },
        BuiltinMeta { name: "floor", sig: "(x : Float) -> Float", doc: "Largest integer not greater than x, as Float.", category: "math" },
        BuiltinMeta { name: "ceil", sig: "(x : Float) -> Float", doc: "Smallest integer not less than x, as Float.", category: "math" },
        BuiltinMeta { name: "round", sig: "(x : Float) -> Float", doc: "Round x to the nearest integer, as Float. Ties go away from zero.", category: "math" },
        BuiltinMeta { name: "int-to-float", sig: "(n : Int) -> Float", doc: "Convert integer n to a floating-point number.", category: "math" },
        BuiltinMeta { name: "float-to-int", sig: "(x : Float) -> Int", doc: "Truncate floating-point x to an integer (toward zero).", category: "math" },
        BuiltinMeta { name: "infinity", sig: "() -> Float", doc: "Positive IEEE 754 infinity.", category: "math" },
        BuiltinMeta { name: "nan", sig: "() -> Float", doc: "IEEE 754 NaN (not-a-number).", category: "math" },
        BuiltinMeta { name: "is-nan?", sig: "(x : Float) -> Bool", doc: "True if x is NaN.", category: "math" },
        BuiltinMeta { name: "is-infinite?", sig: "(x : Float) -> Bool", doc: "True if x is positive or negative infinity.", category: "math" },

        // ── Type / conversion ────────────────────────────────────────────────
        BuiltinMeta { name: "type-of", sig: "(v : Any) -> String", doc: "Return the runtime type name of v as a string (e.g. \"Int\", \"String\", \"List\").", category: "type" },
        BuiltinMeta { name: "valid", sig: "(v : Any) -> Bool", doc: "True if v is not nil (and not an error variant).", category: "type" },
        BuiltinMeta { name: "int-to-string", sig: "(n : Int) -> String", doc: "Decimal string representation of integer n.", category: "type" },
        BuiltinMeta { name: "float-to-string", sig: "(x : Float) -> String", doc: "String representation of floating-point x.", category: "type" },
        BuiltinMeta { name: "string-to-int", sig: "(s : String) -> Int", doc: "Parse decimal integer from string s. Panics on invalid input.", category: "type" },
        BuiltinMeta { name: "string-to-float", sig: "(s : String) -> Float", doc: "Parse floating-point number from string s. Panics on invalid input.", category: "type" },
        BuiltinMeta { name: "int-to-string-radix", sig: "(n : Int) (radix : Int) -> String", doc: "Convert n to a string in the given radix (2–36).", category: "type" },
        BuiltinMeta { name: "parse-int-radix", sig: "(s : String) (radix : Int) -> Int", doc: "Parse string s as an integer in the given radix (2–36).", category: "type" },
        BuiltinMeta { name: "panic", sig: "(msg : String) -> Never", doc: "Abort execution with an error message.", category: "type" },
        BuiltinMeta { name: "assert", sig: "(condition : Bool) (msg : String) -> Nil", doc: "Abort with msg if condition is false.", category: "type" },

        // ── I/O ─────────────────────────────────────────────────────────────
        BuiltinMeta { name: "print", sig: "(v : Any) -> Nil", doc: "Print v to stdout without a trailing newline.", category: "io" },
        BuiltinMeta { name: "println", sig: "(v : Any) -> Nil", doc: "Print v to stdout followed by a newline.", category: "io" },
        BuiltinMeta { name: "eprint", sig: "(v : Any) -> Nil", doc: "Print v to stderr without a trailing newline.", category: "io" },
        BuiltinMeta { name: "eprintln", sig: "(v : Any) -> Nil", doc: "Print v to stderr followed by a newline.", category: "io" },
        BuiltinMeta { name: "write-file", sig: "(path : String) (content : String|Bytes) -> Nil", doc: "Write content to the file at path, creating or truncating it.", category: "io" },
        BuiltinMeta { name: "append-file", sig: "(path : String) (content : String|Bytes) -> Nil", doc: "Append content to the file at path, creating it if necessary.", category: "io" },
        BuiltinMeta { name: "delete-file", sig: "(path : String) -> Nil", doc: "Delete the file at path.", category: "io" },
        BuiltinMeta { name: "rename-file", sig: "(src : String) (dst : String) -> Nil", doc: "Rename (move) the file at src to dst.", category: "io" },
        BuiltinMeta { name: "file-exists?", sig: "(path : String) -> Bool", doc: "True if a file or directory exists at path.", category: "io" },
        BuiltinMeta { name: "file-size", sig: "(path : String) -> Int", doc: "Size of the file at path in bytes.", category: "io" },
        BuiltinMeta { name: "file-mtime", sig: "(path : String) -> Int", doc: "Last-modified time of the file at path as Unix timestamp (seconds).", category: "io" },
        BuiltinMeta { name: "exec-file", sig: "(path : String) -> Nil", doc: "Execute the file at path (exec, replaces current process).", category: "io" },
        BuiltinMeta { name: "read-dir", sig: "(path : String) -> List", doc: "List the entries of directory at path as a list of filename strings.", category: "io" },
        BuiltinMeta { name: "create-dir", sig: "(path : String) -> Nil", doc: "Create a directory (and any missing parents) at path.", category: "io" },
        BuiltinMeta { name: "delete-dir", sig: "(path : String) -> Nil", doc: "Recursively delete the directory at path.", category: "io" },
        BuiltinMeta { name: "is-dir?", sig: "(path : String) -> Bool", doc: "True if path is an existing directory.", category: "io" },
        BuiltinMeta { name: "temp-file", sig: "(suffix : String) -> String", doc: "Create a temporary file with the given suffix and return its path.", category: "io" },
        BuiltinMeta { name: "temp-dir", sig: "(prefix : String) -> String", doc: "Create a temporary directory with the given prefix and return its path.", category: "io" },
        BuiltinMeta { name: "read-line", sig: "() -> String", doc: "Read one line from stdin (blocking). Returns the line without trailing newline.", category: "io" },
        BuiltinMeta { name: "read-lines", sig: "(path : String) -> List", doc: "Read all lines of a text file at path as a list of strings.", category: "io" },
        BuiltinMeta { name: "read-stdin", sig: "() -> String", doc: "Read all of stdin as a string.", category: "io" },
        BuiltinMeta { name: "get-cwd", sig: "() -> String", doc: "Return the current working directory as a string.", category: "io" },

        // ── System ──────────────────────────────────────────────────────────
        BuiltinMeta { name: "sleep", sig: "(ms : Int) -> Nil", doc: "Sleep for ms milliseconds.", category: "system" },
        BuiltinMeta { name: "time-now", sig: "() -> Int", doc: "Current Unix timestamp in milliseconds.", category: "system" },
        BuiltinMeta { name: "cpu-count", sig: "() -> Int", doc: "Number of logical CPU cores available.", category: "system" },
        BuiltinMeta { name: "format-time", sig: "(timestamp_ms : Int) (fmt : String) -> String", doc: "Format a Unix timestamp (milliseconds) using a strftime-style format string.", category: "system" },
        BuiltinMeta { name: "shell-exec", sig: "(cmd : String) (stdin : String) -> Map", doc: "Run cmd in a shell, passing stdin as input. Returns a map with keys stdout, stderr, exit_code.", category: "system" },
        BuiltinMeta { name: "shell-exec-with-stdin", sig: "(cmd : String) (args : List) (stdin : String) -> Map", doc: "Run cmd with explicit args list and stdin. Returns a map with keys stdout, stderr, exit_code.", category: "system" },

        // ── Regex ───────────────────────────────────────────────────────────
        BuiltinMeta { name: "regex-match", sig: "(pattern : String) (s : String) -> Bool", doc: "True if the regular expression pattern matches anywhere in s.", category: "regex" },
        BuiltinMeta { name: "regex-find-all", sig: "(pattern : String) (s : String) -> List", doc: "Return all non-overlapping matches of pattern in s as a list of strings.", category: "regex" },
        BuiltinMeta { name: "regex-replace", sig: "(pattern : String) (replacement : String) (s : String) -> String", doc: "Replace the first match of pattern in s with replacement. Supports $1 capture groups.", category: "regex" },
        BuiltinMeta { name: "regex-split", sig: "(pattern : String) (s : String) -> List", doc: "Split s on every match of the regular expression pattern.", category: "regex" },

        // ── Bytes ───────────────────────────────────────────────────────────
        BuiltinMeta { name: "bytes-alloc", sig: "(n : Int) -> Bytes", doc: "Allocate a zero-filled byte buffer of length n.", category: "bytes" },
        BuiltinMeta { name: "bytes-new", sig: "() -> Bytes", doc: "Create a new empty byte buffer.", category: "bytes" },
        BuiltinMeta { name: "bytes-get", sig: "(buf : Bytes) (index : Int) -> Int", doc: "Return the byte value at index as an integer (0–255).", category: "bytes" },
        BuiltinMeta { name: "bytes-set!", sig: "(buf : Bytes) (index : Int) (value : Int) -> Nil", doc: "Set the byte at index to value (0–255) in-place.", category: "bytes" },
        BuiltinMeta { name: "bytes-length", sig: "(buf : Bytes) -> Int", doc: "Number of bytes in the buffer.", category: "bytes" },
        BuiltinMeta { name: "bytes-from-string", sig: "(s : String) -> Bytes", doc: "Convert a UTF-8 string to a byte buffer.", category: "bytes" },
        BuiltinMeta { name: "bytes-to-string", sig: "(buf : Bytes) (start : Int) (end : Int) -> String", doc: "Decode a slice of buf as a UTF-8 string from byte offset start to end.", category: "bytes" },
        BuiltinMeta { name: "bytes-concat", sig: "(a : Bytes) (b : Bytes) -> Bytes", doc: "Concatenate two byte buffers.", category: "bytes" },
        BuiltinMeta { name: "bytes-concat-all", sig: "(list : List) -> Bytes", doc: "Concatenate a list of byte buffers into one.", category: "bytes" },
        BuiltinMeta { name: "bytes-slice", sig: "(buf : Bytes) (start : Int) (end : Int) -> Bytes", doc: "Return a new byte buffer containing buf[start..end].", category: "bytes" },
        BuiltinMeta { name: "bytes-from-int8", sig: "(n : Int) -> Bytes", doc: "1-byte buffer containing n as a signed 8-bit integer.", category: "bytes" },
        BuiltinMeta { name: "bytes-from-int16", sig: "(n : Int) -> Bytes", doc: "2-byte buffer containing n as a little-endian signed 16-bit integer.", category: "bytes" },
        BuiltinMeta { name: "bytes-from-int32", sig: "(n : Int) -> Bytes", doc: "4-byte buffer containing n as a little-endian signed 32-bit integer.", category: "bytes" },
        BuiltinMeta { name: "bytes-from-int64", sig: "(n : Int) -> Bytes", doc: "8-byte buffer containing n as a little-endian signed 64-bit integer.", category: "bytes" },
        BuiltinMeta { name: "bytes-to-int16", sig: "(buf : Bytes) (offset : Int) -> Int", doc: "Read a little-endian signed 16-bit integer from buf at byte offset.", category: "bytes" },
        BuiltinMeta { name: "bytes-to-int32", sig: "(buf : Bytes) (offset : Int) -> Int", doc: "Read a little-endian signed 32-bit integer from buf at byte offset.", category: "bytes" },
        BuiltinMeta { name: "bytes-to-int64", sig: "(buf : Bytes) (offset : Int) -> Int", doc: "Read a little-endian signed 64-bit integer from buf at byte offset.", category: "bytes" },
        BuiltinMeta { name: "bytes-xor", sig: "(a : Bytes) (b : Bytes) -> Bytes", doc: "XOR two equal-length byte buffers element-wise.", category: "bytes" },
        BuiltinMeta { name: "bytes-xor-scalar", sig: "(buf : Bytes) (byte : Int) -> Bytes", doc: "XOR every byte in buf with the scalar value byte.", category: "bytes" },

        // ── Crypto ──────────────────────────────────────────────────────────
        BuiltinMeta { name: "sha256", sig: "(data : String) -> String", doc: "Hex-encoded SHA-256 digest of a UTF-8 string.", category: "crypto" },
        BuiltinMeta { name: "sha512", sig: "(data : String) -> String", doc: "Hex-encoded SHA-512 digest of a UTF-8 string.", category: "crypto" },
        BuiltinMeta { name: "hmac-sha256", sig: "(key : String) (data : String) -> String", doc: "Hex-encoded HMAC-SHA-256 of data using key.", category: "crypto" },
        BuiltinMeta { name: "hmac-sha512", sig: "(key : String) (data : String) -> String", doc: "Hex-encoded HMAC-SHA-512 of data using key.", category: "crypto" },
        BuiltinMeta { name: "sha256-bytes", sig: "(data : Bytes) -> Bytes", doc: "Raw 32-byte SHA-256 digest of a byte buffer.", category: "crypto" },
        BuiltinMeta { name: "sha512-bytes", sig: "(data : Bytes) -> Bytes", doc: "Raw 64-byte SHA-512 digest of a byte buffer.", category: "crypto" },
        BuiltinMeta { name: "hmac-sha256-bytes", sig: "(key : Bytes) (data : Bytes) -> Bytes", doc: "Raw 32-byte HMAC-SHA-256 of data using key bytes.", category: "crypto" },
        BuiltinMeta { name: "hmac-sha512-bytes", sig: "(key : Bytes) (data : Bytes) -> Bytes", doc: "Raw 64-byte HMAC-SHA-512 of data using key bytes.", category: "crypto" },
        BuiltinMeta { name: "pbkdf2-sha512", sig: "(password : Bytes) (salt : Bytes) (iterations : Int) (key_len : Int) -> Bytes", doc: "PBKDF2 key derivation using HMAC-SHA-512.", category: "crypto" },
        BuiltinMeta { name: "random-bytes", sig: "(n : Int) -> Bytes", doc: "Cryptographically secure random byte buffer of length n.", category: "crypto" },
        BuiltinMeta { name: "crc32c", sig: "(data : Bytes) -> Int", doc: "CRC-32C (Castagnoli) checksum of a byte buffer.", category: "crypto" },

        // ── Bitwise ─────────────────────────────────────────────────────────
        BuiltinMeta { name: "bitwise-and", sig: "(a : Int) (b : Int) -> Int", doc: "Bitwise AND of two integers.", category: "bitwise" },
        BuiltinMeta { name: "bitwise-or", sig: "(a : Int) (b : Int) -> Int", doc: "Bitwise OR of two integers.", category: "bitwise" },
        BuiltinMeta { name: "bitwise-xor", sig: "(a : Int) (b : Int) -> Int", doc: "Bitwise XOR of two integers.", category: "bitwise" },
        BuiltinMeta { name: "bitwise-shl", sig: "(a : Int) (shift : Int) -> Int", doc: "Left-shift a by shift bits.", category: "bitwise" },
        BuiltinMeta { name: "bitwise-shr", sig: "(a : Int) (shift : Int) -> Int", doc: "Arithmetic right-shift a by shift bits.", category: "bitwise" },

        // ── Misc ────────────────────────────────────────────────────────────
        BuiltinMeta { name: "format-time", sig: "(timestamp_ms : Int) (fmt : String) -> String", doc: "Format a Unix timestamp in milliseconds using a strftime-style format string.", category: "misc" },
        BuiltinMeta { name: "thread-spawn", sig: "(fn : (fn [] -> Any)) -> Int", doc: "Spawn a new thread running fn (a zero-argument closure) and return its handle.", category: "misc" },
        BuiltinMeta { name: "thread-join", sig: "(handle : Int) -> Any", doc: "Block until the thread with handle completes and return its result.", category: "misc" },
        BuiltinMeta { name: "thread-set-affinity", sig: "(cpu : Int) -> Nil", doc: "Pin the calling thread to logical CPU cpu.", category: "misc" },
        BuiltinMeta { name: "channel-new", sig: "() -> Channel", doc: "Create a new unbounded MPSC channel.", category: "misc" },
        BuiltinMeta { name: "channel-send", sig: "(ch : Channel) (value : Any) -> Nil", doc: "Send a value into channel ch.", category: "misc" },
        BuiltinMeta { name: "channel-recv", sig: "(ch : Channel) -> Any", doc: "Receive the next value from channel ch, blocking until one is available.", category: "misc" },
        BuiltinMeta { name: "channel-recv-timeout", sig: "(ch : Channel) (timeout_ms : Int) -> Any", doc: "Receive from ch with a timeout in milliseconds. Returns nil on timeout.", category: "misc" },
        BuiltinMeta { name: "channel-drain", sig: "(ch : Channel) -> List", doc: "Non-blocking drain: return all currently queued values as a list.", category: "misc" },
        BuiltinMeta { name: "channel-close", sig: "(ch : Channel) -> Nil", doc: "Close channel ch.", category: "misc" },
        BuiltinMeta { name: "dns-resolve", sig: "(hostname : String) -> List", doc: "Resolve hostname to a list of IP address strings.", category: "misc" },
        BuiltinMeta { name: "icmp-ping", sig: "(host : String) (timeout_ms : Int) -> Bool", doc: "Send an ICMP echo request to host. Returns true if a reply is received within timeout.", category: "misc" },
        BuiltinMeta { name: "tcp-connect", sig: "(host : String) (port : Int) -> TcpSocket", doc: "Open a TCP connection to host:port.", category: "misc" },
        BuiltinMeta { name: "tcp-close", sig: "(sock : TcpSocket) -> Nil", doc: "Close a TCP socket.", category: "misc" },
        BuiltinMeta { name: "tcp-send", sig: "(sock : TcpSocket) (data : Bytes) -> Nil", doc: "Send data over a TCP socket.", category: "misc" },
        BuiltinMeta { name: "tcp-recv", sig: "(sock : TcpSocket) (max_bytes : Int) -> Bytes", doc: "Receive up to max_bytes from a TCP socket.", category: "misc" },
        BuiltinMeta { name: "tcp-recv-exact", sig: "(sock : TcpSocket) (n : Int) -> Bytes", doc: "Receive exactly n bytes from a TCP socket, blocking until available.", category: "misc" },
        BuiltinMeta { name: "tcp-set-timeout", sig: "(sock : TcpSocket) (timeout_ms : Int) -> Nil", doc: "Set read/write timeout on a TCP socket in milliseconds.", category: "misc" },
        BuiltinMeta { name: "tcp-listen", sig: "(host : String) (port : Int) -> TcpListener", doc: "Bind a TCP listener to host:port.", category: "misc" },
        BuiltinMeta { name: "tcp-accept", sig: "(listener : TcpListener) -> TcpSocket", doc: "Accept the next incoming TCP connection, blocking.", category: "misc" },
        BuiltinMeta { name: "gzip-compress", sig: "(data : Bytes) -> Bytes", doc: "Compress data with gzip.", category: "misc" },
        BuiltinMeta { name: "gzip-decompress", sig: "(data : Bytes) -> Bytes", doc: "Decompress gzip-compressed data.", category: "misc" },
        BuiltinMeta { name: "snappy-compress", sig: "(data : Bytes) -> Bytes", doc: "Compress data with Snappy.", category: "misc" },
        BuiltinMeta { name: "snappy-decompress", sig: "(data : Bytes) -> Bytes", doc: "Decompress Snappy-compressed data.", category: "misc" },
        BuiltinMeta { name: "lz4-compress", sig: "(data : Bytes) -> Bytes", doc: "Compress data with LZ4.", category: "misc" },
        BuiltinMeta { name: "lz4-decompress", sig: "(data : Bytes) -> Bytes", doc: "Decompress LZ4-compressed data.", category: "misc" },
        BuiltinMeta { name: "zstd-compress", sig: "(data : Bytes) -> Bytes", doc: "Compress data with Zstandard.", category: "misc" },
        BuiltinMeta { name: "zstd-decompress", sig: "(data : Bytes) -> Bytes", doc: "Decompress Zstandard-compressed data.", category: "misc" },
        BuiltinMeta { name: "fn-metadata", sig: "(fn : Any) -> Map", doc: "Return a map of metadata (name, arity, source) about a function value.", category: "misc" },
        BuiltinMeta { name: "compile-to-executable", sig: "(sources : List) (output : String) -> Nil", doc: "Compile a list of AIRL source file paths to a standalone executable.", category: "misc" },
        BuiltinMeta { name: "run-bytecode", sig: "(bytecode : Bytes) -> Any", doc: "Execute pre-compiled AIRL bytecode and return its result.", category: "misc" },
        BuiltinMeta { name: "ash-install-sigint", sig: "() -> Nil", doc: "Install a SIGINT handler for the ash REPL (captures Ctrl-C without exiting).", category: "misc" },
        BuiltinMeta { name: "ash-sigint-pending", sig: "() -> Bool", doc: "True if a SIGINT has been received since the last call to this function.", category: "misc" },
    ]
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

fn print_builtins_json() {
    let version = env!("CARGO_PKG_VERSION");
    let builtins = all_builtins();
    // Deduplicate by name (format-time appears in both system and misc)
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<&BuiltinMeta> = builtins.iter().filter(|b| seen.insert(b.name)).collect();

    println!("{{");
    println!("  \"version\": {},", json_escape(version));
    println!("  \"builtins\": [");
    let last = unique.len().saturating_sub(1);
    for (i, b) in unique.iter().enumerate() {
        let comma = if i < last { "," } else { "" };
        println!("    {{\"name\": {}, \"sig\": {}, \"doc\": {}, \"category\": {}}}{}",
            json_escape(b.name),
            json_escape(b.sig),
            json_escape(b.doc),
            json_escape(b.category),
            comma,
        );
    }
    println!("  ]");
    println!("}}");
}
