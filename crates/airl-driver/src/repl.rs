use std::io::{self, Write, BufRead};
use airl_runtime::value::Value;
use airl_runtime::bytecode_vm::BytecodeVm;
use airl_types::checker::TypeChecker;
use crate::pipeline::{compile_and_load_stdlib_bytecode_repl, compile_and_run_repl_input};

fn print_help() {
    println!("AIRL REPL Commands:");
    println!("  :help          Show this help message");
    println!("  :quit / :q     Exit the REPL");
    println!("  :type <expr>   Show the type of an expression without evaluating");
    println!("  :load <file>   Load and evaluate an AIRL source file");
    println!();
    println!("Enter any AIRL expression to evaluate it.");
    println!("Multi-line input is supported — keep typing until parens balance.");
}

fn repl_type_check(input: &str, tc: &mut TypeChecker) {
    let mut lexer = airl_syntax::Lexer::new(input);
    let tokens = match lexer.lex_all() {
        Ok(t) => t,
        Err(d) => { eprintln!("error: {}", d.message); return; }
    };
    let sexprs = match airl_syntax::parse_sexpr_all(tokens) {
        Ok(s) => s,
        Err(d) => { eprintln!("error: {}", d.message); return; }
    };

    for sexpr in &sexprs {
        let mut diags = airl_syntax::Diagnostics::new();
        let expr = match airl_syntax::parser::parse_expr(sexpr, &mut diags) {
            Ok(e) => e,
            Err(d) => { eprintln!("error: {}", d.message); return; }
        };

        match tc.check_expr(&expr) {
            Ok(ty) => println!("{}", ty),
            Err(_) => {
                let d = tc.drain_diagnostics();
                for diag in d.iter() {
                    eprintln!("type error: {}", diag.message);
                }
            }
        }
    }
}

pub fn run_repl() {
    let stdin = io::stdin();
    let mut input = String::new();
    let mut tc = TypeChecker::new();

    // Create bytecode VM and load stdlib
    let mut vm = BytecodeVm::new();
    if let Err(e) = compile_and_load_stdlib_bytecode_repl(&mut vm) {
        eprintln!("fatal: stdlib load failed: {}", e);
        return;
    }

    println!("AIRL v{} — Type :help for commands, :quit to exit", env!("CARGO_PKG_VERSION"));

    loop {
        let prompt = if input.is_empty() { "airl> " } else { "...   " };
        eprint!("{}", prompt);
        io::stderr().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed == ":quit" || trimmed == ":q" {
            break;
        }
        if trimmed == ":help" || trimmed == ":h" {
            print_help();
            continue;
        }
        if let Some(expr_str) = trimmed.strip_prefix(":type ") {
            repl_type_check(expr_str.trim(), &mut tc);
            continue;
        }
        if let Some(path) = trimmed.strip_prefix(":load ") {
            let path = path.trim();
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => { eprintln!("error: cannot load '{}': {}", path, e); continue; }
            };
            match compile_and_run_repl_input(&source, &mut vm) {
                Ok(val) => {
                    if val != Value::Unit {
                        println!("{}", val);
                    }
                    println!("loaded {}", path);
                }
                Err(e) => eprintln!("error in {}: {}", path, e),
            }
            continue;
        }

        input.push_str(&line);
        if !parens_balanced(&input) {
            continue;
        }

        match compile_and_run_repl_input(&input, &mut vm) {
            Ok(val) => println!("{}", val),
            Err(e) => eprintln!("error: {}", e),
        }
        input.clear();
    }
}

fn parens_balanced(input: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for ch in input.chars() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
    }
    depth <= 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_runtime::bytecode_vm::BytecodeVm;
    use crate::pipeline::compile_and_load_stdlib_bytecode_repl;

    #[test]
    fn balanced_simple() {
        assert!(parens_balanced("(+ 1 2)"));
    }

    #[test]
    fn unbalanced_open() {
        assert!(!parens_balanced("(+ 1"));
    }

    #[test]
    fn balanced_nested() {
        assert!(parens_balanced("(+ (* 2 3) 4)"));
    }

    #[test]
    fn string_parens_ignored() {
        assert!(parens_balanced(r#"(print "(hello")"#));
    }

    #[test]
    fn escaped_quote_in_string() {
        assert!(parens_balanced(r#"(print "escaped\"paren(")"#));
    }

    #[test]
    fn empty_is_balanced() {
        assert!(parens_balanced(""));
    }

    #[test]
    fn bracket_balanced() {
        assert!(parens_balanced("[1 2 3]"));
    }

    #[test]
    fn bracket_unbalanced() {
        assert!(!parens_balanced("[1 2"));
    }

    #[test]
    fn eval_repl_simple() {
        let mut vm = BytecodeVm::new();
        compile_and_load_stdlib_bytecode_repl(&mut vm).expect("stdlib load failed");
        let result = compile_and_run_repl_input("(+ 10 20)", &mut vm).unwrap();
        assert_eq!(format!("{}", result), "30");
    }

    #[test]
    fn eval_repl_defn() {
        let mut vm = BytecodeVm::new();
        compile_and_load_stdlib_bytecode_repl(&mut vm).expect("stdlib load failed");
        let input = r#"
            (defn double
              :sig [(x : i32) -> i32]
              :intent "double"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (* x 2))
        "#;
        compile_and_run_repl_input(input, &mut vm).unwrap();
        // Call the defined function
        let result = compile_and_run_repl_input("(double 21)", &mut vm).unwrap();
        assert_eq!(format!("{}", result), "42");
    }
}
