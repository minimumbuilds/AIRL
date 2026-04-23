//! Implements the `airl verify-policy` subcommand and the baseline file
//! that tracks grandfathered :verify checked / :verify trusted modules.
//!
//! Baseline file format is a hand-rolled minimal TOML subset:
//!   version = 1
//!   grandfathered_checked = [ "path/a.airl", "path/b.airl#module" ]
//!   grandfathered_trusted = [ "path/c.airl" ]

use std::path::Path;

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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub version: u32,
    pub grandfathered_checked: Vec<BaselineKey>,
    pub grandfathered_trusted: Vec<BaselineKey>,
}

pub const BASELINE_VERSION: u32 = 1;
pub const BASELINE_FILE: &str = ".airl-verify-baseline.toml";

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
}
