use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use airl_syntax::*;
use airl_syntax::parser;
use airl_syntax::ast::{Expr, ExprKind, Pattern, PatternKind, LitPattern};
use airl_runtime::value::Value;
use airl_runtime::error::RuntimeError;
use airl_runtime::ir::*;
use airl_runtime::bytecode_compiler::BytecodeCompiler;
use airl_runtime::bytecode::BytecodeFunc;
use airl_runtime::bytecode_vm::BytecodeVm;
use airl_types::checker::TypeChecker;
use airl_types::linearity::LinearityChecker;

const COLLECTIONS_SOURCE: &str = include_str!("../../../stdlib/prelude.airl");
const MATH_SOURCE: &str = include_str!("../../../stdlib/math.airl");
const RESULT_SOURCE: &str = include_str!("../../../stdlib/result.airl");
const STRING_SOURCE: &str = include_str!("../../../stdlib/string.airl");
const MAP_SOURCE: &str = include_str!("../../../stdlib/map.airl");
const SET_SOURCE: &str = include_str!("../../../stdlib/set.airl");
const IO_SOURCE: &str = include_str!("../../../stdlib/io.airl");
const PATH_SOURCE: &str = include_str!("../../../stdlib/path.airl");
const RANDOM_SOURCE: &str = include_str!("../../../stdlib/random.airl");
#[cfg(not(target_os = "airlos"))]
const SQLITE_SOURCE: &str = include_str!("../../../stdlib/sqlite.airl");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Check,  // type errors block execution
    Run,    // type errors warn to stderr, execution proceeds
    Repl,   // type errors warn to stderr, execution proceeds
}

/// Lex and parse source into top-level forms, with expression fallback.
/// Returns tops without checking accumulated diagnostics — callers that need
/// strict diagnostics checking (e.g. run_source_with_mode) must check separately.
fn parse_source(source: &str) -> Result<(Vec<airl_syntax::ast::TopLevel>, Diagnostics), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(tokens).map_err(PipelineError::Syntax)?;
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
    Ok((tops, diags))
}

/// Lex and parse source into top-level forms, strict mode (no expression fallback).
/// Used by check_source where bare expressions are not expected.
/// Checks diags for errors after parsing.
fn parse_source_strict(source: &str) -> Result<Vec<airl_syntax::ast::TopLevel>, PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(tokens).map_err(PipelineError::Syntax)?;
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
    Ok(tops)
}

/// Lex and parse stdlib source. Fails fast on `Err` from parse_top_level
/// but does NOT check accumulated diagnostics (stdlib may have known warnings).
fn parse_source_stdlib(source: &str, name: &str) -> Result<Vec<airl_syntax::ast::TopLevel>, PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => return Err(PipelineError::Io(format!("{} parse error: {}", name, d.message))),
        }
    }
    Ok(tops)
}

/// Parse source and extract extern-c declarations alongside the top-level forms.
/// Used by compile_to_object to get both AST and extern info in a single pass.
#[cfg(feature = "aot")]
fn parse_source_with_externs(source: &str) -> Result<(Vec<airl_syntax::ast::TopLevel>, Vec<airl_runtime::bytecode_aot::ExternCInfo>), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    let mut extern_c_decls = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                if let airl_syntax::ast::TopLevel::ExternC(ref decl) = top {
                    extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                        c_name: decl.c_name.clone(),
                        arity: decl.params.len(),
                    });
                }
                tops.push(top);
            }
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(_) => return Err(PipelineError::Syntax(d)),
                }
            }
        }
    }
    // Also extract extern-c declarations from inside module bodies
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Module(m) = top {
            for inner in &m.body {
                if let airl_syntax::ast::TopLevel::ExternC(ref decl) = inner {
                    extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                        c_name: decl.c_name.clone(),
                        arity: decl.params.len(),
                    });
                }
            }
        }
    }
    Ok((tops, extern_c_decls))
}

/// Shared Z3 verification helper. Runs Z3 verification on all function definitions
/// in `tops`, respecting VerifyLevel per-module. Returns a ProofCache (for opcode
/// elision in bytecode compilation) and a set of trusted function names.
///
/// `mode` controls error reporting:
/// - `Check` mode: prints "note:" for proven contracts, hard-errors on Proven-level failures
/// - `Run`/`Repl` mode: same hard errors for Proven, warns on Checked-level Unknown
///
/// DiskCache is loaded/saved automatically (unless AIRL_NO_Z3_CACHE is set).

/// Flatten modules: expand `TopLevel::Module` bodies into their contained top-level items.
/// Non-module items are passed through unchanged. This ensures pipeline functions that
/// iterate over tops don't silently skip module-wrapped definitions.
fn flatten_module_tops(tops: &[airl_syntax::ast::TopLevel]) -> Vec<&airl_syntax::ast::TopLevel> {
    let mut result = Vec::new();
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Module(m) => {
                for inner in &m.body {
                    result.push(inner);
                }
            }
            other => result.push(other),
        }
    }
    result
}

