use airl_driver::pipeline::{run_file, run_file_bytecode, check_file, format_diagnostic_with_source, PipelineError};
#[cfg(feature = "jit")]
use airl_driver::pipeline::run_file_jit_full;
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
            Some("--version") | Some("-V") => println!("airl 0.2.0"),
            _ => print_usage(),
        }
    }).expect("failed to spawn main thread");
    handler.join().expect("main thread panicked");
}

fn cmd_run(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl run [--jit|--jit-full|--interpreted] <file.airl>");
        std::process::exit(1);
    }

    let (mode, path) = match args[0].as_str() {
        "--interpreted" => {
            if args.len() < 2 { eprintln!("Usage: airl run --interpreted <file.airl>"); std::process::exit(1); }
            ("interpreted", &args[1])
        }
        "--bytecode" => {
            if args.len() < 2 { eprintln!("Usage: airl run --bytecode <file.airl>"); std::process::exit(1); }
            ("bytecode", &args[1])
        }
        "--jit-full" => {
            if args.len() < 2 { eprintln!("Usage: airl run --jit-full <file.airl>"); std::process::exit(1); }
            ("jit-full", &args[1])
        }
        _ => ("default", &args[0]),
    };

    let result = match mode {
        "interpreted" => run_file(path),
        "bytecode" => run_file_bytecode(path),
        #[cfg(feature = "jit")]
        "jit-full" => run_file_jit_full(path),
        _ => {
            // Default: run_file uses jit-full when JIT feature is enabled
            // (with full type checking, linearity analysis, and Z3 verification).
            // Falls back to pure bytecode when JIT feature is not enabled.
            run_file(path)
        }
    };

    match result {
        Ok(val) => {
            // Only print non-unit results
            if !matches!(val, airl_runtime::value::Value::Unit) {
                println!("{}", val);
            }
        }
        Err(e) => {
            print_pipeline_error(&e, path);
            std::process::exit(1);
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

        // Compile to object file
        let obj_bytes = match compile_to_object(&files) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Compilation error: {}", e);
                std::process::exit(1);
            }
        };

        // Write object file
        let obj_path = format!("{}.o", output);
        std::fs::write(&obj_path, &obj_bytes).unwrap_or_else(|e| {
            eprintln!("Failed to write {}: {}", obj_path, e);
            std::process::exit(1);
        });

        // Find airl-rt static library
        let (rt_lib, runtime_lib) = find_airl_libs();

        // Link with system cc
        let mut cmd = std::process::Command::new("cc");
        cmd.arg(&obj_path)
            .arg("-o")
            .arg(&output)
            .arg(&rt_lib);
        // Also link airl-runtime if available (provides run-bytecode for self-hosting)
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
            // airl-runtime depends on airl-rt, so re-add it for symbol resolution order
            cmd.arg(&rt_lib);
        }
        let status = cmd
            .arg("-lm")
            .arg("-lpthread")
            .arg("-ldl")
            .status();

        // Clean up object file
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
    if rt.is_empty() {
        eprintln!("Warning: could not find libairl_rt.a");
    }
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
        "airl 0.2.0 — The AIRL Language

Usage: airl <command> [args]

Commands:
  run <file>       Run an AIRL source file (JIT-compiled with contracts)
  check <file>     Parse and check a file without running
  repl             Start the interactive REPL
  agent <file>     Run an agent worker (--listen <endpoint>)
  call <ep> <fn>   Call a remote agent function
  fmt <file>       Pretty-print an AIRL source file

Options:
  --version, -V  Show version"
    );
}
