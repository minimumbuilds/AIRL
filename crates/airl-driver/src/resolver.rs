use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use airl_syntax::ast::TopLevel;
use airl_syntax::{Diagnostics, Lexer, parse_sexpr_all, parse_top_level};

/// A resolved module with its parsed contents and public symbol list.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub path: PathBuf,
    pub name: String,
    pub tops: Vec<TopLevel>,
    pub public_fns: Vec<String>,
    pub public_types: Vec<String>,
}

/// How a module was imported by a specific file.
#[derive(Debug, Clone)]
pub struct ImportDirective {
    pub prefix: String,
    pub only: Option<Vec<String>>,
    pub module_name: String,
}

#[derive(Debug)]
pub enum ResolveError {
    Io(String),
    Parse(String),
    CircularDependency(Vec<String>),
    SandboxViolation(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Io(msg) => write!(f, "I/O error: {}", msg),
            ResolveError::Parse(msg) => write!(f, "parse error: {}", msg),
            ResolveError::CircularDependency(chain) => {
                write!(f, "circular dependency: {}", chain.join(" -> "))
            }
            ResolveError::SandboxViolation(path) => {
                write!(f, "sandbox violation: path '{}' is not allowed (no absolute paths or '..')", path)
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolve all imports starting from `entry_path`, returning modules in
/// dependency order (leaves first, entry last) and per-file import directives.
pub fn resolve_imports(
    entry_path: &str,
) -> Result<(Vec<ResolvedModule>, HashMap<PathBuf, Vec<ImportDirective>>), ResolveError> {
    let canonical = std::fs::canonicalize(entry_path)
        .map_err(|e| ResolveError::Io(format!("{}: {}", entry_path, e)))?;

    let mut resolved: Vec<ResolvedModule> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    let mut import_map: HashMap<PathBuf, Vec<ImportDirective>> = HashMap::new();

    resolve_recursive(
        &canonical,
        &mut resolved,
        &mut visited,
        &mut stack,
        &mut import_map,
    )?;

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

    if visited.contains(&canonical) {
        return Ok(());
    }

    if stack.contains(&canonical) {
        // Build the cycle chain from where the cycle starts
        let cycle_start = stack.iter().position(|p| p == &canonical).unwrap();
        let mut chain: Vec<String> = stack[cycle_start..]
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        chain.push(canonical.display().to_string());
        return Err(ResolveError::CircularDependency(chain));
    }

    stack.push(canonical.clone());

    let source = std::fs::read_to_string(&canonical)
        .map_err(|e| ResolveError::Io(format!("{}: {}", canonical.display(), e)))?;
    let tops = parse_file(&source)?;

    let parent = canonical.parent().unwrap_or(Path::new("."));
    let mut directives: Vec<ImportDirective> = Vec::new();

    for top in &tops {
        if let TopLevel::Import { path, alias, only, .. } = top {
            // Sandbox check
            if path.starts_with('/') || path.contains("..") {
                return Err(ResolveError::SandboxViolation(path.clone()));
            }

            let import_path = parent.join(path);
            resolve_recursive(&import_path, resolved, visited, stack, import_map)?;

            let stem = Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(path)
                .to_string();

            let prefix = alias.clone().unwrap_or_else(|| stem.clone());

            directives.push(ImportDirective {
                prefix,
                only: only.clone(),
                module_name: stem,
            });
        }
    }

    // Collect public symbols
    let mut public_fns = Vec::new();
    let mut public_types = Vec::new();
    for top in &tops {
        match top {
            TopLevel::Defn(fndef) if fndef.is_public => {
                public_fns.push(fndef.name.clone());
            }
            TopLevel::DefType(typedef) if typedef.is_public => {
                public_types.push(typedef.name.clone());
            }
            _ => {}
        }
    }

    let name = canonical
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    resolved.push(ResolvedModule {
        path: canonical.clone(),
        name,
        tops,
        public_fns,
        public_types,
    });

    import_map.insert(canonical.clone(), directives);
    visited.insert(canonical.clone());
    stack.pop();

    Ok(())
}

fn parse_file(source: &str) -> Result<Vec<TopLevel>, ResolveError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer
        .lex_all()
        .map_err(|e| ResolveError::Parse(format!("lex error: {}", e.message)))?;
    let sexprs = parse_sexpr_all(&tokens)
        .map_err(|e| ResolveError::Parse(format!("s-expr error: {}", e.message)))?;

    let mut diags = Diagnostics::new();
    let mut tops = Vec::new();
    for sexpr in &sexprs {
        let top = parse_top_level(sexpr, &mut diags)
            .map_err(|e| ResolveError::Parse(format!("parse error: {}", e.message)))?;
        tops.push(top);
    }
    Ok(tops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_temp_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::File::create(&path).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn resolve_no_imports() {
        let dir = tempdir().unwrap();
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(defn add [x : Int y : Int] : Int (+ x y))"#,
        );
        let (modules, imports) = resolve_imports(main_path.to_str().unwrap()).unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "main");
        let canon = std::fs::canonicalize(&main_path).unwrap();
        assert!(imports.get(&canon).unwrap().is_empty());
    }

    #[test]
    fn resolve_single_import() {
        let dir = tempdir().unwrap();
        write_temp_file(
            dir.path(),
            "math.airl",
            r#"(defn square :pub [x : Int] : Int (* x x))"#,
        );
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(import "math.airl")
(defn main [] : Int (math/square 5))"#,
        );
        let (modules, _imports) = resolve_imports(main_path.to_str().unwrap()).unwrap();
        assert_eq!(modules.len(), 2);
        // math should come first (dependency order: leaves first)
        assert_eq!(modules[0].name, "math");
        assert_eq!(modules[1].name, "main");
        // math has a public fn
        assert_eq!(modules[0].public_fns, vec!["square".to_string()]);
    }