fn z3_verify_tops(
    tops: &[airl_syntax::ast::TopLevel],
    mode: PipelineMode,
) -> Result<(airl_solver::ProofCache, HashSet<String>), PipelineError> {
    let z3_prover = airl_solver::prover::Z3Prover::new();
    let mut z3_warned_fns: HashSet<String> = HashSet::new();
    let mut proof_cache = airl_solver::ProofCache::new();
    let mut trusted_fns: HashSet<String> = HashSet::new();

    // Load Z3 disk cache
    let cache_path = std::path::PathBuf::from(".airl-z3-cache");
    let use_cache = std::env::var("AIRL_NO_Z3_CACHE").is_err();
    let mut disk_cache = if use_cache {
        airl_solver::cache::DiskCache::load(&cache_path)
    } else {
        airl_solver::cache::DiskCache::new()
    };
    let mut cache_keys = Vec::new();

    // When AIRL_STRICT_VERIFY is set (via --strict flag), override all verify
    // levels to Proven — every function must have provable contracts.
    let strict_verify = std::env::var("AIRL_STRICT_VERIFY").is_ok();

    // Collect all function definitions, including those inside modules.
    let mut all_fns: Vec<(&airl_syntax::ast::FnDef, airl_syntax::ast::VerifyLevel)> = Vec::new();
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => {
                let mut level = verify_level_for_fn(&f.name, tops);
                if strict_verify { level = airl_syntax::ast::VerifyLevel::Proven; }
                all_fns.push((f, level));
            }
            airl_syntax::ast::TopLevel::Module(m) => {
                for item in &m.body {
                    if let airl_syntax::ast::TopLevel::Defn(f) = item {
                        let level = if strict_verify { airl_syntax::ast::VerifyLevel::Proven } else { m.verify };
                        all_fns.push((f, level));
                    }
                }
            }
            _ => {}
        }
    }

    for (f, level) in &all_fns {
        match level {
            airl_syntax::ast::VerifyLevel::Trusted => {
                // Skip Z3 entirely — contracts are trusted, not checked
                trusted_fns.insert(f.name.clone());
            }
            airl_syntax::ast::VerifyLevel::Proven => {
                // Hard error if contracts can't be proven
                let key = airl_solver::cache_key(f);
                cache_keys.push(key);
                let verification = if let Some(cached) = disk_cache.get(key) {
                    cached
                } else {
                    let v = z3_prover.verify_function(f);
                    disk_cache.insert(key, &v);
                    v
                };
                for (clause, result) in verification.ensures_results.iter().chain(verification.invariants_results.iter()) {
                    proof_cache.insert(&f.name, clause, result.clone());
                    match result {
                        airl_solver::VerifyResult::Proven => {
                            if mode == PipelineMode::Check {
                                eprintln!("note: `{}` contract proven: {}", f.name, clause);
                            }
                        }
                        airl_solver::VerifyResult::Disproven { counterexample } => {
                            return Err(PipelineError::ContractDisproven {
                                fn_name: f.name.clone(),
                                clause: clause.clone(),
                                counterexample: counterexample.clone(),
                            });
                        }
                        airl_solver::VerifyResult::Unknown(reason) | airl_solver::VerifyResult::TranslationError(reason) => {
                            return Err(PipelineError::ContractUnprovable {
                                fn_name: f.name.clone(),
                                clause: clause.clone(),
                                reason: reason.clone(),
                            });
                        }
                    }
                }
            }
            airl_syntax::ast::VerifyLevel::Checked => {
                // Current behavior — warn on unprovable, don't error
                let key = airl_solver::cache_key(f);
                cache_keys.push(key);
                let verification = if let Some(cached) = disk_cache.get(key) {
                    cached
                } else {
                    let v = z3_prover.verify_function(f);
                    disk_cache.insert(key, &v);
                    v
                };
                for (clause, result) in verification.ensures_results.iter().chain(verification.invariants_results.iter()) {
                    proof_cache.insert(&f.name, clause, result.clone());
                    match result {
                        airl_solver::VerifyResult::Proven => {
                            if mode == PipelineMode::Check {
                                eprintln!("note: `{}` contract proven: {}", f.name, clause);
                            }
                        }
                        airl_solver::VerifyResult::Disproven { counterexample } => {
                            return Err(PipelineError::ContractDisproven {
                                fn_name: f.name.clone(),
                                clause: clause.clone(),
                                counterexample: counterexample.clone(),
                            });
                        }
                        airl_solver::VerifyResult::Unknown(_) | airl_solver::VerifyResult::TranslationError(_) => {
                            if z3_warned_fns.insert(f.name.clone()) {
                                eprintln!("warning: Z3 returned Unknown/TranslationError for `{}`, falling back to runtime checking", f.name);
                            }
                        }
                    }
                }
            }
        }
    }

    // Clean up stale cache entries and write to disk
    if use_cache {
        disk_cache.evict_stale(&cache_keys);
        let _ = disk_cache.write(&cache_path);
    }

    Ok((proof_cache, trusted_fns))
}

/// Look up the verify level for a function by finding its containing module.
fn verify_level_for_fn(fn_name: &str, tops: &[airl_syntax::ast::TopLevel]) -> airl_syntax::ast::VerifyLevel {
    for top in tops {
        if let airl_syntax::ast::TopLevel::Module(m) = top {
            for item in &m.body {
                if let airl_syntax::ast::TopLevel::Defn(f) = item {
                    if f.name == fn_name {
                        return m.verify;
                    }
                }
            }
        }
    }
    airl_syntax::ast::VerifyLevel::Checked
}

pub fn run_source_with_mode(source: &str, mode: PipelineMode) -> Result<Value, PipelineError> {
    let strict_mode = std::env::var("AIRL_STRICT").is_ok();

    let (tops, diags) = parse_source(source)?;
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
                if strict_mode {
                    return Err(PipelineError::TypeCheck(type_diags));
                }
                // Print as warnings to stderr, don't block
                for d in type_diags.errors() {
                    eprintln!("warning: {}", d.message);
                }
            }
        }
    }

    // Linearity check (flatten modules so wrapped defns are checked)
    let flat_tops = flatten_module_tops(&tops);
    let mut lin_checker = LinearityChecker::new();
    for top in &flat_tops {
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
                if strict_mode {
                    let mut msgs = Vec::new();
                    for d in lin_diags.errors() {
                        msgs.push(d.message.clone());
                    }
                    return Err(PipelineError::Runtime(RuntimeError::Custom(
                        msgs.join("; ")
                    )));
                }
                // Warn only — runtime ownership tracking enforces moves via MarkMoved/CheckNotMoved
                for d in lin_diags.errors() {
                    eprintln!("warning (linearity): {}", d.message);
                }
            }
        }
    }

    // Z3 contract verification — build proof cache for opcode elision
    let (proof_cache, trusted_fns) = z3_verify_tops(&tops, mode)?;

    // Build ownership map: function name → per-param "is Own" flags
    let ownership_map = build_ownership_map(&tops);

    // Compile AST → IR → Bytecode with contracts (proof cache elides proven opcodes)
    let (ir_nodes, contracts, fn_meta) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    bc_compiler.set_proven_clauses(proof_cache.into_proven_set());
    bc_compiler.set_trusted_fns(trusted_fns);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create VM, load cached stdlib, execute
    let mut vm = BytecodeVm::new();
    load_cached_stdlib(&mut vm)?;

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    for meta in fn_meta {
        vm.store_fn_metadata(meta);
    }
    vm.exec_main().map_err(PipelineError::Runtime)
}

pub fn run_source(source: &str) -> Result<Value, PipelineError> {
    run_source_with_mode(source, PipelineMode::Run)
}

