//! Implements the `airl verify-policy` subcommand and the baseline file
//! that tracks grandfathered :verify checked / :verify trusted modules.
//!
//! Baseline file format is a hand-rolled minimal TOML subset:
//!   version = 1
//!   grandfathered_checked = [ "path/a.airl", "path/b.airl#module" ]
//!   grandfathered_trusted = [ "path/c.airl" ]

use std::path::{Path, PathBuf};

/// An entry in the baseline — either a whole file or a file#name suffix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BaselineKey {
    pub path: String,
    /// Optional disambiguator (module name or top-level defn name).
    pub name: Option<String>,
}

impl BaselineKey {
    pub fn whole_file(path: impl Into<String>) -> Self {
        Self { path: path.into(), name: None }
    }

    pub fn qualified(path: impl Into<String>, name: impl Into<String>) -> Self {
        Self { path: path.into(), name: Some(name.into()) }
    }

    /// Format as it appears in the baseline file.
    pub fn to_string(&self) -> String {
        match &self.name {
            Some(n) => format!("{}#{}", self.path, n),
            None => self.path.clone(),
        }
    }

    /// Parse from a line string like "path/a.airl" or "path/b.airl#name".
    pub fn parse(s: &str) -> Self {
        if let Some(idx) = s.find('#') {
            Self {
                path: s[..idx].to_string(),
                name: Some(s[idx + 1..].to_string()),
            }
        } else {
            Self { path: s.to_string(), name: None }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub version: u32,
    pub grandfathered_checked: Vec<BaselineKey>,
    pub grandfathered_trusted: Vec<BaselineKey>,
}

pub const BASELINE_VERSION: u32 = 1;
pub const BASELINE_FILE: &str = ".airl-verify-baseline.toml";

impl Default for Baseline {
    fn default() -> Self { Self::new() }
}

impl Baseline {
    pub fn new() -> Self {
        Self {
            version: BASELINE_VERSION,
            grandfathered_checked: Vec::new(),
            grandfathered_trusted: Vec::new(),
        }
    }

    /// Parse the minimal TOML subset used by `.airl-verify-baseline.toml`.
    /// Supported grammar:
    ///   - Line comments starting with '#'
    ///   - `version = <int>`
    ///   - `<name> = [ ]` for empty arrays
    ///   - Multi-line string arrays: `<name> = [\n  "...",\n  "...",\n]`
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut version: Option<u32> = None;
        let mut checked: Vec<BaselineKey> = Vec::new();
        let mut trusted: Vec<BaselineKey> = Vec::new();

        let mut lines = src.lines().peekable();
        while let Some(raw) = lines.next() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("version") {
                let rest = rest.trim_start();
                let rest = rest.strip_prefix('=').ok_or("expected `=` after version")?.trim();
                let n: u32 = rest.parse().map_err(|_| format!("invalid version: {}", rest))?;
                version = Some(n);
                continue;
            }
            if let Some(rest) = line.strip_prefix("grandfathered_checked") {
                let entries = parse_array(rest, &mut lines)?;
                checked = entries.into_iter().map(|s| BaselineKey::parse(&s)).collect();
                continue;
            }
            if let Some(rest) = line.strip_prefix("grandfathered_trusted") {
                let entries = parse_array(rest, &mut lines)?;
                trusted = entries.into_iter().map(|s| BaselineKey::parse(&s)).collect();
                continue;
            }
            return Err(format!("unexpected line: {}", line));
        }

        let version = version.ok_or("baseline missing `version` field")?;
        Ok(Self {
            version,
            grandfathered_checked: checked,
            grandfathered_trusted: trusted,
        })
    }

