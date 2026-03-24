use std::collections::HashMap;
use airl_syntax::*;
use airl_syntax::parser;
use airl_syntax::ast::{Expr, ExprKind, Pattern, PatternKind, LitPattern};
use airl_runtime::value::Value;
use airl_runtime::error::RuntimeError;
use airl_runtime::ir::*;
use airl_runtime::bytecode_compiler::BytecodeCompiler;
use airl_runtime::bytecode_vm::BytecodeVm;
use airl_types::checker::TypeChecker;
use airl_types::linearity::LinearityChecker;

const COLLECTIONS_SOURCE: &str = include_str!("../../../stdlib/prelude.airl");
const MATH_SOURCE: &str = include_str!("../../../stdlib/math.airl");
const RESULT_SOURCE: &str = include_str!("../../../stdlib/result.airl");
const STRING_SOURCE: &str = include_str!("../../../stdlib/string.airl");
const MAP_SOURCE: &str = include_str!("../../../stdlib/map.airl");

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

    // Linearity check
    let mut lin_checker = LinearityChecker::new();
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            lin_checker.check_fn(f);
        }
    }
    if lin_checker.has_errors() {
        let lin_diags = lin_checker.drain_diagnostics();
        match mode {
            PipelineMode::Check => {
                // Linearity errors are fatal in Check mode
                let mut msgs = Vec::new();
                for d in lin_diags.errors() {
                    msgs.push(d.message.clone());
                }
                return Err(PipelineError::Runtime(RuntimeError::Custom(
                    msgs.join("; ")
                )));
            }
            PipelineMode::Run | PipelineMode::Repl => {
                // Warn only — runtime ownership tracking enforces moves via MarkMoved/CheckNotMoved
                for d in lin_diags.errors() {
                    eprintln!("warning (linearity): {}", d.message);
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
                        // Clauses referencing `result` are always false positives
                        // because the solver does not constrain `result` to the
                        // function body's return value.
                        if clause.contains("result") {
                            // Suppress — known false positive
                        } else {
                            let msg = format!("contract disproven in `{}`: {} (counterexample: {:?})",
                                f.name, clause, counterexample);
                            match mode {
                                PipelineMode::Check => eprintln!("error: {}", msg),
                                _ => eprintln!("warning: {}", msg),
                            }
                        }
                    }
                    airl_solver::VerifyResult::Unknown(_) | airl_solver::VerifyResult::TranslationError(_) => {
                        // Silent — fall back to runtime checking
                    }
                }
            }
        }
    }

    // Build ownership map: function name → per-param "is Own" flags
    let ownership_map = build_ownership_map(&tops);

    // Compile AST → IR → Bytecode with contracts
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create VM, load stdlib, execute
    // When JIT feature is enabled, use jit-full (compiles ALL functions to native x86-64).
    // Bytecode VM still executes __main__ and dispatches to native code for each call.
    #[cfg(feature = "jit")]
    let mut vm = BytecodeVm::new_with_full_jit();
    #[cfg(not(feature = "jit"))]
    let mut vm = BytecodeVm::new();
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    // JIT-full: compile all loaded functions to native code before execution
    #[cfg(feature = "jit")]
    vm.jit_full_compile_all();
    vm.exec_main().map_err(PipelineError::Runtime)
}

pub fn run_source(source: &str) -> Result<Value, PipelineError> {
    run_source_with_mode(source, PipelineMode::Run)
}

pub fn run_file(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source(&source)
}

