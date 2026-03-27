# Module System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add file-based `(import ...)` with `:pub` visibility, qualified names (`math.abs`), and circular dependency detection to AIRL.

**Architecture:** New `Import` AST variant + `:pub` flag on FnDef/TypeDef parsed in `airl-syntax`. New `resolver.rs` in `airl-driver` resolves import graphs to dependency-ordered module lists. Pipeline compiles each module with a unique prefix and rewrites qualified names (`math.abs` → `math_abs`) before bytecode compilation. No new crates — all changes in existing crates.

**Tech Stack:** Rust (airl-syntax, airl-driver, airl-runtime, airl-types). No new dependencies.

**Design Spec:** `docs/superpowers/specs/2026-03-27-module-system.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/airl-syntax/src/ast.rs` | Modify | Add `Import` to `TopLevel`, `is_public` to `FnDef`/`TypeDef` |
| `crates/airl-syntax/src/parser.rs` | Modify | Parse `(import ...)` and `:pub` keyword |
| `crates/airl-driver/src/resolver.rs` | Create | Import graph resolution, circular dep detection, dependency ordering |
| `crates/airl-driver/src/pipeline.rs` | Modify | Wire resolver into `run_source_with_mode`, `run_file_with_preloads`, `compile_to_object` |
| `crates/airl-driver/src/main.rs` | Modify | Pass entry file path to pipeline (resolver needs it for relative path resolution) |
| `crates/airl-driver/src/lib.rs` | Modify | Add `pub mod resolver;` |
| `crates/airl-types/src/checker.rs` | Modify | Validate qualified references, enforce visibility |
| `tests/fixtures/valid/import_basic.airl` | Create | Basic import integration test |
| `tests/fixtures/valid/import_alias.airl` | Create | Import with `:as` alias |
| `tests/fixtures/valid/import_only.airl` | Create | Selective import with `:only` |
| `tests/fixtures/valid/modules/` | Create | Helper modules for import tests |
| `tests/fixtures/type_errors/import_private.airl` | Create | Private symbol access error |
| `tests/fixtures/type_errors/import_circular.airl` | Create | Circular dependency error |

---

### Task 1: Add `Import` to AST and `:pub` flag to FnDef/TypeDef

**Files:**
- Modify: `crates/airl-syntax/src/ast.rs`

- [ ] **Step 1: Write a test for the new AST nodes**

Add to the `#[cfg(test)] mod tests` block in `ast.rs`:

```rust
#[test]
fn import_ast_node_constructable() {
    let import = TopLevel::Import {
        path: "lib/math.airl".to_string(),
        alias: Some("m".to_string()),
        only: None,
        span: Span::dummy(),
    };
    let _ = import.clone();
    let _ = format!("{:?}", import);
}

#[test]
fn fn_def_has_is_public() {
    let f = FnDef {
        name: "test".to_string(),
        params: vec![],
        return_type: AstType { kind: AstTypeKind::Named("Unit".to_string()), span: Span::dummy() },
        intent: None,
        requires: vec![],
        ensures: vec![],
        invariants: vec![],
        body: Expr { kind: ExprKind::NilLit, span: Span::dummy() },
        execute_on: None,
        priority: None,
        is_public: true,
        span: Span::dummy(),
    };
    assert!(f.is_public);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-syntax -- import_ast_node`
Expected: FAIL — `Import` variant doesn't exist, `is_public` field doesn't exist.

- [ ] **Step 3: Add `Import` variant to `TopLevel` enum**

In `crates/airl-syntax/src/ast.rs`, add to the `TopLevel` enum (after `UseDecl`):

```rust
Import {
    path: String,           // "lib/math.airl"
    alias: Option<String>,  // :as name
    only: Option<Vec<String>>, // :only [names]
    span: Span,
},
```

- [ ] **Step 4: Add `is_public` field to `FnDef`**

In `crates/airl-syntax/src/ast.rs`, add field to `FnDef` struct (after `priority`):

```rust
pub is_public: bool,
```

- [ ] **Step 5: Add `is_public` field to `TypeDef`**

In `crates/airl-syntax/src/ast.rs`, add field to `TypeDef` struct (after `body`):

```rust
pub is_public: bool,
```

- [ ] **Step 6: Fix all compilation errors from new fields**

Every place that constructs a `FnDef` or `TypeDef` now needs `is_public: false` (the default). Every `match` on `TopLevel` needs a `TopLevel::Import { .. }` arm. Search for these with:

```bash
cargo build -p airl-syntax 2>&1 | head -60
```

Key places that need updates:
- `parser.rs`: `parse_defn` → add `is_public: false` to `FnDef` construction
- `parser.rs`: `parse_deftype` → add `is_public: false` to `TypeDef` construction
- `pipeline.rs`: `compile_top_level` match → add `TopLevel::Import { .. } => IRNode::Nil`
- `pipeline.rs`: `compile_tops_with_contracts` match → add `TopLevel::Import { .. }` arm
- `pipeline.rs`: `build_ownership_map` → already only matches `Defn`, no change needed
- `checker.rs` in `airl-types`: `check_top_level` match → add `TopLevel::Import { .. }` arm
- Any other file matching on `TopLevel` exhaustively

Run: `cargo build --features jit,aot` — fix all errors until clean.

- [ ] **Step 7: Run tests to verify everything passes**

Run: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
Expected: All existing tests pass + new tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/airl-syntax/src/ast.rs crates/airl-syntax/src/parser.rs crates/airl-driver/src/pipeline.rs crates/airl-types/src/checker.rs
git commit -m "feat(modules): add Import AST variant and :pub flag on FnDef/TypeDef"
```

---

### Task 2: Parse `(import ...)` syntax

**Files:**
- Modify: `crates/airl-syntax/src/parser.rs`

- [ ] **Step 1: Write parser tests for import syntax**

Add to `#[cfg(test)] mod tests` in `parser.rs`:

```rust
#[test]
fn parse_import_basic() {
    let tops = parse_top(r#"(import "lib/math.airl")"#);
    if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
        assert_eq!(path, "lib/math.airl");
        assert!(alias.is_none());
        assert!(only.is_none());
    } else {
        panic!("expected Import, got {:?}", tops[0]);
    }
}

#[test]
fn parse_import_with_alias() {
    let tops = parse_top(r#"(import "lib/math.airl" :as m)"#);
    if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
        assert_eq!(path, "lib/math.airl");
        assert_eq!(alias.as_deref(), Some("m"));
        assert!(only.is_none());
    } else {
        panic!("expected Import");
    }
}

#[test]
fn parse_import_with_only() {
    let tops = parse_top(r#"(import "lib/math.airl" :only [abs min max])"#);
    if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
        assert_eq!(path, "lib/math.airl");
        assert!(alias.is_none());
        assert_eq!(only.as_ref().unwrap(), &vec!["abs".to_string(), "min".to_string(), "max".to_string()]);
    } else {
        panic!("expected Import");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-syntax -- parse_import`
Expected: FAIL — `parse_top` doesn't handle `import` head yet.

- [ ] **Step 3: Add `import` case to `parse_top_level`**

In `parser.rs`, in the `match head` block of `parse_top_level`, add:

```rust
"import" => parse_import(&items[1..], *span, diags),
```

- [ ] **Step 4: Implement `parse_import` function**

Add after the `parse_module` function:

```rust
fn parse_import(items: &[SExpr], span: Span, _diags: &mut Diagnostics) -> Result<TopLevel, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("import requires a path string", span));
    }

    let path = expect_string(&items[0])?;
    let mut alias = None;
    let mut only = None;

    let mut i = 1;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "as" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected name after :as", span));
                    }
                    alias = Some(expect_symbol(&items[i])?);
                }
                "only" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected symbol list after :only", span));
                    }
                    only = Some(parse_symbol_list(&items[i])?);
                }
                _ => return Err(Diagnostic::error(
                    &format!("unknown import option :{}", kw), span)),
            }
        }
        i += 1;
    }

    Ok(TopLevel::Import { path, alias, only, span })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p airl-syntax -- parse_import`