    #[test]
    fn resolve_import_with_alias() {
        let dir = tempdir().unwrap();
        write_temp_file(
            dir.path(),
            "math.airl",
            r#"(defn square :pub [x : Int] : Int (* x x))"#,
        );
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(import "math.airl" :as m)
(defn main [] : Int (m/square 5))"#,
        );
        let (modules, imports) = resolve_imports(main_path.to_str().unwrap()).unwrap();
        assert_eq!(modules.len(), 2);
        let canon = std::fs::canonicalize(main_path).unwrap();
        let directives = imports.get(&canon).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].prefix, "m");
        assert_eq!(directives[0].module_name, "math");
    }

    #[test]
    fn resolve_circular_dependency() {
        let dir = tempdir().unwrap();
        write_temp_file(
            dir.path(),
            "a.airl",
            r#"(import "b.airl")
(defn fa [] : Int 1)"#,
        );
        write_temp_file(
            dir.path(),
            "b.airl",
            r#"(import "a.airl")
(defn fb [] : Int 2)"#,
        );
        let a_path = dir.path().join("a.airl");
        let result = resolve_imports(a_path.to_str().unwrap());
        assert!(result.is_err());
        match result.unwrap_err() {
            ResolveError::CircularDependency(chain) => {
                assert!(chain.len() >= 2, "chain should have at least 2 entries");
                // The chain should start and end with the same file
                let first = &chain[0];
                let last = chain.last().unwrap();
                assert_eq!(first, last, "cycle should start and end at the same file");
            }
            other => panic!("expected CircularDependency, got {:?}", other),
        }
    }

    #[test]
    fn resolve_diamond_dependency() {
        let dir = tempdir().unwrap();
        write_temp_file(
            dir.path(),
            "base.airl",
            r#"(defn id :pub [x : Int] : Int x)"#,
        );
        write_temp_file(
            dir.path(),
            "left.airl",
            r#"(import "base.airl")
(defn left-fn [] : Int (base/id 1))"#,
        );
        write_temp_file(
            dir.path(),
            "right.airl",
            r#"(import "base.airl")
(defn right-fn [] : Int (base/id 2))"#,
        );
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(import "left.airl")
(import "right.airl")
(defn main [] : Int 0)"#,
        );
        let (modules, _imports) = resolve_imports(main_path.to_str().unwrap()).unwrap();
        // base should appear exactly once
        let base_count = modules.iter().filter(|m| m.name == "base").count();
        assert_eq!(base_count, 1, "base should appear exactly once");
        // base should come before main
        let base_idx = modules.iter().position(|m| m.name == "base").unwrap();
        let main_idx = modules.iter().position(|m| m.name == "main").unwrap();
        assert!(base_idx < main_idx, "base should come before main");
        assert_eq!(modules.len(), 4); // base, left, right, main
    }

    #[test]
    fn resolve_sandbox_violation() {
        let dir = tempdir().unwrap();
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(import "/etc/passwd")
(defn main [] : Int 0)"#,
        );
        let result = resolve_imports(main_path.to_str().unwrap());
        assert!(result.is_err());
        match result.unwrap_err() {
            ResolveError::SandboxViolation(path) => {
                assert_eq!(path, "/etc/passwd");
            }
            other => panic!("expected SandboxViolation, got {:?}", other),
        }
    }

    #[test]
    fn resolve_sandbox_dotdot() {
        let dir = tempdir().unwrap();
        let main_path = write_temp_file(
            dir.path(),
            "main.airl",
            r#"(import "../secret.airl")
(defn main [] : Int 0)"#,
        );
        let result = resolve_imports(main_path.to_str().unwrap());
        assert!(result.is_err());
        match result.unwrap_err() {
            ResolveError::SandboxViolation(path) => {
                assert_eq!(path, "../secret.airl");
            }
            other => panic!("expected SandboxViolation, got {:?}", other),
        }
    }
}