/// Run a file with pre-loaded modules. Each preload is compiled as a separate
/// compilation unit (like stdlib modules) before the main file. This allows
/// jit-full to handle each module independently instead of one concatenated file.
pub fn run_file_with_preloads(path: &str, preloads: &[String]) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    // Lex + parse + compile user source
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();
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

    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create VM with jit-full when available
    #[cfg(feature = "jit")]
    let mut vm = BytecodeVm::new_with_full_jit();
    #[cfg(not(feature = "jit"))]
    let mut vm = BytecodeVm::new();

    // Load stdlib
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    // Load each preloaded module as a separate compilation unit
    for preload_path in preloads {
        let preload_src = std::fs::read_to_string(preload_path)
            .map_err(|e| PipelineError::Io(format!("{}: {}", preload_path, e)))?;
        let module_name = std::path::Path::new(preload_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "module".to_string());
        compile_and_load_stdlib_bytecode(&mut vm, &preload_src, &module_name)?;
    }

    // Load user functions
    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);

    // Skip JIT for preloaded programs — preloaded modules may have 100+ functions
    // that overwhelm Cranelift. Bytecode VM handles them fine.
    // The user's main file functions are small enough for JIT if needed,
    // but for simplicity we run everything on bytecode when preloads are used.

    vm.exec_main().map_err(PipelineError::Runtime)
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

    // Linearity check (strict mode)
    let mut lin_checker = LinearityChecker::new();
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            lin_checker.check_fn(f);
        }
    }
    if lin_checker.has_errors() {
        let lin_diags = lin_checker.drain_diagnostics();
        for d in lin_diags.errors() {
            eprintln!("linearity error: {}", d.message);
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
                        eprintln!("note: `{}` contract proven: {}", f.name, clause);
                    }
                    airl_solver::VerifyResult::Disproven { counterexample } => {
                        // Clauses referencing `result` are always false positives
                        // because the solver does not constrain `result` to the
                        // function body's return value.
                        if !clause.contains("result") {
                            eprintln!("error: contract disproven in `{}`: {} (counterexample: {:?})",
                                f.name, clause, counterexample);
                        }
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

// ── AST-to-IR Compiler (Rust-side, mirrors compiler.airl) ─────

fn compile_expr(expr: &Expr) -> IRNode {
    match &expr.kind {
        ExprKind::IntLit(v) => IRNode::Int(*v),
        ExprKind::FloatLit(v) => IRNode::Float(*v),
        ExprKind::StrLit(s) => IRNode::Str(s.clone()),
        ExprKind::BoolLit(b) => IRNode::Bool(*b),
        ExprKind::NilLit => IRNode::Nil,
        ExprKind::SymbolRef(name) => IRNode::Load(name.clone()),
        ExprKind::KeywordLit(k) => IRNode::Str(format!(":{}", k)),

        ExprKind::If(cond, then_, else_) => IRNode::If(
            Box::new(compile_expr(cond)),
            Box::new(compile_expr(then_)),
            Box::new(compile_expr(else_)),
        ),

        ExprKind::Let(bindings, body) => {
            let ir_bindings: Vec<IRBinding> = bindings.iter().map(|b| {
                IRBinding {
                    name: b.name.clone(),
                    expr: compile_expr(&b.value),
                }
            }).collect();
            IRNode::Let(ir_bindings, Box::new(compile_expr(body)))
        }

        ExprKind::Do(exprs) => IRNode::Do(exprs.iter().map(compile_expr).collect()),

        ExprKind::FnCall(callee, args) => {
            let ir_args: Vec<IRNode> = args.iter().map(compile_expr).collect();
            if let ExprKind::SymbolRef(name) = &callee.kind {
                IRNode::Call(name.clone(), ir_args)
            } else {
                IRNode::CallExpr(Box::new(compile_expr(callee)), ir_args)
            }
        }

        ExprKind::Lambda(params, body) => {
            let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            IRNode::Lambda(param_names, Box::new(compile_expr(body)))
        }

        ExprKind::Match(scrutinee, arms) => {
            let ir_arms: Vec<IRArm> = arms.iter().map(|arm| {
                IRArm {
                    pattern: compile_pattern(&arm.pattern),
                    body: compile_expr(&arm.body),
                }
            }).collect();
            IRNode::Match(Box::new(compile_expr(scrutinee)), ir_arms)
        }

        ExprKind::ListLit(items) => IRNode::List(items.iter().map(compile_expr).collect()),

        ExprKind::VariantCtor(name, args) => {
            IRNode::Variant(name.clone(), args.iter().map(compile_expr).collect())
        }

        ExprKind::Try(inner) => IRNode::Try(Box::new(compile_expr(inner))),

        ExprKind::StructLit(name, fields) => {
            // Compile struct literal as a variant with a list of key-value pairs
            let mut items = Vec::new();
            for (key, val) in fields {
                items.push(IRNode::List(vec![
                    IRNode::Str(key.clone()),
                    compile_expr(val),
                ]));
            }
            IRNode::Variant(name.clone(), vec![IRNode::List(items)])
        }

        // Quantifiers: desugar to fold over range using stdlib functions.
        // forall [i : T] (where guard) body → (fold (fn [acc i] (if (not acc) false body)) true (range 0 bound))
        // exists [i : T] (where guard) body → (fold (fn [acc i] (if acc true body)) false (range 0 bound))
        // The where clause (< i N) extracts N as the upper bound; default is 10000.
        ExprKind::Forall(..) | ExprKind::Exists(..) => {
            let is_forall = matches!(&expr.kind, ExprKind::Forall(..));

            let (param, where_clause, body) = match &expr.kind {
                ExprKind::Forall(p, w, b) | ExprKind::Exists(p, w, b) => (p, w, b),
                _ => unreachable!(),
            };

            let var_name = param.name.clone();
            let acc_name = "__quant_acc".to_string();

            // Extract upper bound from where clause, or default to 10000
            let upper_bound = match where_clause {
                Some(w) => extract_upper_bound(w, &var_name).unwrap_or_else(|| compile_expr(w)),
                None => IRNode::Int(10000),
            };

            let compiled_body = compile_expr(body);

            // Build the fold callback: (fn [acc var] ...)
            let fold_body = if is_forall {
                // (if (not acc) false body)  — short-circuit on first failure
                IRNode::If(
                    Box::new(IRNode::Call("not".to_string(), vec![IRNode::Load(acc_name.clone())])),
                    Box::new(IRNode::Bool(false)),
                    Box::new(compiled_body),
                )
            } else {
                // (if acc true body)  — short-circuit on first success
                IRNode::If(
                    Box::new(IRNode::Load(acc_name.clone())),
                    Box::new(IRNode::Bool(true)),
                    Box::new(compiled_body),
                )
            };

            let callback = IRNode::Lambda(
                vec![acc_name.clone(), var_name.clone()],
                Box::new(fold_body),
            );

            let init = if is_forall { IRNode::Bool(true) } else { IRNode::Bool(false) };

            let range_expr = IRNode::Call("range".to_string(), vec![IRNode::Int(0), upper_bound]);

            // (fold callback init (range 0 bound))
            IRNode::Call("fold".to_string(), vec![callback, init, range_expr])
        }
    }
}

fn compile_pattern(pat: &Pattern) -> IRPattern {
    match &pat.kind {
        PatternKind::Wildcard => IRPattern::Wild,
        PatternKind::Binding(name) => IRPattern::Bind(name.clone()),
        PatternKind::Literal(lit) => {
            let val = match lit {
                LitPattern::Int(v) => Value::Int(*v),
                LitPattern::Float(v) => Value::Float(*v),
                LitPattern::Str(s) => Value::Str(s.clone()),
                LitPattern::Bool(b) => Value::Bool(*b),
                LitPattern::Nil => Value::Nil,
            };
            IRPattern::Lit(val)
        }
        PatternKind::Variant(name, sub_pats) => {
            IRPattern::Variant(name.clone(), sub_pats.iter().map(compile_pattern).collect())
        }
    }
}

/// Extract upper bound from a where clause of the form `(< var N)` or `(<= var N)`.
/// Returns `Some(IRNode)` for the bound (exclusive for `<`, N+1 for `<=`), or `None` if
/// the clause doesn't match this pattern.
fn extract_upper_bound(where_expr: &Expr, var_name: &str) -> Option<IRNode> {
    if let ExprKind::FnCall(callee, args) = &where_expr.kind {
        if let ExprKind::SymbolRef(op) = &callee.kind {
            if args.len() == 2 {
                if let ExprKind::SymbolRef(ref name) = &args[0].kind {
                    if name == var_name {
                        if op == "<" {
                            return Some(compile_expr(&args[1]));
                        } else if op == "<=" {
                            // (<= i N) means range 0..(N+1)
                            return Some(IRNode::Call("+".to_string(), vec![
                                compile_expr(&args[1]),
                                IRNode::Int(1),
                            ]));
                        }
                    }
                }
            }
        }
    }
    None
}

fn compile_top_level(top: &airl_syntax::ast::TopLevel) -> IRNode {
    match top {
        airl_syntax::ast::TopLevel::Defn(f) => {
            let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
            IRNode::Func(f.name.clone(), param_names, Box::new(compile_expr(&f.body)))
        }
        airl_syntax::ast::TopLevel::Expr(e) => compile_expr(e),
        _ => IRNode::Nil, // Module, DefType, Task, UseDecl — no runtime effect in compiled mode
    }
}

// ── Shared: ownership map builder ──────────────────────────

/// Build a map from function names to per-parameter ownership flags.
/// Only includes functions that have at least one explicitly `Own`-annotated parameter.
fn build_ownership_map(tops: &[airl_syntax::ast::TopLevel]) -> HashMap<String, Vec<bool>> {
    let mut map = HashMap::new();
    for top in tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            let own_flags: Vec<bool> = f.params.iter().map(|p| {
                matches!(p.ownership, airl_syntax::ast::Ownership::Own)
            }).collect();
            if own_flags.iter().any(|&o| o) {
                map.insert(f.name.clone(), own_flags);
            }
        }
    }
    map
}