Expected: All 3 import tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-syntax/src/parser.rs
git commit -m "feat(modules): parse (import ...) with :as and :only options"
```

---

### Task 3: Parse `:pub` on defn and deftype

**Files:**
- Modify: `crates/airl-syntax/src/parser.rs`

- [ ] **Step 1: Write parser tests for `:pub`**

Add to tests:

```rust
#[test]
fn parse_defn_public() {
    let tops = parse_top(r#"
        (defn abs :pub
          :sig [(x : i64) -> i64]
          :body (if (< x 0) (- 0 x) x))
    "#);
    if let TopLevel::Defn(f) = &tops[0] {
        assert_eq!(f.name, "abs");
        assert!(f.is_public);
    } else {
        panic!("expected Defn");
    }
}

#[test]
fn parse_defn_private_by_default() {
    let tops = parse_top(r#"
        (defn helper
          :sig [(x : i64) -> i64]
          :body x)
    "#);
    if let TopLevel::Defn(f) = &tops[0] {
        assert!(!f.is_public);
    } else {
        panic!("expected Defn");
    }
}

#[test]
fn parse_deftype_public() {
    let tops = parse_top(r#"
        (deftype Color :pub
          (| (Red) (Green) (Blue)))
    "#);
    if let TopLevel::DefType(td) = &tops[0] {
        assert_eq!(td.name, "Color");
        assert!(td.is_public);
    } else {
        panic!("expected DefType");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-syntax -- parse_defn_public parse_deftype_public`
Expected: FAIL — `:pub` not recognized.

- [ ] **Step 3: Add `:pub` parsing to `parse_defn`**

In `parse_defn`, after extracting `name` (line ~548) and before the keyword parsing loop, check if the next item is `:pub`:

```rust
let name = expect_symbol(&items[0])?;

// Check for :pub modifier immediately after name
let mut is_public = false;
let mut start_idx = 1;
if start_idx < items.len() {
    if let Some("pub") = items[start_idx].as_keyword() {
        is_public = true;
        start_idx += 1;
    }
}

// ... existing keyword loop starts at start_idx instead of 1 ...
let mut i = start_idx;
```

Then in the `FnDef` construction at the bottom of `parse_defn`, add `is_public`.

- [ ] **Step 4: Add `:pub` parsing to `parse_deftype`**

Same pattern — check for `:pub` right after the name in `parse_deftype`. The deftype parser extracts the name first, then looks for type params or body. Insert the `:pub` check between name extraction and the rest:

```rust
let name = expect_symbol(&items[0])?;

let mut is_public = false;
let mut start_idx = 1;
if start_idx < items.len() {
    if let Some("pub") = items[start_idx].as_keyword() {
        is_public = true;
        start_idx += 1;
    }
}
```

Then pass `is_public` into the `TypeDef` construction.

- [ ] **Step 5: Run tests**

Run: `cargo test -p airl-syntax`
Expected: All parser tests pass including new `:pub` tests and all existing tests.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-syntax/src/parser.rs
git commit -m "feat(modules): parse :pub visibility on defn and deftype"
```

---

### Task 4: Import Resolver

**Files:**
- Create: `crates/airl-driver/src/resolver.rs`
- Modify: `crates/airl-driver/src/lib.rs`

- [ ] **Step 1: Create `resolver.rs` with struct definitions and test scaffolding**

Create `crates/airl-driver/src/resolver.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use airl_syntax::ast::TopLevel;

/// A resolved module: parsed, with its public symbols identified.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Canonical absolute path to the source file.
    pub path: PathBuf,
    /// Module name (filename stem, e.g., "math" for "lib/math.airl").
    pub name: String,
    /// Parsed top-level forms.
    pub tops: Vec<TopLevel>,
    /// Names of public functions (`:pub` annotated).
    pub public_fns: Vec<String>,
    /// Names of public types (`:pub` annotated).
    pub public_types: Vec<String>,
}

/// How a module was imported by a specific file.
#[derive(Debug, Clone)]
pub struct ImportDirective {
    /// The module name (filename stem) or alias.
    pub prefix: String,
    /// If `:only [names]` was used, the specific symbols imported without prefix.
    pub only: Option<Vec<String>>,
    /// Resolved module name (always the filename stem, regardless of alias).
    pub module_name: String,
}

#[derive(Debug)]
pub enum ResolveError {
    Io(String),
    Parse(String),
    CircularDependency(Vec<String>),
    SandboxViolation(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Io(msg) => write!(f, "import error: {}", msg),
            ResolveError::Parse(msg) => write!(f, "import parse error: {}", msg),
            ResolveError::CircularDependency(chain) => {
                write!(f, "circular dependency detected:\n")?;
                for (i, path) in chain.iter().enumerate() {
                    if i > 0 { write!(f, "  imports ")?; }
                    write!(f, "{}", path)?;
                    if i < chain.len() - 1 { writeln!(f)?; }
                }
                Ok(())
            }
            ResolveError::SandboxViolation(msg) => write!(f, "import sandbox violation: {}", msg),
        }
    }
}

/// Resolve all imports starting from an entry file.
/// Returns modules in dependency order (leaves first, entry last).
/// Also returns the import directives for each file (keyed by canonical path).
pub fn resolve_imports(entry_path: &str) -> Result<(Vec<ResolvedModule>, HashMap<PathBuf, Vec<ImportDirective>>), ResolveError> {
    let entry = std::fs::canonicalize(entry_path)
        .map_err(|e| ResolveError::Io(format!("{}: {}", entry_path, e)))?;

    let mut resolved: Vec<ResolvedModule> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut import_map: HashMap<PathBuf, Vec<ImportDirective>> = HashMap::new();

    resolve_recursive(&entry, &mut resolved, &mut visited, &mut vec![], &mut import_map)?;

    Ok((resolved, import_map))
}

fn resolve_recursive(
    file_path: &Path,
    resolved: &mut Vec<ResolvedModule>,
    visited: &mut HashSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
    import_map: &mut HashMap<PathBuf, Vec<ImportDirective>>,
) -> Result<(), ResolveError> {
    let canonical = std::fs::canonicalize(file_path)
        .map_err(|e| ResolveError::Io(format!("{}: {}", file_path.display(), e)))?;

    // Already resolved — skip
    if visited.contains(&canonical) {
        return Ok(());
    }

    // Circular dependency check
    if stack.contains(&canonical) {
        let mut chain: Vec<String> = stack.iter()
            .skip_while(|p| **p != canonical)
            .map(|p| p.display().to_string())
            .collect();
        chain.push(canonical.display().to_string());
        return Err(ResolveError::CircularDependency(chain));
    }

    stack.push(canonical.clone());

    // Read and parse the file
    let source = std::fs::read_to_string(&canonical)
        .map_err(|e| ResolveError::Io(format!("{}: {}", canonical.display(), e)))?;
    let tops = parse_file(&source)?;

    let parent_dir = canonical.parent().unwrap_or(Path::new("."));
    let mut directives = Vec::new();

    // Find and resolve imports
    for top in &tops {
        if let TopLevel::Import { path, alias, only, .. } = top {
            // Sandbox check: no absolute paths, no ..
            if path.starts_with('/') || path.contains("..") {
                return Err(ResolveError::SandboxViolation(
                    format!("{}: path '{}' violates sandbox (no absolute paths, no ..)",
                            canonical.display(), path)));
            }

            let import_path = parent_dir.join(path);
            resolve_recursive(&import_path, resolved, visited, stack, import_map)?;

            let module_name = Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let prefix = alias.clone().unwrap_or_else(|| module_name.clone());

            directives.push(ImportDirective {
                prefix,
                only: only.clone(),
                module_name,
            });
        }
    }

    if !directives.is_empty() {
        import_map.insert(canonical.clone(), directives);
    }

    // Collect public symbols
    let mut public_fns = Vec::new();
    let mut public_types = Vec::new();
    for top in &tops {
        match top {
            TopLevel::Defn(f) if f.is_public => public_fns.push(f.name.clone()),
            TopLevel::DefType(td) if td.is_public => public_types.push(td.name.clone()),
            _ => {}
        }
    }

    let name = canonical.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    resolved.push(ResolvedModule {
        path: canonical.clone(),
        name,
        tops,
        public_fns,
        public_types,
    });

    visited.insert(canonical.clone());
    stack.pop();

    Ok(())
}

fn parse_file(source: &str) -> Result<Vec<TopLevel>, ResolveError> {
    use airl_syntax::{Lexer, parse_sexpr_all, Diagnostics};
    use airl_syntax::parser;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all()
        .map_err(|d| ResolveError::Parse(d.message))?;
    let sexprs = parse_sexpr_all(&tokens)
        .map_err(|d| ResolveError::Parse(d.message))?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(TopLevel::Expr(expr)),
                    Err(_) => return Err(ResolveError::Parse(d.message)),
                }
            }
        }
    }

    Ok(tops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn resolve_no_imports() {
        let dir = tempfile::tempdir().unwrap();
        let main = write_temp_file(dir.path(), "main.airl", "(defn foo :sig [(x : i64) -> i64] :body x)\n(foo 42)");
        let (modules, imports) = resolve_imports(main.to_str().unwrap()).unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "main");
        assert!(imports.is_empty());
    }

    #[test]
    fn resolve_single_import() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_file(dir.path(), "math.airl",
            "(defn abs :pub :sig [(x : i64) -> i64] :body (if (< x 0) (- 0 x) x))");
        let main = write_temp_file(dir.path(), "main.airl",
            "(import \"math.airl\")\n(math.abs -5)");
        let (modules, imports) = resolve_imports(main.to_str().unwrap()).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].name, "math"); // dependency first
        assert_eq!(modules[1].name, "main");
        assert_eq!(modules[0].public_fns, vec!["abs".to_string()]);
        let main_imports = imports.get(&std::fs::canonicalize(&main).unwrap()).unwrap();
        assert_eq!(main_imports[0].prefix, "math");
    }

    #[test]
    fn resolve_import_with_alias() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_file(dir.path(), "math.airl",
            "(defn abs :pub :sig [(x : i64) -> i64] :body (if (< x 0) (- 0 x) x))");
        let main = write_temp_file(dir.path(), "main.airl",
            "(import \"math.airl\" :as m)\n(m.abs -5)");
        let (_, imports) = resolve_imports(main.to_str().unwrap()).unwrap();
        let main_imports = imports.get(&std::fs::canonicalize(&main).unwrap()).unwrap();
        assert_eq!(main_imports[0].prefix, "m");
        assert_eq!(main_imports[0].module_name, "math");
    }

    #[test]
    fn resolve_circular_dependency() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_file(dir.path(), "a.airl", "(import \"b.airl\")\n(defn a-fn :pub :sig [-> i64] :body 1)");
        write_temp_file(dir.path(), "b.airl", "(import \"a.airl\")\n(defn b-fn :pub :sig [-> i64] :body 2)");
        let a = dir.path().join("a.airl");
        let result = resolve_imports(a.to_str().unwrap());
        assert!(matches!(result, Err(ResolveError::CircularDependency(_))));
    }

    #[test]
    fn resolve_diamond_dependency() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_file(dir.path(), "base.airl",
            "(defn base-fn :pub :sig [-> i64] :body 1)");
        write_temp_file(dir.path(), "left.airl",
            "(import \"base.airl\")\n(defn left-fn :pub :sig [-> i64] :body (base.base-fn))");
        write_temp_file(dir.path(), "right.airl",
            "(import \"base.airl\")\n(defn right-fn :pub :sig [-> i64] :body (base.base-fn))");
        let main = write_temp_file(dir.path(), "main.airl",
            "(import \"left.airl\")\n(import \"right.airl\")\n(+ (left.left-fn) (right.right-fn))");
        let (modules, _) = resolve_imports(main.to_str().unwrap()).unwrap();
        // base should appear exactly once, before left and right
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names.iter().filter(|&&n| n == "base").count(), 1);
        let base_pos = names.iter().position(|&n| n == "base").unwrap();
        let main_pos = names.iter().position(|&n| n == "main").unwrap();
        assert!(base_pos < main_pos);
    }

    #[test]
    fn resolve_sandbox_violation() {
        let dir = tempfile::tempdir().unwrap();
        let main = write_temp_file(dir.path(), "main.airl",
            "(import \"/etc/passwd\")");
        let result = resolve_imports(main.to_str().unwrap());
        assert!(matches!(result, Err(ResolveError::SandboxViolation(_))));
    }
}
```

- [ ] **Step 2: Add `tempfile` dev-dependency for resolver tests**

In `crates/airl-driver/Cargo.toml`, add under `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 3: Register the module in `lib.rs`**

In `crates/airl-driver/src/lib.rs`, add:

```rust
pub mod resolver;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver -- resolver`
Expected: All 5 resolver tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/resolver.rs crates/airl-driver/src/lib.rs crates/airl-driver/Cargo.toml
git commit -m "feat(modules): add import resolver with circular dep detection"
```

---

### Task 5: Wire Resolver into Pipeline — Qualified Name Rewriting

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`

This is the core integration. The strategy: when imports are present, the pipeline resolves them, then for each module compiles its functions with a module prefix. Qualified names like `math.abs` in user code are rewritten to `math_abs` (matching the prefixed function name). `:only` imports are rewritten from bare `abs` to `math_abs`.

- [ ] **Step 1: Add `run_file_with_imports` to pipeline.rs**

Add a new public function that handles import resolution and multi-module compilation:

```rust
/// Run a file with import resolution. Entry point for files that use `(import ...)`.
pub fn run_file_with_imports(entry_path: &str) -> Result<Value, PipelineError> {
    use crate::resolver::{resolve_imports, ResolveError, ImportDirective};

    let (modules, import_map) = resolve_imports(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    // Build a map: module_name → list of public function names
    let mut module_publics: HashMap<String, Vec<String>> = HashMap::new();
    for module in &modules {
        module_publics.insert(module.name.clone(), module.public_fns.clone());
    }

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
        (SET_SOURCE, "set"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    let entry_canonical = std::fs::canonicalize(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    // Compile and load each module in dependency order
    for module in &modules {
        let is_entry = module.path == entry_canonical;
        let directives = import_map.get(&module.path);

        // Filter tops: remove Import nodes, skip top-level exprs for non-entry modules
        let filtered_tops: Vec<&airl_syntax::ast::TopLevel> = module.tops.iter()
            .filter(|t| !matches!(t, airl_syntax::ast::TopLevel::Import { .. }))
            .filter(|t| is_entry || !matches!(t, airl_syntax::ast::TopLevel::Expr(_)))
            .collect();

        // For non-entry modules: compile functions with module name as prefix for their lambdas,
        // and register functions with prefixed names (module_name + "_" + fn_name)
        let prefix = &module.name;

        let ownership_map = build_ownership_map_refs(&filtered_tops);
        let (ir_nodes, contracts, fn_meta) = compile_tops_with_contracts_refs(&filtered_tops);

        // Rewrite qualified names in IR
        let rewritten_ir = if is_entry {
            rewrite_qualified_names(&ir_nodes, directives.map(|d| d.as_slice()).unwrap_or(&[]), &module_publics)
        } else {
            ir_nodes
        };

        let mut bc_compiler = BytecodeCompiler::with_prefix(prefix);
        bc_compiler.set_ownership_map(ownership_map);

        if is_entry {
            // Entry module: compile as the main program (has __main__)
            let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&rewritten_ir, &contracts);
            for func in funcs {
                vm.load_function(func);
            }
            vm.load_function(main_func);
            for meta in fn_meta {
                vm.store_fn_metadata(meta);
            }
        } else {
            // Imported module: compile functions and register them with qualified names
            let (funcs, _main) = bc_compiler.compile_program_with_contracts(&rewritten_ir, &contracts);
            for mut func in funcs {
                // Register with qualified name: math_abs
                let qualified_name = format!("{}_{}", module.name, func.name);
                func.name = qualified_name;
                vm.load_function(func);
            }
        }
    }

    #[cfg(feature = "jit")]
    vm.jit_full_compile_all();
    vm.exec_main().map_err(PipelineError::Runtime)
}
```

- [ ] **Step 2: Implement `rewrite_qualified_names` function**

This rewrites `math.abs` → `math_abs` in IR nodes, and handles `:only` imports:

```rust
/// Rewrite qualified names (e.g., `math.abs`) to flat names (e.g., `math_abs`) in IR,
/// and rewrite bare names from `:only` imports.
fn rewrite_qualified_names(
    nodes: &[IRNode],
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> Vec<IRNode> {
    nodes.iter().map(|node| rewrite_ir_node(node, directives, module_publics)).collect()
}

fn rewrite_ir_node(
    node: &IRNode,
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> IRNode {
    match node {
        IRNode::Ref(name) => {
            // Check for qualified name: prefix.symbol
            if let Some(dot_pos) = name.find('.') {
                let prefix = &name[..dot_pos];
                let symbol = &name[dot_pos + 1..];
                // Find which directive matches this prefix
                for d in directives {
                    if d.prefix == prefix {
                        return IRNode::Ref(format!("{}_{}", d.module_name, symbol));
                    }
                }
            }
            // Check for :only imports (bare name that should be rewritten)
            for d in directives {
                if let Some(only_list) = &d.only {
                    if only_list.contains(name) {
                        return IRNode::Ref(format!("{}_{}", d.module_name, name));
                    }
                }
            }
            node.clone()
        }
        IRNode::Call(name, args) => {
            let rewritten_args: Vec<IRNode> = args.iter()
                .map(|a| rewrite_ir_node(a, directives, module_publics))
                .collect();
            // Check for qualified call
            if let Some(dot_pos) = name.find('.') {
                let prefix = &name[..dot_pos];
                let symbol = &name[dot_pos + 1..];
                for d in directives {
                    if d.prefix == prefix {
                        return IRNode::Call(format!("{}_{}", d.module_name, symbol), rewritten_args);
                    }
                }
            }
            // Check :only imports
            for d in directives {
                if let Some(only_list) = &d.only {
                    if only_list.contains(name) {
                        return IRNode::Call(format!("{}_{}", d.module_name, name), rewritten_args);
                    }
                }
            }
            IRNode::Call(name.clone(), rewritten_args)
        }
        IRNode::Func(name, params, body) => {
            let rewritten_body = rewrite_ir_node(body, directives, module_publics);
            IRNode::Func(name.clone(), params.clone(), Box::new(rewritten_body))
        }
        IRNode::If(cond, then_b, else_b) => {
            IRNode::If(
                Box::new(rewrite_ir_node(cond, directives, module_publics)),
                Box::new(rewrite_ir_node(then_b, directives, module_publics)),
                Box::new(rewrite_ir_node(else_b, directives, module_publics)),
            )
        }
        IRNode::Let(name, val, body) => {
            IRNode::Let(
                name.clone(),
                Box::new(rewrite_ir_node(val, directives, module_publics)),
                Box::new(rewrite_ir_node(body, directives, module_publics)),
            )
        }
        IRNode::Do(nodes) => {
            IRNode::Do(nodes.iter().map(|n| rewrite_ir_node(n, directives, module_publics)).collect())
        }
        IRNode::Lambda(params, body) => {
            IRNode::Lambda(params.clone(), Box::new(rewrite_ir_node(body, directives, module_publics)))
        }
        IRNode::Match(scrutinee, arms) => {
            let new_scrutinee = Box::new(rewrite_ir_node(scrutinee, directives, module_publics));
            let new_arms: Vec<(IRNode, IRNode)> = arms.iter()
                .map(|(pat, body)| (pat.clone(), rewrite_ir_node(body, directives, module_publics)))
                .collect();
            IRNode::Match(new_scrutinee, new_arms)
        }
        // Leaf nodes — no rewriting needed
        _ => node.clone(),
    }
}
```

- [ ] **Step 3: Add helper functions for working with `&[&TopLevel]`**

The resolver gives us `Vec<TopLevel>` per module but we need to work with filtered subsets. Add these thin wrappers:

```rust
fn build_ownership_map_refs(tops: &[&airl_syntax::ast::TopLevel]) -> HashMap<String, Vec<bool>> {
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

fn compile_tops_with_contracts_refs(
    tops: &[&airl_syntax::ast::TopLevel],
) -> (
    Vec<IRNode>,
    HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
    Vec<airl_runtime::bytecode::FnDefMetadata>,
) {
    let owned: Vec<airl_syntax::ast::TopLevel> = tops.iter().map(|t| (*t).clone()).collect();
    compile_tops_with_contracts(&owned)
}
```

- [ ] **Step 4: Modify `cmd_run` in `main.rs` to use import-aware path**

In `main.rs`, in the `cmd_run` function, after the preloads check (line ~83), before the AOT compile path, add import detection:

```rust
// Check if the file uses imports — if so, use the import-aware pipeline
if !preloads.is_empty() {
    // existing preload path...
    return;
}

// Check if file has imports
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

// No preloads or imports: AOT compile path...
```

- [ ] **Step 5: Build and fix any compilation errors**

Run: `cargo build --features jit,aot`

Fix any type mismatches or missing imports. The key areas likely to need adjustment:
- Ensure `IRNode` variants used in `rewrite_ir_node` match the actual IR enum. Check `crates/airl-runtime/src/ir.rs` for the exact variant names and shapes.
- The `compile_tops_with_contracts_refs` wrapper avoids changing the existing function signature.

- [ ] **Step 6: Run all existing tests to verify no regressions**

Run: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
Expected: All ~508 existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs crates/airl-driver/src/main.rs
git commit -m "feat(modules): wire import resolver into pipeline with qualified name rewriting"
```

---

### Task 6: Integration Tests — Multi-File Import

**Files:**
- Create: `tests/fixtures/modules/math_lib.airl`
- Create: `tests/fixtures/modules/string_lib.airl`
- Create: `tests/fixtures/valid/import_basic.airl`
- Create: `tests/fixtures/valid/import_alias.airl`
- Create: `tests/fixtures/valid/import_only.airl`

- [ ] **Step 1: Create helper modules for tests**

`tests/fixtures/modules/math_lib.airl`:
```clojure
(defn my-abs :pub
  :sig [(x : i64) -> i64]
  :body (if (< x 0) (- 0 x) x))

(defn my-square :pub
  :sig [(x : i64) -> i64]
  :body (* x x))

;; Private helper — should NOT be accessible from importers
(defn internal-helper
  :sig [(x : i64) -> i64]
  :body (+ x 1))
```

`tests/fixtures/modules/string_lib.airl`:
```clojure
(defn greet :pub
  :sig [(name : Str) -> Str]
  :body (str "Hello, " name "!"))
```

- [ ] **Step 2: Create basic import test fixture**

`tests/fixtures/valid/import_basic.airl`:
```clojure
;; EXPECT: 25
(import "modules/math_lib.airl")
(math_lib.my-square 5)
```

Note: The fixture runner uses `run_source` which doesn't resolve files. We need a different approach — write a dedicated test function.

- [ ] **Step 3: Add a multi-file import integration test to `fixtures.rs`**

In `crates/airl-driver/tests/fixtures.rs`, add a test that uses `run_file_with_imports`:

```rust
#[test]
fn import_basic_prefix() {
    let root = fixtures_root();
    let test_file = root.join("valid").join("import_basic.airl");
    if !test_file.exists() {
        return; // Skip if fixture not yet created
    }
    let result = airl_driver::pipeline::run_file_with_imports(test_file.to_str().unwrap());
    match result {
        Ok(v) => assert_eq!(format!("{}", v), "25", "import_basic.airl"),
        Err(e) => panic!("import_basic.airl failed: {}", e),
    }
}

#[test]
fn import_alias() {
    let root = fixtures_root();
    let test_file = root.join("valid").join("import_alias.airl");
    if !test_file.exists() {
        return;
    }
    let result = airl_driver::pipeline::run_file_with_imports(test_file.to_str().unwrap());
    match result {
        Ok(v) => assert_eq!(format!("{}", v), "5", "import_alias.airl"),
        Err(e) => panic!("import_alias.airl failed: {}", e),
    }
}

#[test]
fn import_only() {
    let root = fixtures_root();
    let test_file = root.join("valid").join("import_only.airl");
    if !test_file.exists() {
        return;
    }
    let result = airl_driver::pipeline::run_file_with_imports(test_file.to_str().unwrap());
    match result {
        Ok(v) => assert_eq!(format!("{}", v), "25", "import_only.airl"),
        Err(e) => panic!("import_only.airl failed: {}", e),
    }
}
```

- [ ] **Step 4: Create test fixture files**

`tests/fixtures/valid/import_basic.airl`:
```clojure
;; EXPECT: 25
(import "modules/math_lib.airl")
(math_lib.my-square 5)
```

`tests/fixtures/valid/import_alias.airl`:
```clojure
;; EXPECT: 5
(import "modules/math_lib.airl" :as m)
(m.my-abs -5)
```

`tests/fixtures/valid/import_only.airl`:
```clojure
;; EXPECT: 25
(import "modules/math_lib.airl" :only [my-square])
(my-square 5)
```

- [ ] **Step 5: Run integration tests**

Run: `cargo test -p airl-driver -- import`
Expected: All 3 import integration tests pass.

- [ ] **Step 6: Debug and fix any issues**

Common issues to expect:
- IR node shapes may not match — check `crates/airl-runtime/src/ir.rs` for exact variants
- `run_file_with_imports` path resolution — the fixture test file path must be absolute for canonicalize
- Qualified name `math_lib.my-square` — the dot is part of the symbol, need to verify the lexer passes it through. If the lexer splits on `.`, the parser would see it as a different token. **Key concern:** check if the lexer treats `.` as a symbol character.

If the lexer does NOT include `.` in symbols: qualified names like `math.abs` will parse as `(. math abs)` or fail. In that case, we need to handle this in the parser or expression compiler — either by adding `.` to the symbol character set, or by treating `(. prefix symbol)` as a qualified reference. Check `crates/airl-syntax/src/lexer.rs` for the symbol character set.

- [ ] **Step 7: Commit**

```bash
git add tests/fixtures/modules/ tests/fixtures/valid/import_*.airl crates/airl-driver/tests/fixtures.rs
git commit -m "test(modules): add multi-file import integration tests"
```

---

### Task 7: Handle Qualified Names in Lexer/Parser

**Files:**
- Modify: `crates/airl-syntax/src/lexer.rs` (potentially)
- Modify: `crates/airl-syntax/src/parser.rs` (potentially)

**Note:** This task handles the critical question of how `math.abs` is lexed and parsed. The implementation depends on what we find.

- [ ] **Step 1: Check how the lexer handles dots in symbols**

Read the lexer's symbol character detection logic. Search for the `is_symbol_char` function or equivalent.

```bash
cargo test -p airl-syntax -- 2>&1 | head -5  # Verify current tests pass
```

Then write a quick parser test:

```rust
#[test]
fn parse_qualified_name() {
    let e = parse_expr_str("(math.abs -5)");
    // What does this produce? FnCall with SymbolRef("math.abs")?
    // Or does it fail? Or produce something else?
    match &e.kind {
        ExprKind::FnCall(callee, args) => {
            if let ExprKind::SymbolRef(name) = &callee.kind {
                assert_eq!(name, "math.abs");
            } else {
                panic!("expected SymbolRef, got {:?}", callee.kind);
            }
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected FnCall, got {:?}", e.kind),
    }
}
```

- [ ] **Step 2: If the lexer already passes dots through as part of symbols — great, no changes needed**

If `math.abs` is lexed as a single `Symbol("math.abs")` token, the parser produces `SymbolRef("math.abs")` and our IR rewriting in Task 5 handles it.

- [ ] **Step 3: If the lexer does NOT include dots in symbols — add `.` to symbol characters**

In `crates/airl-syntax/src/lexer.rs`, find the function that determines symbol characters (e.g., `is_symbol_char` or similar). Add `.` to the set, BUT be careful:
- Floats like `3.14` must still lex as `Float(3.14)`, not `Symbol("3.14")`
- The lexer should only include `.` in symbols when the token starts with a non-digit letter

If this is too risky (could break float lexing), use an alternative approach: in `parse_expr`, detect the pattern `SymbolRef("math") . SymbolRef("abs")` and merge them into `SymbolRef("math.abs")`. Or detect it in `compile_expr`.

- [ ] **Step 4: Run all tests after the fix**

Run: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
Expected: All existing tests pass, plus the new `parse_qualified_name` test.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-syntax/src/lexer.rs crates/airl-syntax/src/parser.rs
git commit -m "feat(modules): support qualified names (math.abs) in lexer/parser"
```

---

### Task 8: Visibility Enforcement

**Files:**
- Modify: `crates/airl-driver/src/resolver.rs`
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Write a test for private symbol rejection**

Add to resolver tests:

```rust
#[test]
fn rewrite_rejects_private_symbol() {
    // When a user tries to call a private function via qualified name,
    // the pipeline should error at compile time.
    // This is enforced during name rewriting — if a qualified name
    // references a function that isn't in the module's public_fns list.
}
```

Actually, enforcement is cleaner in the rewriting pass. Add a `validate_qualified_names` function.

- [ ] **Step 2: Add visibility validation to `rewrite_qualified_names`**

In `pipeline.rs`, modify `rewrite_ir_node` so that when it encounters a qualified name `prefix.symbol`, it checks that `symbol` is in the module's `public_fns`:

Change `rewrite_qualified_names` to return `Result<Vec<IRNode>, PipelineError>`:

```rust
fn rewrite_qualified_names(
    nodes: &[IRNode],
    directives: &[crate::resolver::ImportDirective],
    module_publics: &HashMap<String, Vec<String>>,
) -> Result<Vec<IRNode>, PipelineError> {
    nodes.iter()
        .map(|node| rewrite_ir_node(node, directives, module_publics))
        .collect()
}
```

In `rewrite_ir_node`, when a qualified name `prefix.symbol` is found:

```rust
for d in directives {
    if d.prefix == prefix {
        if let Some(publics) = module_publics.get(&d.module_name) {
            if !publics.contains(&symbol.to_string()) {
                return Err(PipelineError::Io(format!(
                    "error: '{}' is private in module '{}' — add :pub to export it",
                    symbol, d.module_name
                )));
            }
        }
        return Ok(IRNode::Ref(format!("{}_{}", d.module_name, symbol)));
    }
}
```

- [ ] **Step 3: Update callers of `rewrite_qualified_names` to handle `Result`**

In `run_file_with_imports`, the call now returns `Result`:

```rust
let rewritten_ir = if is_entry {
    rewrite_qualified_names(&ir_nodes, directives.map(|d| d.as_slice()).unwrap_or(&[]), &module_publics)?
} else {
    ir_nodes
};
```

- [ ] **Step 4: Create a fixture test for private access rejection**

`tests/fixtures/type_errors/import_private.airl`:
```clojure
;; ERROR: private
(import "modules/math_lib.airl")
(math_lib.internal-helper 5)
```

Add test to `fixtures.rs`:

```rust
#[test]
fn import_private_rejected() {
    let root = fixtures_root();
    let test_file = root.join("type_errors").join("import_private.airl");
    if !test_file.exists() {
        return;
    }
    let result = airl_driver::pipeline::run_file_with_imports(test_file.to_str().unwrap());
    assert!(result.is_err(), "accessing private symbol should fail");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("private"), "error should mention 'private': {}", err_msg);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p airl-driver -- import`
Expected: All import tests pass, including private access rejection.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs tests/fixtures/type_errors/import_private.airl crates/airl-driver/tests/fixtures.rs
git commit -m "feat(modules): enforce :pub visibility on cross-module access"
```

---

### Task 9: Backward Compatibility — Single-File Mode

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs` (if needed)

- [ ] **Step 1: Verify all existing fixture tests pass unchanged**

Run: `cargo test -p airl-driver -- fixtures`
Expected: All valid/ and error/ fixture tests pass. Single-file programs without `import` use the existing pipeline path with no changes.

- [ ] **Step 2: Verify files without :pub don't enforce visibility in single-file mode**

Per the spec: "files without any imports don't enforce visibility (everything is effectively public in single-file mode)". This is automatic because only `run_file_with_imports` does visibility checking, and single-file programs never enter that path.

Write a quick sanity test:

```rust
#[test]
fn single_file_no_pub_still_works() {
    // Functions without :pub should work normally in single-file mode
    let result = airl_driver::pipeline::run_source(
        "(defn foo :sig [(x : i64) -> i64] :body (* x x))\n(foo 7)"
    );
    assert_eq!(format!("{}", result.unwrap()), "49");
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
Expected: All tests pass.

- [ ] **Step 4: Commit (if any changes were needed)**

```bash
git add -A
git commit -m "test(modules): verify backward compatibility for single-file programs"
```

---

### Task 10: AOT Compilation with Imports

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`

The `compile_to_object` path also needs import support for `airl compile`.

- [ ] **Step 1: Add `compile_to_object_with_imports` function**

In `pipeline.rs`:

```rust
/// AOT compile a file with import resolution to a native object.
#[cfg(feature = "aot")]
pub fn compile_to_object_with_imports(entry_path: &str) -> Result<Vec<u8>, PipelineError> {
    use airl_runtime::bytecode::BytecodeFunc;
    use airl_runtime::bytecode_aot::BytecodeAot;
    use crate::resolver::resolve_imports;

    let (modules, import_map) = resolve_imports(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    let mut module_publics: HashMap<String, Vec<String>> = HashMap::new();
    for module in &modules {
        module_publics.insert(module.name.clone(), module.public_fns.clone());
    }

    let mut all_funcs: Vec<BytecodeFunc> = Vec::new();

    // 1. Compile stdlib
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
    ] {
        let (funcs, _) = compile_source_to_bytecode(src, name)?;
        all_funcs.extend(funcs);
    }

    let entry_canonical = std::fs::canonicalize(entry_path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;

    // 2. Compile each module in dependency order
    for module in &modules {
        let is_entry = module.path == entry_canonical;
        let directives = import_map.get(&module.path);

        let filtered_tops: Vec<&airl_syntax::ast::TopLevel> = module.tops.iter()
            .filter(|t| !matches!(t, airl_syntax::ast::TopLevel::Import { .. }))
            .filter(|t| is_entry || !matches!(t, airl_syntax::ast::TopLevel::Expr(_)))
            .collect();

        let prefix = &module.name;
        let (ir_nodes, contracts, _) = compile_tops_with_contracts_refs(&filtered_tops);

        let rewritten_ir = if is_entry {
            rewrite_qualified_names(&ir_nodes, directives.map(|d| d.as_slice()).unwrap_or(&[]), &module_publics)?
        } else {
            ir_nodes
        };

        let mut bc_compiler = BytecodeCompiler::with_prefix(prefix);

        if is_entry {
            let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&rewritten_ir, &contracts);
            all_funcs.extend(funcs);
            all_funcs.push(main_func);
        } else {
            let (funcs, _) = bc_compiler.compile_program_with_contracts(&rewritten_ir, &contracts);
            for mut func in funcs {
                let qualified = format!("{}_{}", module.name, func.name);
                func.name = qualified;
                all_funcs.push(func);
            }
        }
    }

    // 3. AOT compile
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new().map_err(|e| PipelineError::Runtime(
        airl_runtime::error::RuntimeError::TypeError(e)
    ))?;

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
```

- [ ] **Step 2: Modify `cmd_compile` in main.rs to detect imports**

In `cmd_compile`, before the existing `compile_to_object` call, add:

```rust
// If single file and has imports, use import-aware path
if files.len() == 1 {
    let source_check = std::fs::read_to_string(&files[0]).unwrap_or_default();
    if source_check.contains("(import ") {
        use airl_driver::pipeline::compile_to_object_with_imports;
        let obj_bytes = match compile_to_object_with_imports(&files[0]) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Compilation error: {}", e);
                std::process::exit(1);
            }
        };
        // ... same write + link logic as existing path ...
    }
}
```

- [ ] **Step 3: Build and test**

Run: `cargo build --features jit,aot && cargo test -p airl-driver`
Expected: Clean build, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs crates/airl-driver/src/main.rs
git commit -m "feat(modules): AOT compilation support for files with imports"
```

---

### Task 11: Update Documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `AIRL-Header.md` (if it exists in the worktree)

- [ ] **Step 1: Add module system to CLAUDE.md**

Add to the Completed Tasks section:

```markdown
- **Module System (v0.6.1)** — File-based `(import ...)` with `:pub` visibility, qualified names (`math.abs`), `:as` aliases, `:only` selective imports. Import resolver with circular dependency detection and sandbox enforcement. Qualified name rewriting at IR level. Works with both VM (run) and AOT (compile) paths. Backward compatible — single-file programs unchanged.
```

Add to CLAUDE.md Standard Library section or a new Module System section:

```markdown
## Module System

### Import Syntax

```clojure
(import "lib/math.airl")             ;; prefix access: (math.abs -5)
(import "lib/math.airl" :as m)      ;; alias: (m.abs -5)
(import "lib/math.airl" :only [abs]) ;; bare: (abs -5)
```

### Export

```clojure
(defn abs :pub                       ;; visible to importers
  :sig [(x : i64) -> i64]
  :body (if (< x 0) (- 0 x) x))
```

- `:pub` on defn/deftype makes it visible to importers
- Without `:pub`, functions are module-private
- Stdlib is always available without import
- Import paths are relative to the importing file's directory
- No absolute paths or `..` (sandbox constraint)
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document module system syntax and usage"
```

---

### Task 12: Final Verification

- [ ] **Step 1: Run full test suite**

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: All tests pass (existing ~508 + new module system tests).

- [ ] **Step 2: Manual smoke test — run a multi-file program**

Create a temp test:

```bash
mkdir -p /tmp/airl-module-test
cat > /tmp/airl-module-test/utils.airl << 'EOF'
(defn double :pub
  :sig [(x : i64) -> i64]
  :body (* x 2))
EOF

cat > /tmp/airl-module-test/main.airl << 'EOF'
(import "utils.airl")
(print (utils.double 21))
EOF

cargo run --features jit -- run /tmp/airl-module-test/main.airl
```

Expected output: `42`

- [ ] **Step 3: Verify backward compatibility with existing programs**

```bash
cargo run --features jit -- run tests/fixtures/valid/arithmetic.airl
cargo run --features jit -- run tests/fixtures/valid/lambda.airl
cargo run --features jit -- run tests/fixtures/valid/contracts.airl
```

Expected: All run successfully with same output as before.

- [ ] **Step 4: Clean up temp files**

```bash
rm -rf /tmp/airl-module-test
```