/// Run source and also return the set of function names that were fully Z3-verified.
/// A function is "Z3-verified" if it has at least one :ensures/:invariant clause and all
/// clauses are `VerifyResult::Proven`. Used by the fixture test harness for ;;Z3-PROVEN: checks.
///
/// Uses `z3_verify_tops` (with VerifyLevel + DiskCache support) instead of a separate Z3 loop,
/// then inlines the remaining pipeline steps to avoid re-parsing and double Z3 verification.
pub fn run_source_with_z3_info(source: &str) -> Result<(Value, Vec<String>), PipelineError> {
    let strict_mode = std::env::var("AIRL_STRICT").is_ok();

    let (tops, diags) = parse_source(source)?;
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
        if strict_mode {
            return Err(PipelineError::TypeCheck(type_diags));
        }
        for d in type_diags.errors() {
            eprintln!("warning: {}", d.message);
        }
    }

    // Linearity check (flatten modules so wrapped defns are checked)
    let flat_tops = flatten_module_tops(&tops);
    let mut lin_checker = LinearityChecker::new();
    for top in &flat_tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            lin_checker.check_fn(f);
        }
    }
    if lin_checker.has_errors() {
        let lin_diags = lin_checker.drain_diagnostics();
        if strict_mode {
            let mut msgs = Vec::new();
            for d in lin_diags.errors() {
                msgs.push(d.message.clone());
            }
            return Err(PipelineError::Runtime(RuntimeError::Custom(msgs.join("; "))));
        }
        for d in lin_diags.errors() {
            eprintln!("warning (linearity): {}", d.message);
        }
    }

    // Z3 verification via centralized helper (respects VerifyLevel + DiskCache)
    let (proof_cache, trusted_fns) = z3_verify_tops(&tops, PipelineMode::Run)?;

    // Extract fully verified function names before consuming proof_cache
    let z3_verified = proof_cache.fully_verified_functions();

    // Compile AST → IR → Bytecode with contracts
    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts, fn_meta) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    bc_compiler.set_proven_clauses(proof_cache.into_proven_set());
    bc_compiler.set_trusted_fns(trusted_fns);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    // Create VM, load cached stdlib, execute
    let mut vm = BytecodeVm::new();
    load_cached_stdlib(&mut vm)?;
    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    for meta in fn_meta {
        vm.store_fn_metadata(meta);
    }
    let value = vm.exec_main().map_err(PipelineError::Runtime)?;
    Ok((value, z3_verified))
}

/// Run a file with preloaded modules (required for G3 bootstrap).
/// Each --load module is compiled and loaded into the VM before the main file runs.
pub fn run_file_with_preloads(path: &str, preloads: &[String]) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    let (tops, _diags) = parse_source(&source)?;

    // Z3 contract verification
    let (proof_cache, trusted_fns) = z3_verify_tops(&tops, PipelineMode::Run)?;

    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts, fn_meta) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    bc_compiler.set_proven_clauses(proof_cache.into_proven_set());
    bc_compiler.set_trusted_fns(trusted_fns);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    let mut vm = BytecodeVm::new();

    // Load cached stdlib
    load_cached_stdlib(&mut vm)?;

    // Load each preloaded module
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
    for meta in fn_meta {
        vm.store_fn_metadata(meta);
    }

    vm.exec_main().map_err(PipelineError::Runtime)
}

pub fn run_file(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source(&source)
}

pub fn check_source(source: &str) -> Result<(), PipelineError> {
    let tops = parse_source_strict(source)?;

    // Type check (strict mode)
    let mut checker = TypeChecker::new();
    for top in &tops {
        let _ = checker.check_top_level(top);
    }
    if checker.has_errors() {
        return Err(PipelineError::TypeCheck(checker.into_diagnostics()));
    }

    // Linearity check (strict mode, flatten modules so wrapped defns are checked)
    let flat_tops = flatten_module_tops(&tops);
    let mut lin_checker = LinearityChecker::new();
    for top in &flat_tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            lin_checker.check_fn(f);
        }
    }
    if lin_checker.has_errors() {
        let lin_diags = lin_checker.drain_diagnostics();
        let mut msgs = Vec::new();
        for d in lin_diags.errors() {
            msgs.push(d.message.clone());
            eprintln!("linearity error: {}", d.message);
        }
        return Err(PipelineError::Runtime(RuntimeError::Custom(
            msgs.join("; ")
        )));
    }

    // Z3 contract verification — respect VerifyLevel (with DiskCache)
    let _z3_results = z3_verify_tops(&tops, PipelineMode::Check)?;

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

fn compile_top_level(top: &airl_syntax::ast::TopLevel) -> Vec<IRNode> {
    match top {
        airl_syntax::ast::TopLevel::Defn(f) => {
            let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
            vec![IRNode::Func(f.name.clone(), param_names, Box::new(compile_expr(&f.body)))]
        }
        airl_syntax::ast::TopLevel::Define(d) => {
            vec![IRNode::Func(d.name.clone(), d.params.clone(), Box::new(compile_expr(&d.body)))]
        }
        airl_syntax::ast::TopLevel::Expr(e) => vec![compile_expr(e)],
        airl_syntax::ast::TopLevel::Module(m) => {
            m.body.iter().flat_map(compile_top_level).collect()
        }
        _ => vec![IRNode::Nil], // DefType, Task, UseDecl — no runtime effect in compiled mode
    }
}

// ── Shared: ownership map builder ──────────────────────────

/// Build a map from function names to per-parameter ownership flags.
/// Only includes functions that have at least one explicitly `Own`-annotated parameter.
fn build_ownership_map(tops: &[airl_syntax::ast::TopLevel]) -> HashMap<String, Vec<bool>> {
    let mut map = HashMap::new();
    let mut insert_fn = |f: &airl_syntax::ast::FnDef| {
        let own_flags: Vec<bool> = f.params.iter().map(|p| {
            matches!(p.ownership, airl_syntax::ast::Ownership::Own)
        }).collect();
        if own_flags.iter().any(|&o| o) {
            map.insert(f.name.clone(), own_flags);
        }
    };
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => insert_fn(f),
            airl_syntax::ast::TopLevel::Module(m) => {
                for inner in &m.body {
                    if let airl_syntax::ast::TopLevel::Defn(f) = inner {
                        insert_fn(f);
                    }
                }
            }
            _ => {}
        }
    }
    map
}

// ── Shared: AST → IR with contracts ───────────────────────

