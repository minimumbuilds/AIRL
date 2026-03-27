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
        eprintln!("airl run requires AOT compilation support: rebuild with --features jit,aot");
        std::process::exit(1);
    }
    #[cfg(feature = "aot")]
    {
        if args.is_empty() {
            eprintln!("Usage: airl run [--load module.airl ...] <file.airl> [-- args...]");
            std::process::exit(1);
        }

        // Parse flags: --load, --jit-full (ignored), main file, -- user args
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
                "--jit-full" | "--bytecode" => { i += 1; } // ignored, compile path only
                "--" => { past_separator = true; i += 1; }
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

        // If --load is used, we need the VM path: --load modules must be executed
        // sequentially to register their functions before the main file runs.
        // This is required for G3 bootstrap (--load lexer/parser/bc_compiler).
        if !preloads.is_empty() {
            use airl_driver::pipeline::run_file_with_preloads;
            let result = run_file_with_preloads(&main, &preloads);
            match result {
                Ok(val) => {
                    if !matches!(val, airl_runtime::value::Value::Unit) {
                        println!("{}", val);
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            return;
        }

        // Check if file uses imports — use import-aware pipeline
        let source_check = std::fs::read_to_string(&main).unwrap_or_default();
        if source_check.contains("(import ") {
            use airl_driver::pipeline::run_file_with_imports;
            let result = run_file_with_imports(&main);
            match result {
                Ok(val) => {
                    if !matches!(val, airl_runtime::value::Value::Unit) {
                        println!("{}", val);
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            return;
        }

        // No preloads: compile to temp binary, execute, clean up
        let temp_bin = std::env::temp_dir().join(format!("airl_run_{}", std::process::id()));
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
            eprintln!("Usage: airl compile <file.airl ...> [-o output]");
            std::process::exit(1);
        }

        // Parse args: files and -o flag
        let mut files: Vec<String> = Vec::new();
        let mut output = String::from("a.out");
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
                match compile_to_object_with_imports(&files[0]) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Compilation error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match compile_to_object(&files) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Compilation error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            match compile_to_object(&files) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Compilation error: {}", e);
                    std::process::exit(1);
                }
            }
        };

        // Link object bytes to final binary
        link_object_to_binary(&obj_bytes, &output, &files);
    }
}

/// Write object bytes to disk, link with system cc, produce final binary.
#[cfg(feature = "aot")]
fn link_object_to_binary(obj_bytes: &[u8], output: &str, source_files: &[String]) {
    let obj_path = format!("{}.o", output);
    std::fs::write(&obj_path, obj_bytes).unwrap_or_else(|e| {
        eprintln!("Failed to write {}: {}", obj_path, e);
        std::process::exit(1);
    });

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
    if needs_runtime && !runtime_lib.is_empty() {
        cmd.arg(&runtime_lib);
        cmd.arg(&rt_lib);
    }
    let status = cmd
        .arg("-lm")
        .arg("-lpthread")
        .arg("-ldl")
        .arg("-lcurl")
        .status();

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
        eprintln!("Usage: airl check <file.airl>");
        std::process::exit(1);
    }
    let path = &args[0];
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
        "airl 0.6.1 — The AIRL Language

Usage: airl <command> [args]

Commands:
  run <file>       Compile and run an AIRL source file
  check <file>     Parse and check a file without running
  repl             Start the interactive REPL
  agent <file>     Run an agent worker (--listen <endpoint>)
  call <ep> <fn>   Call a remote agent function
  fmt <file>       Pretty-print an AIRL source file

Options:
  --version, -V  Show version"
    );
}