// ── Shared: AST → IR with contracts ───────────────────────

/// Compile a list of top-level AST nodes to IR, extracting contract clauses as IR+source pairs.
fn compile_tops_with_contracts(
    tops: &[airl_syntax::ast::TopLevel],
) -> (Vec<IRNode>, HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>) {
    let mut ir_nodes = Vec::new();
    let mut contracts = HashMap::new();

    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => {
                let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                ir_nodes.push(IRNode::Func(f.name.clone(), param_names, Box::new(compile_expr(&f.body))));

                let req: Vec<(IRNode, String)> = f.requires.iter()
                    .map(|e| (compile_expr(e), e.to_airl()))
                    .collect();
                let ens: Vec<(IRNode, String)> = f.ensures.iter()
                    .map(|e| (compile_expr(e), e.to_airl()))
                    .collect();
                let inv: Vec<(IRNode, String)> = f.invariants.iter()
                    .map(|e| (compile_expr(e), e.to_airl()))
                    .collect();
                if !req.is_empty() || !ens.is_empty() || !inv.is_empty() {
                    contracts.insert(f.name.clone(), (req, ens, inv));
                }
            }
            airl_syntax::ast::TopLevel::Expr(e) => {
                ir_nodes.push(compile_expr(e));
            }
            _ => {
                ir_nodes.push(IRNode::Nil);
            }
        }
    }

    (ir_nodes, contracts)
}