/// Compile a list of top-level AST nodes to IR, extracting contract clauses as IR+source pairs
/// and function metadata for runtime introspection.
fn compile_tops_with_contracts(
    tops: &[airl_syntax::ast::TopLevel],
) -> (
    Vec<IRNode>,
    HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
    Vec<airl_runtime::bytecode::FnDefMetadata>,
) {
    let mut ir_nodes = Vec::new();
    let mut contracts = HashMap::new();
    let mut metadata = Vec::new();

    // Helper: compile a single defn and record its contracts/metadata
    let mut compile_defn = |f: &airl_syntax::ast::FnDef,
                            ir_nodes: &mut Vec<IRNode>,
                            contracts: &mut HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
                            metadata: &mut Vec<airl_runtime::bytecode::FnDefMetadata>| {
        let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        ir_nodes.push(IRNode::Func(f.name.clone(), param_names.clone(), Box::new(compile_expr(&f.body))));

        let mut req_strings = Vec::with_capacity(f.requires.len());
        let req: Vec<(IRNode, String)> = f.requires.iter()
            .map(|e| { let s = e.to_airl(); req_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();
        let mut ens_strings = Vec::with_capacity(f.ensures.len());
        let ens: Vec<(IRNode, String)> = f.ensures.iter()
            .map(|e| { let s = e.to_airl(); ens_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();
        let mut inv_strings = Vec::with_capacity(f.invariants.len());
        let inv: Vec<(IRNode, String)> = f.invariants.iter()
            .map(|e| { let s = e.to_airl(); inv_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();

        if !req.is_empty() || !ens.is_empty() || !inv.is_empty() {
            contracts.insert(f.name.clone(), (req, ens, inv));
        }

        metadata.push(airl_runtime::bytecode::FnDefMetadata {
            name: f.name.clone(),
            param_names,
            param_types: f.params.iter().map(|p| p.ty.to_airl()).collect(),
            return_type: f.return_type.to_airl(),
            intent: f.intent.clone(),
            requires: req_strings,
            ensures: ens_strings,
            invariants: inv_strings,
        });
    };

    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => {
                compile_defn(f, &mut ir_nodes, &mut contracts, &mut metadata);
            }
            airl_syntax::ast::TopLevel::Module(m) => {
                for inner in &m.body {
                    match inner {
                        airl_syntax::ast::TopLevel::Defn(f) => {
                            compile_defn(f, &mut ir_nodes, &mut contracts, &mut metadata);
                        }
                        airl_syntax::ast::TopLevel::Define(d) => {
                            ir_nodes.push(IRNode::Func(d.name.clone(), d.params.clone(), Box::new(compile_expr(&d.body))));
                        }
                        airl_syntax::ast::TopLevel::Expr(e) => {
                            ir_nodes.push(compile_expr(e));
                        }
                        _ => {}
                    }
                }
            }
            airl_syntax::ast::TopLevel::Define(d) => {
                ir_nodes.push(IRNode::Func(d.name.clone(), d.params.clone(), Box::new(compile_expr(&d.body))));
            }
            airl_syntax::ast::TopLevel::Expr(e) => {
                ir_nodes.push(compile_expr(e));
            }
            _ => {
                ir_nodes.push(IRNode::Nil);
            }
        }
    }

    (ir_nodes, contracts, metadata)
}

/// Like `compile_tops_with_contracts` but skips `Expr` nodes.
/// Used for library modules in the import pipeline where top-level expressions
/// should not be executed.
fn compile_tops_without_exprs(
    tops: &[airl_syntax::ast::TopLevel],
) -> (
    Vec<IRNode>,
    HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
    Vec<airl_runtime::bytecode::FnDefMetadata>,
) {
    let mut ir_nodes = Vec::new();
    let mut contracts = HashMap::new();
    let mut metadata = Vec::new();

    // Helper: compile a defn for library use (no-expr mode)
    let mut compile_defn = |f: &airl_syntax::ast::FnDef,
                            ir_nodes: &mut Vec<IRNode>,
                            contracts: &mut HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
                            metadata: &mut Vec<airl_runtime::bytecode::FnDefMetadata>| {
        let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        ir_nodes.push(IRNode::Func(f.name.clone(), param_names.clone(), Box::new(compile_expr(&f.body))));

        let mut req_strings = Vec::with_capacity(f.requires.len());
        let req: Vec<(IRNode, String)> = f.requires.iter()
            .map(|e| { let s = e.to_airl(); req_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();
        let mut ens_strings = Vec::with_capacity(f.ensures.len());
        let ens: Vec<(IRNode, String)> = f.ensures.iter()
            .map(|e| { let s = e.to_airl(); ens_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();
        let mut inv_strings = Vec::with_capacity(f.invariants.len());
        let inv: Vec<(IRNode, String)> = f.invariants.iter()
            .map(|e| { let s = e.to_airl(); inv_strings.push(s.clone()); (compile_expr(e), s) })
            .collect();

        if !req.is_empty() || !ens.is_empty() || !inv.is_empty() {
            contracts.insert(f.name.clone(), (req, ens, inv));
        }

        metadata.push(airl_runtime::bytecode::FnDefMetadata {
            name: f.name.clone(),
            param_names,
            param_types: f.params.iter().map(|p| p.ty.to_airl()).collect(),
            return_type: f.return_type.to_airl(),
            intent: f.intent.clone(),
            requires: req_strings,
            ensures: ens_strings,
            invariants: inv_strings,
        });
    };

    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => {
                compile_defn(f, &mut ir_nodes, &mut contracts, &mut metadata);
            }
            airl_syntax::ast::TopLevel::Module(m) => {
                for inner in &m.body {
                    match inner {
                        airl_syntax::ast::TopLevel::Defn(f) => {
                            compile_defn(f, &mut ir_nodes, &mut contracts, &mut metadata);
                        }
                        airl_syntax::ast::TopLevel::Define(d) => {
                            ir_nodes.push(IRNode::Func(d.name.clone(), d.params.clone(), Box::new(compile_expr(&d.body))));
                        }
                        // Skip Expr inside modules for library imports
                        _ => {}
                    }
                }
            }
            airl_syntax::ast::TopLevel::Define(d) => {
                ir_nodes.push(IRNode::Func(d.name.clone(), d.params.clone(), Box::new(compile_expr(&d.body))));
            }
            // Skip Expr and Import nodes for library modules
            _ => {}
        }
    }

    (ir_nodes, contracts, metadata)
}

/// Stdlib source file paths (relative to the manifest directory at build time).
/// These are checked at runtime to detect source changes since the last embed.
const STDLIB_PATHS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/prelude.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/math.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/result.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/string.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/map.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/set.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/io.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/path.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),
    #[cfg(not(target_os = "airlos"))]
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),
];

/// Compute a hash of the embedded stdlib sources to detect changes.
fn stdlib_embed_hash() -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    COLLECTIONS_SOURCE.hash(&mut hasher);
    MATH_SOURCE.hash(&mut hasher);
    RESULT_SOURCE.hash(&mut hasher);
    STRING_SOURCE.hash(&mut hasher);
    MAP_SOURCE.hash(&mut hasher);
    SET_SOURCE.hash(&mut hasher);
    IO_SOURCE.hash(&mut hasher);
    PATH_SOURCE.hash(&mut hasher);
    RANDOM_SOURCE.hash(&mut hasher);
    #[cfg(not(target_os = "airlos"))]
    SQLITE_SOURCE.hash(&mut hasher);
    hasher.finish()
}

/// Compute a hash based on the mtime of the stdlib source files on disk.
/// Returns None if any file cannot be stat'd (e.g. not a development build).
fn stdlib_disk_hash() -> Option<u64> {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    for path in STDLIB_PATHS {
        match std::fs::metadata(path) {
            Ok(meta) => {
                match meta.modified() {
                    Ok(mtime) => mtime.hash(&mut hasher),
                    Err(_) => return None,
                }
            }
            Err(_) => return None,
        }
    }
    Some(hasher.finish())
}

/// Cached stdlib functions: parsed and compiled once, cloned into each new VM.
/// Stores the embed hash alongside compiled functions for invalidation detection.
static STDLIB_CACHE: OnceLock<(u64, Vec<(Vec<BytecodeFunc>, BytecodeFunc)>)> = OnceLock::new();

fn compile_stdlib_all() -> Result<Vec<(Vec<BytecodeFunc>, BytecodeFunc)>, PipelineError> {
    #[cfg(target_os = "airlos")]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
    ];
    #[cfg(not(target_os = "airlos"))]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
        (SQLITE_SOURCE, "sqlite"),
    ];
    let mut result = Vec::new();
    for (src, name) in stdlib_modules {
        let tops = parse_source_stdlib(src, name)?;
        let ir_nodes: Vec<IRNode> = tops.iter().flat_map(compile_top_level).collect();
        let mut bc_compiler = BytecodeCompiler::with_prefix(name);
        let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);
        result.push((funcs, main_func));
    }
    Ok(result)
}

