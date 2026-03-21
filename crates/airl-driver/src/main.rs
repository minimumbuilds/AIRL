use airl_driver::pipeline::{run_file, check_file, format_diagnostic_with_source, PipelineError};
use airl_driver::fmt::format_source;
use airl_agent::transport::Transport;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[2..]),
        Some("check") => cmd_check(&args[2..]),
        Some("repl") => cmd_repl(),
        Some("agent") => cmd_agent(&args[2..]),
        Some("call") => cmd_call(&args[2..]),
        Some("fmt") => cmd_fmt(&args[2..]),
        Some("--version") | Some("-V") => println!("airl 0.1.0"),
        _ => print_usage(),
    }
}

fn cmd_run(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl run <file.airl>");
        std::process::exit(1);
    }
    let path = &args[0];
    match run_file(path) {
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
        _ => eprintln!("{}", err),
    }
}

fn print_usage() {
    println!(
        "airl 0.1.0 — The AIRL Language

Usage: airl <command> [args]

Commands:
  run <file>       Run an AIRL source file
  check <file>     Parse and check a file without running
  repl             Start the interactive REPL
  agent <file>     Run an agent worker (--listen <endpoint>)
  call <ep> <fn>   Call a remote agent function
  fmt <file>       Pretty-print an AIRL source file

Options:
  --version, -V  Show version"
    );
}