    /// Render the baseline as a stable, sorted TOML string.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("# Managed by `airl verify-policy`. Hand-edits are allowed; CI validates\n");
        s.push_str("# consistency on every run. Remove an entry to ratchet — requires upgrading\n");
        s.push_str("# the module's :verify to proven and adding :ensures to every :pub defn.\n");
        s.push_str(&format!("version = {}\n", self.version));
        s.push_str("\n");
        s.push_str("grandfathered_checked = [\n");
        let mut checked: Vec<BaselineKey> = self.grandfathered_checked.clone();
        checked.sort();
        checked.dedup();
        for k in &checked {
            s.push_str(&format!("  \"{}\",\n", k.to_string()));
        }
        s.push_str("]\n");
        s.push_str("\n");
        s.push_str("grandfathered_trusted = [\n");
        let mut trusted: Vec<BaselineKey> = self.grandfathered_trusted.clone();
        trusted.sort();
        trusted.dedup();
        for k in &trusted {
            s.push_str(&format!("  \"{}\",\n", k.to_string()));
        }
        s.push_str("]\n");
        s
    }

    /// Read baseline from disk, or return `Ok(Baseline::new())` if missing.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Baseline::new());
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {}", path.display(), e))?;
        Baseline::parse(&src)
    }

    /// Write baseline to disk atomically-ish (write then rename).
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let rendered = self.render();
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, rendered.as_bytes())
            .map_err(|e| format!("writing {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("renaming to {}: {}", path.display(), e))?;
        Ok(())
    }
}

fn strip_comment(line: &str) -> &str {
    // Smart version: find '#' that is NOT inside a "..." string.
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_string = !in_string,
            b'#' if !in_string => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn parse_array<'a, I>(after_name: &str, lines: &mut std::iter::Peekable<I>) -> Result<Vec<String>, String>
where
    I: Iterator<Item = &'a str>,
{
    // after_name is the text after the array name on the same line,
    // e.g. " = [" or " = []" or " = [ \"a\", \"b\" ]"
    let rest = after_name.trim_start().strip_prefix('=').ok_or("expected `=` after array name")?.trim_start();
    // Concatenate all lines until we see the closing `]`.
    let mut buf = String::from(rest);
    if !buf.contains(']') {
        for next in lines.by_ref() {
            buf.push(' ');
            buf.push_str(strip_comment(next).trim());
            if buf.contains(']') {
                break;
            }
        }
    }
    // Strip brackets.
    let inner = buf
        .trim()
        .strip_prefix('[')
        .ok_or("expected `[` starting array")?
        .trim_end()
        .strip_suffix(']')
        .ok_or("expected `]` ending array")?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if p.is_empty() { continue; }
        let s = p
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| format!("array element not quoted: {}", p))?;
        out.push(s.to_string());
    }
    Ok(out)
}

/// Extract (key, level) pairs for every module and top-level defn in the file.
///
/// Multi-module files emit qualified keys (path#modname / path#defnname).
/// Single-entry files use a plain-path key only when the result is unambiguous.
pub fn extract_verify_entries(
    path: &str,
    tops: &[airl_syntax::ast::TopLevel],
) -> Vec<(BaselineKey, airl_syntax::ast::VerifyLevel)> {
    use airl_syntax::ast::TopLevel;

    let mut modules: Vec<(String, airl_syntax::ast::VerifyLevel)> = Vec::new();
    let mut bare_defns: Vec<(String, airl_syntax::ast::VerifyLevel)> = Vec::new();

    for top in tops {
        match top {
            TopLevel::Module(m) => {
                modules.push((m.name.clone(), m.verify));
            }
            TopLevel::Defn(f) => {
                let level = f.verify.unwrap_or_default();
                bare_defns.push((f.name.clone(), level));
            }
            _ => {}
        }
    }

    // Use a plain (whole-file) key only when there is exactly one module and no
    // bare top-level defns.  Bare defns always get a name-qualified key so the
    // function name is preserved regardless of how many top-level items there are.
    let single_module = modules.len() == 1 && bare_defns.is_empty();

    let mut out = Vec::new();
    for (name, level) in modules {
        let key = if single_module {
            BaselineKey::whole_file(path)
        } else {
            BaselineKey::qualified(path, name)
        };
        out.push((key, level));
    }
    for (name, level) in bare_defns {
        // Always qualified for bare defns.
        out.push((BaselineKey::qualified(path, name), level));
    }
    out
}