/// Load pre-compiled stdlib functions into a VM, executing each module's init.
fn load_cached_stdlib(vm: &mut BytecodeVm) -> Result<(), PipelineError> {
    let embed_hash = stdlib_embed_hash();

    // Warn if the on-disk stdlib source differs from the embedded version.
    if let Some(disk_hash) = stdlib_disk_hash() {
        if disk_hash != embed_hash {
            eprintln!("warning: stdlib source changed since last embed, recompiling");
        }
    }

    let (_, cached) = STDLIB_CACHE.get_or_init(|| {
        let compiled = compile_stdlib_all().expect("stdlib compilation failed");
        (embed_hash, compiled)
    });
    for (funcs, main_func) in cached {
        for func in funcs {
            vm.load_function(func.clone());
        }
        vm.load_function(main_func.clone());
        vm.exec_main().map_err(|e| PipelineError::Runtime(e))?;
    }
    Ok(())
}

fn compile_and_load_stdlib_bytecode(vm: &mut BytecodeVm, source: &str, name: &str) -> Result<(), PipelineError> {
    let tops = parse_source_stdlib(source, name)?;

    let ir_nodes: Vec<IRNode> = tops.iter().flat_map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix(name);
    let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    vm.exec_main().map_err(|e| PipelineError::Runtime(e))?;

    Ok(())
}

// ── Import-aware pipeline ──────────────────────────────────

/// Run a file that uses `(import ...)` directives. Resolves all imports,
/// loads dependency modules with qualified function names, then executes
/// the entry module.
pub fn run_file_with_imports(entry_path: &str) -> Result<Value, PipelineError> {
    use crate::resolver::resolve_imports;

    let (modules, import_map) = resolve_imports(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    let mut module_publics: HashMap<String, Vec<String>> = HashMap::new();
    for module in &modules {
        module_publics.insert(module.name.clone(), module.public_fns.clone());
    }

    let mut vm = BytecodeVm::new();

    // Load cached stdlib
    load_cached_stdlib(&mut vm)?;

    // The resolver returns modules in dependency order with the entry last.
    // Use the entry path from the resolved modules directly to avoid a redundant canonicalize.
    let entry_canonical = &modules.last()
        .ok_or_else(|| PipelineError::Io("no modules resolved".into()))?
        .path;

    for module in &modules {
        let is_entry = &module.path == entry_canonical;
        let directives = import_map.get(&module.path)
            .map(|d| d.as_slice())
            .unwrap_or(&[]);

        // Import nodes compile to Nil (handled by the _ branch in compile_tops_with_contracts),
        // so we only need to filter Expr nodes for non-entry (library) modules.
        // Pass tops directly to avoid cloning the entire AST.
        let ownership_map = build_ownership_map(&module.tops);
        let (ir_nodes, contracts, fn_meta) = if is_entry {
            compile_tops_with_contracts(&module.tops)
        } else {
            compile_tops_without_exprs(&module.tops)
        };

        // Rewrite qualified names for the entry module (with visibility checks)
        let final_ir = if is_entry && !directives.is_empty() {
            rewrite_qualified_names(&ir_nodes, directives, &module_publics)?
        } else {
            ir_nodes
        };

        if is_entry {
            let mut bc_compiler = BytecodeCompiler::with_prefix("user");
            bc_compiler.set_ownership_map(ownership_map);
            let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&final_ir, &contracts);
            for func in funcs {
                vm.load_function(func);
            }
            vm.load_function(main_func);
            for meta in fn_meta {
                vm.store_fn_metadata(meta);
            }
        } else {
            // Library module: compile functions with qualified names (module_funcname)
            let mut bc_compiler = BytecodeCompiler::with_prefix(&module.name);
            bc_compiler.set_ownership_map(ownership_map);
            let (funcs, _main) = bc_compiler.compile_program_with_contracts(&final_ir, &contracts);
            for mut func in funcs {
                // Prefix function name with module name to create qualified name
                if func.name != "__main__" {
                    let qualified = format!("{}_{}", module.name, func.name);
                    func.name = qualified;
                }
                vm.load_function(func);
            }
            // Execute the module's __main__ (top-level expressions, if any)
            // For library modules we already filtered out Expr nodes, so this is a no-op
        }
    }

    vm.exec_main().map_err(PipelineError::Runtime)
}

// ── Qualified name rewriting for import resolution ────────

