use airl_syntax::*;
use airl_syntax::parser;
use airl_runtime::eval::Interpreter;
use airl_runtime::value::Value;
use airl_runtime::error::RuntimeError;
use airl_types::checker::TypeChecker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Check,  // type errors block execution
    Run,    // type errors warn to stderr, execution proceeds
    Repl,   // type errors warn to stderr, execution proceeds
}

pub fn run_source_with_mode(source: &str, mode: PipelineMode) -> Result<Value, PipelineError> {
    // Lex
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    // Parse all top-level forms
    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(_) => return Err(PipelineError::Syntax(d)),
                }
            }
        }
    }
    if diags.has_errors() {
        return Err(PipelineError::Parse(diags));
    }

    // Type check
    let mut checker = TypeChecker::new();
    for top in &tops {
        let _ = checker.check_top_level(top);
    }
    if checker.has_errors() {
        let type_diags = checker.into_diagnostics();
        match mode {
            PipelineMode::Check => return Err(PipelineError::TypeCheck(type_diags)),
            PipelineMode::Run | PipelineMode::Repl => {
                // Print as warnings to stderr, don't block
                for d in type_diags.errors() {
                    eprintln!("warning: {}", d.message);
                }
            }
        }
    }

    // Z3 contract verification
    let z3_prover = airl_solver::prover::Z3Prover::new();
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            let verification = z3_prover.verify_function(f);
            for (clause, result) in &verification.ensures_results {
                match result {
                    airl_solver::VerifyResult::Proven => {
                        if mode == PipelineMode::Check {
                            eprintln!("note: `{}` contract proven: {}", f.name, clause);
                        }
                    }
                    airl_solver::VerifyResult::Disproven { counterexample } => {
                        let msg = format!("contract disproven in `{}`: {} (counterexample: {:?})",
                            f.name, clause, counterexample);
                        match mode {
                            PipelineMode::Check => eprintln!("error: {}", msg),
                            _ => eprintln!("warning: {}", msg),
                        }
                    }
                    airl_solver::VerifyResult::Unknown(_) | airl_solver::VerifyResult::TranslationError(_) => {
                        // Silent — fall back to runtime checking
                    }
                }
            }
        }
    }

    // Evaluate
    let mut interp = Interpreter::new();
    let mut result = Value::Unit;
    for top in &tops {
        result = interp.eval_top_level(top).map_err(PipelineError::Runtime)?;
    }
    Ok(result)
}

pub fn run_source(source: &str) -> Result<Value, PipelineError> {
    run_source_with_mode(source, PipelineMode::Run)
}

pub fn run_file(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source(&source)
}

pub fn check_source(source: &str) -> Result<(), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(_) => {}
        }
    }
    if diags.has_errors() {
        return Err(PipelineError::Parse(diags));
    }

    // Type check (strict mode)
    let mut checker = TypeChecker::new();
    for top in &tops {
        let _ = checker.check_top_level(top);
    }
    if checker.has_errors() {
        return Err(PipelineError::TypeCheck(checker.into_diagnostics()));
    }

    // Z3 contract verification
    let z3_prover = airl_solver::prover::Z3Prover::new();
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            let verification = z3_prover.verify_function(f);
            for (clause, result) in &verification.ensures_results {
                match result {
                    airl_solver::VerifyResult::Proven => {
                        eprintln!("note: `{}` contract proven: {}", f.name, clause);
                    }
                    airl_solver::VerifyResult::Disproven { counterexample } => {
                        eprintln!("error: contract disproven in `{}`: {} (counterexample: {:?})",
                            f.name, clause, counterexample);
                    }
                    airl_solver::VerifyResult::Unknown(_) | airl_solver::VerifyResult::TranslationError(_) => {
                        // Silent — fall back to runtime checking
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn check_file(path: &str) -> Result<(), PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    check_source(&source)
}

#[derive(Debug)]
pub enum PipelineError {
    Io(String),
    Syntax(Diagnostic),
    Parse(Diagnostics),
    TypeCheck(Diagnostics),
    Runtime(RuntimeError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Io(msg) => write!(f, "IO error: {}", msg),
            PipelineError::Syntax(d) => write!(f, "Syntax error: {}", d.message),
            PipelineError::Parse(ds) => {
                for d in ds.errors() {
                    writeln!(f, "Parse error: {}", d.message)?;
                }
                Ok(())
            }
            PipelineError::TypeCheck(ds) => {
                for d in ds.errors() {
                    writeln!(f, "Type error: {}", d.message)?;
                }
                Ok(())
            }
            PipelineError::Runtime(e) => write!(f, "Runtime error: {}", e),
        }
    }
}

// ── Error formatting with source context ─────────────────

pub fn format_diagnostic_with_source(diag: &Diagnostic, source: &str, filename: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_num = diag.span.line as usize;
    let col = diag.span.col as usize;

    let severity = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };

    let mut output = format!(
        "{}: {}\n  --> {}:{}:{}\n",
        severity, diag.message, filename, line_num, col
    );

    if line_num > 0 && line_num <= lines.len() {
        let line = lines[line_num - 1];
        output.push_str(&format!("   |\n{:>3} | {}\n   |", line_num, line));
        output.push_str(&format!("{}^\n", " ".repeat(col + 1)));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_simple_expression() {
        let result = run_source("(+ 1 2)").unwrap();
        match result {
            Value::Int(n) => assert_eq!(n, 3),
            other => panic!("expected Int(3), got {:?}", other),
        }
    }

    #[test]
    fn run_defn_and_call() {
        let source = r#"
            (defn add
              :sig [(x : i32) (y : i32) -> i32]
              :intent "Add two numbers"
              :requires [(valid x) (valid y)]
              :ensures [(= result (+ x y))]
              :body (+ x y))
            (add 3 4)
        "#;
        let result = run_source(source).unwrap();
        match result {
            Value::Int(n) => assert_eq!(n, 7),
            other => panic!("expected Int(7), got {:?}", other),
        }
    }

    #[test]
    fn check_valid_source() {
        assert!(check_source("(+ 1 2)").is_ok());
    }

    #[test]
    fn check_invalid_source() {
        assert!(check_source("(").is_err());
    }

    #[test]
    fn run_file_not_found() {
        let err = run_file("/nonexistent/path.airl").unwrap_err();
        match err {
            PipelineError::Io(_) => {}
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[test]
    fn format_error_with_context() {
        let diag = Diagnostic::error(
            "unexpected token",
            airl_syntax::Span::new(4, 5, 1, 4),
        );
        let source = "(+ 1 !)";
        let formatted = format_diagnostic_with_source(&diag, source, "test.airl");
        assert!(formatted.contains("error: unexpected token"));
        assert!(formatted.contains("test.airl:1:4"));
        assert!(formatted.contains("(+ 1 !)"));
        assert!(formatted.contains("^"));
    }

    #[test]
    fn format_warning() {
        let diag = Diagnostic::warning(
            "unused variable",
            airl_syntax::Span::new(0, 1, 1, 0),
        );
        let source = "x";
        let formatted = format_diagnostic_with_source(&diag, source, "test.airl");
        assert!(formatted.contains("warning: unused variable"));
    }

    #[test]
    fn pipeline_error_display() {
        let err = PipelineError::Io("file not found".to_string());
        assert_eq!(format!("{}", err), "IO error: file not found");
    }

    #[test]
    fn check_source_with_type_checker() {
        // Valid source should pass check
        let result = check_source("(+ 1 2)");
        assert!(result.is_ok());
    }
}