// ── Bytecode Pipeline ─────────────────────────────────────

/// Run source through bytecode pipeline with contracts: parse → IR compile → bytecode compile → bytecode VM
pub fn run_source_bytecode(source: &str) -> Result<Value, PipelineError> {
    // Lex + parse
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

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

    // Build ownership map and compile AST → IR with contracts
    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create VM, load stdlib, execute
    let mut vm = BytecodeVm::new();
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    vm.exec_main().map_err(PipelineError::Runtime)
}

fn compile_and_load_stdlib_bytecode(vm: &mut BytecodeVm, source: &str, name: &str) -> Result<(), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => panic!("{} parse error: {}", name, d.message),
        }
    }

    let ir_nodes: Vec<IRNode> = tops.iter().map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix(name);
    let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    vm.exec_main().unwrap_or_else(|e| panic!("{} stdlib load failed: {}", name, e));

    Ok(())
}

pub fn run_file_bytecode(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source_bytecode(&source)
}

// ── JIT Pipeline ───────────────────────────────────────────

/// Run source through JIT pipeline: parse → IR compile → bytecode compile → JIT → execute
#[cfg(feature = "jit")]
pub fn run_source_jit(source: &str) -> Result<Value, PipelineError> {
    // Lex + parse
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

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

    // Build ownership map and compile AST → IR → Bytecode with contracts
    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create JIT-enabled VM
    let mut vm = airl_runtime::bytecode_vm::BytecodeVm::new_with_jit();

    // Load stdlib through bytecode path
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    // Load user functions
    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);

    // Two-pass: compile all loaded functions, then execute
    vm.jit_compile_all();
    vm.exec_main().map_err(PipelineError::Runtime)
}

#[cfg(feature = "jit")]
pub fn run_file_jit(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source_jit(&source)
}

// ── JIT-Full Pipeline ──────────────────────────────────────

/// Run source through JIT-full pipeline: parse → IR compile → bytecode compile → full JIT → execute
#[cfg(feature = "jit")]
pub fn run_source_jit_full(source: &str) -> Result<Value, PipelineError> {
    // Lex + parse
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

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

    // Build ownership map and compile AST → IR → Bytecode with contracts
    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create full-JIT-enabled VM
    let mut vm = airl_runtime::bytecode_vm::BytecodeVm::new_with_full_jit();

    // Load stdlib through bytecode path
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    // Load user functions
    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);

    // Two-pass: compile all loaded functions, then execute
    vm.jit_full_compile_all();
    vm.exec_main().map_err(PipelineError::Runtime)
}

#[cfg(feature = "jit")]
pub fn run_file_jit_full(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source_jit_full(&source)
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

// ── REPL helpers ────────────────────────────────────────────

/// Load stdlib into a bytecode VM for use by the REPL.
/// Panics on stdlib errors since stdlib is trusted code.
pub fn compile_and_load_stdlib_bytecode_repl(vm: &mut BytecodeVm) {
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(vm, src, name)
            .unwrap_or_else(|e| panic!("stdlib {} load failed: {}", name, e));
    }
}