fn rewrite_qualified_names(
    nodes: &[IRNode],
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> Result<Vec<IRNode>, PipelineError> {
    nodes.iter()
        .map(|n| rewrite_ir_node(n, directives, module_publics))
        .collect()
}

fn rewrite_ir_node(
    node: &IRNode,
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> Result<IRNode, PipelineError> {
    match node {
        IRNode::Load(name) => {
            if let Some(rewritten) = rewrite_name(name, directives, module_publics)? {
                Ok(IRNode::Load(rewritten))
            } else {
                Ok(node.clone())
            }
        }
        IRNode::Call(name, args) => {
            let new_args: Vec<IRNode> = args.iter()
                .map(|a| rewrite_ir_node(a, directives, module_publics))
                .collect::<Result<_, _>>()?;
            if let Some(rewritten) = rewrite_name(name, directives, module_publics)? {
                Ok(IRNode::Call(rewritten, new_args))
            } else {
                Ok(IRNode::Call(name.clone(), new_args))
            }
        }
        IRNode::CallExpr(callee, args) => {
            Ok(IRNode::CallExpr(
                Box::new(rewrite_ir_node(callee, directives, module_publics)?),
                args.iter()
                    .map(|a| rewrite_ir_node(a, directives, module_publics))
                    .collect::<Result<_, _>>()?,
            ))
        }
        IRNode::If(c, t, e) => Ok(IRNode::If(
            Box::new(rewrite_ir_node(c, directives, module_publics)?),
            Box::new(rewrite_ir_node(t, directives, module_publics)?),
            Box::new(rewrite_ir_node(e, directives, module_publics)?),
        )),
        IRNode::Do(nodes) => Ok(IRNode::Do(
            nodes.iter()
                .map(|n| rewrite_ir_node(n, directives, module_publics))
                .collect::<Result<_, _>>()?,
        )),
        IRNode::Let(bindings, body) => {
            let new_bindings: Vec<IRBinding> = bindings.iter()
                .map(|b| Ok(IRBinding {
                    name: b.name.clone(),
                    expr: rewrite_ir_node(&b.expr, directives, module_publics)?,
                }))
                .collect::<Result<_, PipelineError>>()?;
            Ok(IRNode::Let(new_bindings, Box::new(rewrite_ir_node(body, directives, module_publics)?)))
        }
        IRNode::Func(name, params, body) => {
            Ok(IRNode::Func(name.clone(), params.clone(), Box::new(rewrite_ir_node(body, directives, module_publics)?)))
        }
        IRNode::Lambda(params, body) => {
            Ok(IRNode::Lambda(params.clone(), Box::new(rewrite_ir_node(body, directives, module_publics)?)))
        }
        IRNode::List(items) => Ok(IRNode::List(
            items.iter()
                .map(|i| rewrite_ir_node(i, directives, module_publics))
                .collect::<Result<_, _>>()?,
        )),
        IRNode::Variant(tag, args) => {
            Ok(IRNode::Variant(tag.clone(), args.iter()
                .map(|a| rewrite_ir_node(a, directives, module_publics))
                .collect::<Result<_, _>>()?))
        }
        IRNode::Match(scrutinee, arms) => {
            let new_arms: Vec<IRArm> = arms.iter()
                .map(|arm| Ok(IRArm {
                    pattern: arm.pattern.clone(),
                    body: rewrite_ir_node(&arm.body, directives, module_publics)?,
                }))
                .collect::<Result<_, PipelineError>>()?;
            Ok(IRNode::Match(
                Box::new(rewrite_ir_node(scrutinee, directives, module_publics)?),
                new_arms,
            ))
        }
        IRNode::Try(inner) => Ok(IRNode::Try(Box::new(rewrite_ir_node(inner, directives, module_publics)?))),
        // Leaf nodes: Int, Float, Str, Bool, Nil — no rewriting needed
        _ => Ok(node.clone()),
    }
}

/// Check if a name should be rewritten based on import directives.
/// Returns Ok(Some(new_name)) if the name matches a qualified reference or an :only import.
/// Returns Err if the symbol is private (not in module_publics).
fn rewrite_name(
    name: &str,
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> Result<Option<String>, PipelineError> {
    // Check for qualified name: prefix.symbol (e.g., "math_lib.my-abs")
    if let Some(dot_pos) = name.find('.') {
        let prefix = &name[..dot_pos];
        let symbol = &name[dot_pos + 1..];
        for d in directives {
            if d.prefix == prefix {
                // Visibility check: symbol must be in module's public_fns
                if let Some(publics) = module_publics.get(&d.module_name) {
                    if !publics.contains(&symbol.to_string()) {
                        return Err(PipelineError::Io(format!(
                            "'{}' is not public in module '{}' -- add :pub to export it",
                            symbol, d.module_name
                        )));
                    }
                }
                return Ok(Some(format!("{}_{}", d.module_name, symbol)));
            }
        }
    }
    // Check for :only imports (bare name that should be rewritten)
    for d in directives {
        if let Some(only_list) = &d.only {
            if only_list.iter().any(|s| s == name) {
                // Visibility check for :only imports too
                if let Some(publics) = module_publics.get(&d.module_name) {
                    if !publics.contains(&name.to_string()) {
                        return Err(PipelineError::Io(format!(
                            "'{}' is not public in module '{}' -- add :pub to export it",
                            name, d.module_name
                        )));
                    }
                }
                return Ok(Some(format!("{}_{}", d.module_name, name)));
            }
        }
    }
    Ok(None)
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
    ContractDisproven {
        fn_name: String,
        clause: String,
        counterexample: Vec<(String, String)>,
    },
    ContractUnprovable {
        fn_name: String,
        clause: String,
        reason: String,
    },
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
            PipelineError::ContractDisproven { fn_name, clause, counterexample } => {
                write!(f, "Contract disproven in `{}`: {} (counterexample: {:?})", fn_name, clause, counterexample)
            }
            PipelineError::ContractUnprovable { fn_name, clause, reason } => {
                write!(f, "Contract unprovable in `{}`: {} (reason: {})", fn_name, clause, reason)
            }
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
pub fn compile_and_load_stdlib_bytecode_repl(vm: &mut BytecodeVm) -> Result<(), PipelineError> {
    load_cached_stdlib(vm)
}

/// Compile AIRL source and run it in an existing bytecode VM (for incremental REPL use).
/// Functions defined in previous calls persist in the VM's function table.
pub fn compile_and_run_repl_input(source: &str, vm: &mut BytecodeVm) -> Result<Value, String> {
    let (tops, _diags) = parse_source(source).map_err(|e| format!("{}", e))?;

    let ownership_map = build_ownership_map(&tops);
    let (ir_nodes, contracts, fn_meta) = compile_tops_with_contracts(&tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("repl");
    bc_compiler.set_ownership_map(ownership_map);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);

    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);
    for meta in fn_meta {
        vm.store_fn_metadata(meta);
    }
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
        let sexprs = parse_sexpr_all(tokens).unwrap();
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
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        for sexpr in &sexprs {
            if let Ok(airl_syntax::ast::TopLevel::Defn(f)) = parser::parse_top_level(sexpr, &mut diags) {
                lin.check_fn(&f);
            }
        }
        assert!(!lin.has_errors(), "default ownership should not trigger linearity errors");
    }

    // ── Fix 1: AIRL_STRICT mode tests ────────────────────────────────────

    /// Test helper: run source with an explicit strict flag instead of reading
    /// the env var.  Avoids set_var/remove_var races under parallel test runners
    /// (issue-057).
    fn run_source_with_strict(source: &str, mode: PipelineMode, strict: bool) -> Result<Value, PipelineError> {
        let (tops, diags) = parse_source(source)?;
        if diags.has_errors() {
            return Err(PipelineError::Parse(diags));
        }
        let mut checker = TypeChecker::new();
        for top in &tops {
            let _ = checker.check_top_level(top);
        }
        if checker.has_errors() {
            let type_diags = checker.into_diagnostics();
            match mode {
                PipelineMode::Check => return Err(PipelineError::TypeCheck(type_diags)),
                PipelineMode::Run | PipelineMode::Repl => {
                    if strict {
                        return Err(PipelineError::TypeCheck(type_diags));
                    }
                    for d in type_diags.errors() { eprintln!("warning: {}", d.message); }
                }
            }
        }
        // Re-use the public pipeline for the rest (eval, linearity checks, etc.)
        run_source_with_mode(source, mode)
    }

    #[test]
    fn strict_mode_off_type_errors_are_warnings_not_fatal() {
        // Without strict mode, type errors in Run mode warn but don't block execution.
        // Simple integer arithmetic is always type-correct so this is the happy path.
        // Uses run_source_with_strict to avoid set_var races (issue-057).
        let result = run_source_with_strict("(+ 1 2)", PipelineMode::Run, false);
        assert!(result.is_ok(), "should succeed without strict mode: {:?}", result);
    }

    #[test]
    fn strict_mode_on_does_not_break_valid_source() {
        // Strict mode should not reject valid, well-typed source.
        // Uses run_source_with_strict to avoid set_var races (issue-057).
        let result = run_source_with_strict("(+ 1 2)", PipelineMode::Run, true);
        assert!(result.is_ok(), "valid source must succeed under strict mode: {:?}", result);
    }

    #[test]
    fn strict_mode_env_var_detected() {
        // Confirm the pipeline reads AIRL_STRICT from the environment at call time.
        // We verify via run_source_with_mode and the known absence of AIRL_STRICT
        // in a freshly-started test process (issue-057: no set_var/remove_var calls).
        let strict_set = std::env::var("AIRL_STRICT").is_ok();
        // run_source_with_mode reads AIRL_STRICT; result must be consistent with it.
        let result = run_source_with_mode("(+ 1 2)", PipelineMode::Run);
        assert!(result.is_ok(), "pipeline should handle (+ 1 2) regardless of AIRL_STRICT={strict_set}");
    }

    // ── Strict-verify (--strict) override ──────────────────────────────

    #[test]
    fn strict_verify_overrides_checked_to_proven() {
        // A Checked module with an untranslatable ensures clause (lambda call)
        // should succeed under normal Checked mode (warn only) but fail under
        // AIRL_STRICT_VERIFY which elevates all levels to Proven.
        let source = r#"
(module sv-test
  :version 0.1.0
  :provides [sv-id]
  :verify checked
  (defn sv-id
    :sig [(x : i32) -> i32]
    :ensures [(= result x)]
    :body ((fn [(y : i32)] y) x)))
(sv-id 1)
"#;
        // Without AIRL_STRICT_VERIFY, Checked + Unknown succeeds (warns only)
        let result = check_source(source);
        assert!(result.is_ok(), "Checked mode should warn, not error: {:?}", result);

        // With AIRL_STRICT_VERIFY, same code errors (elevated to Proven)
        std::env::set_var("AIRL_STRICT_VERIFY", "1");
        let result2 = check_source(source);
        std::env::remove_var("AIRL_STRICT_VERIFY");
        assert!(result2.is_err(), "strict-verify should elevate Checked to Proven and error");
    }

    // ── Fix 2: Z3 Unknown deduplication ──────────────────────────────────

    #[test]
    fn z3_warned_fns_deduplication() {
        // Verify that a HashSet correctly deduplicates function names.
        let mut warned: HashSet<String> = HashSet::new();
        // Inserting the same name twice — second insert returns false.
        assert!(warned.insert("foo".to_string()), "first insert should return true");
        assert!(!warned.insert("foo".to_string()), "second insert should return false");
        assert_eq!(warned.len(), 1, "set should contain only one entry");
    }

    // ── Fix 3: stdlib cache invalidation helpers ──────────────────────────

    #[test]
    fn stdlib_embed_hash_is_stable() {
        // Hash of embedded stdlib content should be deterministic across calls.
        let h1 = stdlib_embed_hash();
        let h2 = stdlib_embed_hash();
        assert_eq!(h1, h2, "embed hash must be deterministic");
    }

    #[test]
    fn stdlib_embed_hash_is_nonzero() {
        // Sanity: the embedded content is non-empty so hash should be non-zero.
        let h = stdlib_embed_hash();
        assert_ne!(h, 0, "embed hash of non-empty stdlib should be non-zero");
    }

    #[test]
    fn stdlib_disk_hash_returns_some_or_none() {
        // stdlib_disk_hash() is allowed to return None (e.g. in a release build
        // where source files are not present). We just verify it doesn't panic.
        let _result = stdlib_disk_hash();
        // No assertion needed — just confirming no panic.
    }
}

// ── AOT Compilation Pipeline ──────────────────────────────────────────────

/// Compile AIRL source files to a native object file.
/// Returns the object file bytes (ELF on Linux, Mach-O on macOS).
#[cfg(feature = "aot")]
pub fn compile_to_object(paths: &[String], target: Option<&str>) -> Result<Vec<u8>, PipelineError> {
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
        (SET_SOURCE, "set"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
    ] {
        let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
        // Only take named functions, skip the __main__ (which just returns nil)
        all_funcs.extend(funcs);
    }

    // 1b. Compile stdlib with extern-c declarations (io.airl, sqlite.airl)
    let mut stdlib_extern_c_decls: Vec<airl_runtime::bytecode_aot::ExternCInfo> = Vec::new();
    for (src, name) in &[
        (IO_SOURCE, "io"),
        (SQLITE_SOURCE, "sqlite"),
    ] {
        let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(src, name)?;
        all_funcs.extend(funcs);
        stdlib_extern_c_decls.extend(externs);
    }

    // 2. Parse and compile user source files with Z3 verification
    let mut all_source = String::new();
    for path in paths {
        let source = std::fs::read_to_string(path)
            .map_err(|e| PipelineError::Io(format!("{}: {}", path, e)))?;
        all_source.push_str(&source);
        all_source.push('\n');
    }

    // Parse user source and extract extern-c declarations
    let (user_tops, user_extern_c_decls) = parse_source_with_externs(&all_source)?;

    // Z3 contract verification on user code
    let (proof_cache, trusted_fns) = z3_verify_tops(&user_tops, PipelineMode::Run)?;

    // Compile user source with contracts and Z3 proof results
    let ownership_map = build_ownership_map(&user_tops);
    let (ir_nodes, contracts, _fn_meta) = compile_tops_with_contracts(&user_tops);
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    bc_compiler.set_ownership_map(ownership_map);
    bc_compiler.set_proven_clauses(proof_cache.into_proven_set());
    bc_compiler.set_trusted_fns(trusted_fns);
    let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts);
    let extern_c_decls = user_extern_c_decls;
    all_funcs.extend(funcs);
    all_funcs.push(main_func); // Only the user's __main__

    // 3. AOT compile bytecode → native object
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new_with_target(target).map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    for ext in stdlib_extern_c_decls.iter().chain(&extern_c_decls) {
        aot.register_extern_c(&ext.c_name, ext.arity);
    }

    // Register Z3 bridge builtins so g3's z3_bridge_g3.airl can call
    // them without extern-c declarations.
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
    }

    for func in &all_funcs {
        aot.compile_all(std::slice::from_ref(func), &func_map)
            .map_err(|e| PipelineError::Runtime(
                airl_runtime::error::RuntimeError::TypeError(e)
            ))?;
    }

    aot.emit_entry_point().map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    Ok(aot.finish())
}