#[derive(Debug, Default)]
pub struct PolicyDiff {
    /// Keys at :verify checked in the tree but missing from grandfathered_checked.
    pub new_checked: Vec<BaselineKey>,
    /// Keys at :verify trusted in the tree but missing from grandfathered_trusted.
    pub new_trusted: Vec<BaselineKey>,
    /// Keys in grandfathered_checked but no longer at :verify checked in the tree.
    pub stale_checked: Vec<BaselineKey>,
    /// Keys in grandfathered_trusted but no longer at :verify trusted in the tree.
    pub stale_trusted: Vec<BaselineKey>,
}

impl PolicyDiff {
    /// "Clean" means no regressions. Stale entries are tolerated (they just
    /// mean the user has ratcheted but not yet run --prune).
    pub fn is_clean(&self) -> bool {
        self.new_checked.is_empty() && self.new_trusted.is_empty()
    }

    pub fn is_fully_clean(&self) -> bool {
        self.new_checked.is_empty()
            && self.new_trusted.is_empty()
            && self.stale_checked.is_empty()
            && self.stale_trusted.is_empty()
    }
}

pub fn compute_diff(
    baseline: &Baseline,
    scanned: &[(BaselineKey, airl_syntax::ast::VerifyLevel)],
) -> PolicyDiff {
    use std::collections::HashSet;
    use airl_syntax::ast::VerifyLevel;

    let baseline_checked: HashSet<&BaselineKey> = baseline.grandfathered_checked.iter().collect();
    let baseline_trusted: HashSet<&BaselineKey> = baseline.grandfathered_trusted.iter().collect();

    let mut scanned_checked: HashSet<&BaselineKey> = HashSet::new();
    let mut scanned_trusted: HashSet<&BaselineKey> = HashSet::new();
    for (key, level) in scanned {
        match level {
            VerifyLevel::Checked => { scanned_checked.insert(key); }
            VerifyLevel::Trusted => { scanned_trusted.insert(key); }
            VerifyLevel::Proven => {}
        }
    }

    let mut diff = PolicyDiff::default();
    for k in &scanned_checked {
        if !baseline_checked.contains(k) {
            diff.new_checked.push((*k).clone());
        }
    }
    for k in &scanned_trusted {
        if !baseline_trusted.contains(k) {
            diff.new_trusted.push((*k).clone());
        }
    }
    for k in &baseline_checked {
        if !scanned_checked.contains(k) {
            diff.stale_checked.push((*k).clone());
        }
    }
    for k in &baseline_trusted {
        if !scanned_trusted.contains(k) {
            diff.stale_trusted.push((*k).clone());
        }
    }
    diff.new_checked.sort();
    diff.new_trusted.sort();
    diff.stale_checked.sort();
    diff.stale_trusted.sort();
    diff
}