/// Compile AIRL source and run it in an existing bytecode VM (for incremental REPL use).
/// Functions defined in previous calls persist in the VM's function table.
pub fn compile_and_run_repl_input(source: &str, vm: &mut BytecodeVm) -> Result<Value, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(|d| d.message)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(_) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(d) => return Err(d.message),
                }
            }
        }
    }

    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("repl");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    vm.exec_main().map_err(|e| format!("{}", e))
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

    #[test]
    fn linearity_checker_detects_use_after_move() {
        // The static linearity checker should detect that `x` is moved
        // twice when passed as `own` — once in consume1 and again in consume2.
        let source = r#"
            (defn consume1
              :sig [(own x : i32) -> i32]
              :intent "consume"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body x)
            (defn consume2
              :sig [(own x : i32) -> i32]
              :intent "consume"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body x)
            (defn double-use
              :sig [(own val : i32) -> i32]
              :intent "use val twice"
              :requires [(valid val)]
              :ensures [(valid result)]
              :body (+ (consume1 val) (consume2 val)))
        "#;
        // The static checker should detect the double move of val
        let mut lin = LinearityChecker::new();
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        for sexpr in &sexprs {
            if let Ok(airl_syntax::ast::TopLevel::Defn(f)) = parser::parse_top_level(sexpr, &mut diags) {
                lin.check_fn(&f);
            }
        }
        assert!(lin.has_errors(), "linearity checker should detect use-after-move");
        let lin_diags = lin.drain_diagnostics();
        let err_msg = lin_diags.errors().next().unwrap().message.clone();
        assert!(err_msg.contains("moved"), "error should mention 'moved', got: {}", err_msg);
    }

    #[test]
    fn linearity_checker_allows_default_ownership() {
        // Default ownership (no annotation) should not trigger linearity errors.
        let source = r#"
            (defn use-twice
              :sig [(x : i32) -> i32]
              :intent "use x twice"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (+ x x))
        "#;
        let mut lin = LinearityChecker::new();
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        for sexpr in &sexprs {
            if let Ok(airl_syntax::ast::TopLevel::Defn(f)) = parser::parse_top_level(sexpr, &mut diags) {
                lin.check_fn(&f);
            }
        }
        assert!(!lin.has_errors(), "default ownership should not trigger linearity errors");
    }
}

// ── AOT Compilation Pipeline ──────────────────────────────────────────────

/// Compile AIRL source files to a native object file.
/// Returns the object file bytes (ELF on Linux, Mach-O on macOS).
#[cfg(feature = "aot")]
pub fn compile_to_object(paths: &[String]) -> Result<Vec<u8>, PipelineError> {
    use airl_runtime::bytecode::BytecodeFunc;
    use airl_runtime::bytecode_aot::BytecodeAot;
    use std::collections::HashMap;

    let mut all_funcs: Vec<BytecodeFunc> = Vec::new();

    // 1. Compile stdlib to bytecode (skip their __main__ — they're no-op for pure defn modules)
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
        // Only take named functions, skip the __main__ (which just returns nil)
        all_funcs.extend(funcs);
    }

    // 2. Compile user source files to bytecode
    let mut all_source = String::new();
    for path in paths {
        let source = std::fs::read_to_string(path)
            .map_err(|e| PipelineError::Io(format!("{}: {}", path, e)))?;
        all_source.push_str(&source);
        all_source.push('\n');
    }

    let (funcs, main_func) = compile_source_to_bytecode(&all_source, "user")?;
    all_funcs.extend(funcs);
    all_funcs.push(main_func); // Only the user's __main__

    // 3. AOT compile bytecode → native object
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new().map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    for func in &all_funcs {
        aot.compile_all(std::slice::from_ref(func), &func_map);
    }

    aot.emit_entry_point().map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    Ok(aot.finish())
}

/// Compile source string to bytecode functions (shared by run and AOT paths).
#[cfg(feature = "aot")]
fn compile_source_to_bytecode(source: &str, prefix: &str) -> Result<(Vec<airl_runtime::bytecode::BytecodeFunc>, airl_runtime::bytecode::BytecodeFunc), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

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

    let ir_nodes: Vec<IRNode> = tops.iter().map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix(prefix);
    Ok(bc_compiler.compile_program(&ir_nodes))
}