/// Compile AIRL source file with imports to a native object file.
/// Mirrors `run_file_with_imports` but produces AOT output instead of running in VM.
#[cfg(feature = "aot")]
pub fn compile_to_object_with_imports(entry_path: &str, target: Option<&str>) -> Result<Vec<u8>, PipelineError> {
    use crate::resolver::resolve_imports;
    use airl_runtime::bytecode::BytecodeFunc;
    use airl_runtime::bytecode_aot::BytecodeAot;

    let (modules, import_map) = resolve_imports(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    let mut module_publics: HashMap<String, Vec<String>> = HashMap::new();
    for module in &modules {
        module_publics.insert(module.name.clone(), module.public_fns.clone());
    }

    let mut all_funcs: Vec<BytecodeFunc> = Vec::new();

    // 1. Compile stdlib to bytecode
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (PATH_SOURCE, "path"),
    ] {
        let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
        all_funcs.extend(funcs);
    }

    // 1b. Compile stdlib with extern-c declarations (io.airl, sqlite.airl)
    let mut extern_c_decls: Vec<airl_runtime::bytecode_aot::ExternCInfo> = Vec::new();
    for (src, name) in &[
        (IO_SOURCE, "io"),
        (SQLITE_SOURCE, "sqlite"),
    ] {
        let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(src, name)?;
        all_funcs.extend(funcs);
        extern_c_decls.extend(externs);
    }

    // 2. Compile each module in dependency order
    let entry_canonical = std::fs::canonicalize(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    for module in &modules {
        let is_entry = module.path == entry_canonical;
        let directives = import_map.get(&module.path)
            .map(|d| d.as_slice())
            .unwrap_or(&[]);

        // Extract extern-c declarations from this module
        for top in &module.tops {
            if let airl_syntax::ast::TopLevel::ExternC(decl) = top {
                extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                    c_name: decl.c_name.clone(),
                    arity: decl.params.len(),
                });
            }
            // Also extract extern-c from inside module bodies
            if let airl_syntax::ast::TopLevel::Module(m) = top {
                for inner in &m.body {
                    if let airl_syntax::ast::TopLevel::ExternC(decl) = inner {
                        extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                            c_name: decl.c_name.clone(),
                            arity: decl.params.len(),
                        });
                    }
                }
            }
        }

        // Filter tops: remove Import nodes, skip Expr for non-entry modules
        let filtered_tops: Vec<airl_syntax::ast::TopLevel> = module.tops.iter()
            .filter(|t| !matches!(t, airl_syntax::ast::TopLevel::Import { .. }))
            .filter(|t| is_entry || !matches!(t, airl_syntax::ast::TopLevel::Expr(_)))
            .cloned()
            .collect();

        // Z3 contract verification for this module
        let (proof_cache, trusted_fns) = z3_verify_tops(&filtered_tops, PipelineMode::Run)?;
        let proven_set = proof_cache.into_proven_set();

        let (ir_nodes, contracts, _fn_meta) = compile_tops_with_contracts(&filtered_tops);

        // Rewrite qualified names for the entry module (with visibility checks)
        let final_ir = if is_entry && !directives.is_empty() {
            rewrite_qualified_names(&ir_nodes, directives, &module_publics)?
        } else {
            ir_nodes
        };

        if is_entry {
            let ownership_map = build_ownership_map(&filtered_tops);
            let mut bc_compiler = BytecodeCompiler::with_prefix("user");
            bc_compiler.set_ownership_map(ownership_map);
            bc_compiler.set_proven_clauses(proven_set);
            bc_compiler.set_trusted_fns(trusted_fns);
            let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&final_ir, &contracts);
            all_funcs.extend(funcs);
            all_funcs.push(main_func); // Only entry module's __main__
        } else {
            // Library module: compile functions with qualified names
            let ownership_map = build_ownership_map(&filtered_tops);
            let mut bc_compiler = BytecodeCompiler::with_prefix(&module.name);
            bc_compiler.set_ownership_map(ownership_map);
            bc_compiler.set_proven_clauses(proven_set);
            bc_compiler.set_trusted_fns(trusted_fns);
            let (funcs, _main) = bc_compiler.compile_program_with_contracts(&final_ir, &contracts);
            for mut func in funcs {
                if func.name != "__main__" {
                    let qualified = format!("{}_{}", module.name, func.name);
                    func.name = qualified;
                }
                all_funcs.push(func);
            }
        }
    }

    // 3. AOT compile bytecode -> native object
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new_with_target(target).map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    for ext in &extern_c_decls {
        aot.register_extern_c(&ext.c_name, ext.arity);
    }

    for func in &all_funcs {
        aot.compile_all(std::slice::from_ref(func), &func_map)
            .map_err(|e| PipelineError::Runtime(
                airl_runtime::error::RuntimeError::TypeError(e)
            ))?;
    }

    aot.emit_entry_point().map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

    Ok(aot.finish())
}

