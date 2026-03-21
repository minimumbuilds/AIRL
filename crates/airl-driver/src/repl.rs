use std::io::{self, Write, BufRead};
use airl_runtime::eval::Interpreter;
use airl_runtime::value::Value;

fn print_env(interp: &Interpreter) {
    let bindings = interp.env.iter_bindings();

    let values: Vec<(&str, &airl_runtime::env::Slot)> = bindings
        .into_iter()
        .filter(|(_, slot)| !matches!(slot.value, Value::BuiltinFn(_)))
        .collect();

    if values.is_empty() {
        println!("(no user bindings)");
        return;
    }

    let (functions, others): (Vec<_>, Vec<_>) = values
        .into_iter()
        .partition(|(_, slot)| matches!(slot.value, Value::Function(_)));

    if !others.is_empty() {
        println!("── Bindings ──");
        for (name, slot) in &others {
            if slot.moved {
                println!("  {} = {} [moved]", name, slot.value);
            } else {
                println!("  {} = {}", name, slot.value);
            }
        }
    }

    if !functions.is_empty() {
        println!("── Functions ──");
        for (name, slot) in &functions {
            if let Value::Function(f) = &slot.value {
                let params: Vec<&str> = f.def.params.iter().map(|p| p.name.as_str()).collect();
                let param_str = format!("({})", params.join(", "));
                let ret = format!("{:?}", f.def.return_type.kind);
                println!("  {} : {} -> {}", name, param_str, ret);
            }
        }
    }
}

pub fn run_repl() {
    let stdin = io::stdin();
    let mut input = String::new();
    let mut interp = Interpreter::new();

    println!("AIRL v0.1.0 — Type :quit to exit");

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
        if trimmed == ":env" {
            print_env(&interp);
            continue;
        }

        input.push_str(&line);
        if !parens_balanced(&input) {
            continue;
        }

        match eval_repl_input(&input, &mut interp) {
            Ok(val) => println!("{}", val),
            Err(e) => eprintln!("error: {}", e),
        }
        input.clear();
    }
}

fn eval_repl_input(
    input: &str,
    interp: &mut Interpreter,
) -> Result<Value, String> {
    let mut lexer = airl_syntax::Lexer::new(input);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens).map_err(|d| d.message)?;
    let mut diags = airl_syntax::Diagnostics::new();
    let mut result = Value::Unit;

    for sexpr in &sexprs {
        match airl_syntax::parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                result = interp
                    .eval_top_level(&top)
                    .map_err(|e| format!("{}", e))?;
            }
            Err(_) => {
                let mut diags2 = airl_syntax::Diagnostics::new();
                match airl_syntax::parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => {
                        result = interp.eval(&expr).map_err(|e| format!("{}", e))?;
                    }
                    Err(d) => return Err(d.message),
                }
            }
        }
    }
    Ok(result)
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
        let mut interp = Interpreter::new();
        let result = eval_repl_input("(+ 10 20)", &mut interp).unwrap();
        assert_eq!(format!("{}", result), "30");
    }

    #[test]
    fn eval_repl_then_env() {
        let mut interp = Interpreter::new();
        let input = r#"
            (defn greet
              :sig [(name : String) -> String]
              :intent "greet"
              :requires [(valid name)]
              :ensures [(valid result)]
              :body name)
        "#;
        eval_repl_input(input, &mut interp).unwrap();
        let bindings = interp.env.iter_bindings();
        let has_greet = bindings.iter().any(|(name, slot)| {
            *name == "greet" && matches!(slot.value, Value::Function(_))
        });
        assert!(has_greet);
    }
}