/// Walk the tree rooted at `root`, collecting `.airl` files.
/// Excludes `tests/fixtures/**` and anything under a `target/` or `.git/` directory.
pub fn enumerate_airl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        // Exclusions
        if rel_str.starts_with("tests/fixtures/") || rel_str.starts_with("tests\\fixtures\\") {
            continue;
        }
        if let Some(name) = path.file_name() {
            if name == "target" || name == ".git" {
                continue;
            }
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().map_or(false, |e| e == "airl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_key_whole_file_roundtrip() {
        let k = BaselineKey::whole_file("crates/foo/bar.airl");
        assert_eq!(k.to_string(), "crates/foo/bar.airl");
        assert_eq!(BaselineKey::parse("crates/foo/bar.airl"), k);
    }

    #[test]
    fn baseline_key_qualified_roundtrip() {
        let k = BaselineKey::qualified("crates/foo/bar.airl", "mymod");
        assert_eq!(k.to_string(), "crates/foo/bar.airl#mymod");
        assert_eq!(BaselineKey::parse("crates/foo/bar.airl#mymod"), k);
    }

    #[test]
    fn parse_baseline_minimal() {
        let src = r#"
version = 1
grandfathered_checked = [
  "crates/a.airl",
  "crates/b.airl#mod2",
]
grandfathered_trusted = [
  "bootstrap/x.airl",
]
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.version, 1);
        assert_eq!(b.grandfathered_checked.len(), 2);
        assert_eq!(b.grandfathered_checked[0], BaselineKey::whole_file("crates/a.airl"));
        assert_eq!(b.grandfathered_checked[1], BaselineKey::qualified("crates/b.airl", "mod2"));
        assert_eq!(b.grandfathered_trusted.len(), 1);
        assert_eq!(b.grandfathered_trusted[0], BaselineKey::whole_file("bootstrap/x.airl"));
    }

    #[test]
    fn parse_baseline_empty_arrays() {
        let src = r#"
version = 1
grandfathered_checked = []
grandfathered_trusted = []
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.version, 1);
        assert!(b.grandfathered_checked.is_empty());
        assert!(b.grandfathered_trusted.is_empty());
    }

    #[test]
    fn parse_baseline_ignores_comments_and_blank_lines() {
        let src = r#"
# a leading comment
version = 1

# another comment
grandfathered_checked = [
  "a.airl",  # inline comment
]
grandfathered_trusted = []
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.grandfathered_checked.len(), 1);
        assert_eq!(b.grandfathered_checked[0].path, "a.airl");
    }

    #[test]
    fn parse_baseline_rejects_missing_version() {
        let src = r#"
grandfathered_checked = []
grandfathered_trusted = []
"#;
        assert!(Baseline::parse(src).is_err());
    }

    #[test]
    fn baseline_writer_roundtrip() {
        let mut b = Baseline::new();
        b.grandfathered_checked = vec![
            BaselineKey::whole_file("crates/a.airl"),
            BaselineKey::qualified("crates/b.airl", "mod2"),
        ];
        b.grandfathered_trusted = vec![
            BaselineKey::whole_file("bootstrap/x.airl"),
        ];
        let rendered = b.render();
        let parsed = Baseline::parse(&rendered).expect("roundtrip parse failed");
        assert_eq!(parsed, b);
    }

    #[test]
    fn baseline_writer_sorts_entries() {
        let mut b = Baseline::new();
        b.grandfathered_checked = vec![
            BaselineKey::whole_file("z.airl"),
            BaselineKey::whole_file("a.airl"),
            BaselineKey::whole_file("m.airl"),
        ];
        let rendered = b.render();
        let a_pos = rendered.find("a.airl").unwrap();
        let m_pos = rendered.find("m.airl").unwrap();
        let z_pos = rendered.find("z.airl").unwrap();
        assert!(a_pos < m_pos && m_pos < z_pos, "entries not sorted:\n{}", rendered);
    }

    // ── Task 4.2 helpers ──────────────────────────────────────────────────────

    /// Parse AIRL source into a Vec<TopLevel> using the public airl-syntax API.
    fn parse_tops(src: &str) -> Vec<airl_syntax::ast::TopLevel> {
        use airl_syntax::{Lexer, parse_sexpr_all, parser, diagnostic::Diagnostics};
        let tokens = Lexer::new(src).lex_all().expect("lex_all failed");
        let sexprs = parse_sexpr_all(tokens).expect("parse_sexpr_all failed");
        let mut diags = Diagnostics::new();
        let mut tops = Vec::new();
        for sexpr in &sexprs {
            match parser::parse_top_level(sexpr, &mut diags) {
                Ok(top) => tops.push(top),
                Err(d) => panic!("parse_top_level failed: {:?}", d),
            }
        }
        tops
    }

    #[test]
    fn extract_entries_from_module() {
        let src = r#"(module foo :verify checked (defn x :pub :sig [-> i64] :requires [true] :body 0))"#;
        let tops = parse_tops(src);
        let entries = extract_verify_entries("path/foo.airl", &tops);
        assert_eq!(entries.len(), 1);
        let (key, level) = &entries[0];
        assert_eq!(key, &BaselineKey::whole_file("path/foo.airl"));
        assert_eq!(*level, airl_syntax::ast::VerifyLevel::Checked);
    }

    #[test]
    fn extract_entries_multi_module_file() {
        let src = r#"
          (module foo :verify checked (defn x :pub :sig [-> i64] :requires [true] :body 0))
          (module bar :verify trusted (defn y :pub :sig [-> i64] :requires [true] :body 0))
        "#;
        let tops = parse_tops(src);
        let entries = extract_verify_entries("path/f.airl", &tops);
        assert_eq!(entries.len(), 2);
        let names: Vec<Option<String>> = entries.iter().map(|(k, _)| k.name.clone()).collect();
        assert!(names.contains(&Some("foo".to_string())));
        assert!(names.contains(&Some("bar".to_string())));
    }

    #[test]
    fn extract_entries_top_level_defn() {
        let src = r#"(defn foo :pub :verify checked :sig [-> i64] :requires [true] :body 0)"#;
        let tops = parse_tops(src);
        let entries = extract_verify_entries("path/f.airl", &tops);
        assert_eq!(entries.len(), 1);
        let (key, level) = &entries[0];
        assert_eq!(key.name.as_deref(), Some("foo"));
        assert_eq!(*level, airl_syntax::ast::VerifyLevel::Checked);
    }

    // ── Task 4.3 tests ───────────────────────────────────────────────────────

    #[test]
    fn diff_detects_new_checked_module_not_in_baseline() {
        let b = Baseline::new();
        // Baseline is empty
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Checked),
        ];
        let diff = compute_diff(&b, &scanned);
        assert_eq!(diff.new_checked.len(), 1);
        assert_eq!(diff.new_checked[0], BaselineKey::whole_file("a.airl"));
        assert!(diff.new_trusted.is_empty());
        assert!(diff.stale_checked.is_empty());
    }

    #[test]
    fn diff_tolerates_upgraded_module_in_baseline() {
        let mut b = Baseline::new();
        b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Proven),
        ];
        let diff = compute_diff(&b, &scanned);
        assert!(diff.new_checked.is_empty(), "should not regress on upgraded module");
        assert_eq!(diff.stale_checked.len(), 1, "should report upgrade as prunable");
    }

    #[test]
    fn diff_clean_when_baseline_matches() {
        let mut b = Baseline::new();
        b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Checked),
        ];
        let diff = compute_diff(&b, &scanned);
        assert!(diff.is_clean(), "expected clean: {:?}", diff);
    }

    #[test]
    fn diff_flags_new_trusted_separately() {
        let b = Baseline::new();
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Trusted),
        ];
        let diff = compute_diff(&b, &scanned);
        assert_eq!(diff.new_trusted.len(), 1);
        assert!(diff.new_checked.is_empty());
    }

    #[test]
    fn scan_airl_files_excludes_fixtures() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::create_dir_all(root.join("tests/fixtures/valid")).unwrap();
        std::fs::write(root.join("crates/a/src/lib.airl"),
            "(module a (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();
        std::fs::write(root.join("tests/fixtures/valid/skip.airl"),
            "(module skip (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();
        std::fs::write(root.join("crates/a/src/notes.md"), "# not airl").unwrap();

        let files = enumerate_airl_files(root);
        let rel: Vec<String> = files.iter()
            .map(|p: &std::path::PathBuf| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(rel.iter().any(|p: &String| p == "crates/a/src/lib.airl"), "missing lib.airl: {:?}", rel);
        assert!(!rel.iter().any(|p: &String| p.starts_with("tests/fixtures/")), "included fixture: {:?}", rel);
    }
}