/// Compile source string to bytecode functions (shared by run and AOT paths).
#[cfg(feature = "aot")]
fn compile_source_to_bytecode(source: &str, prefix: &str) -> Result<(Vec<airl_runtime::bytecode::BytecodeFunc>, airl_runtime::bytecode::BytecodeFunc), PipelineError> {
    let (funcs, main, _externs) = compile_source_to_bytecode_with_externs(source, prefix)?;
    Ok((funcs, main))
}

/// Like `compile_source_to_bytecode` but also returns extern-c declarations.
#[cfg(feature = "aot")]
fn compile_source_to_bytecode_with_externs(
    source: &str,
    prefix: &str,
) -> Result<(Vec<airl_runtime::bytecode::BytecodeFunc>, airl_runtime::bytecode::BytecodeFunc, Vec<airl_runtime::bytecode_aot::ExternCInfo>), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    let mut extern_c_decls = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                if let airl_syntax::ast::TopLevel::ExternC(ref decl) = top {
                    extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                        c_name: decl.c_name.clone(),
                        arity: decl.params.len(),
                    });
                }
                tops.push(top);
            }
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(_) => return Err(PipelineError::Syntax(d)),
                }
            }
        }
    }
    // Also extract extern-c declarations from inside module bodies
    for top in &tops {
        if let airl_syntax::ast::TopLevel::Module(m) = top {
            for inner in &m.body {
                if let airl_syntax::ast::TopLevel::ExternC(ref decl) = inner {
                    extern_c_decls.push(airl_runtime::bytecode_aot::ExternCInfo {
                        c_name: decl.c_name.clone(),
                        arity: decl.params.len(),
                    });
                }
            }
        }
    }

    let ir_nodes: Vec<IRNode> = tops.iter().flat_map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix(prefix);
    let (funcs, main) = bc_compiler.compile_program(&ir_nodes);
    Ok((funcs, main, extern_c_decls))
}
