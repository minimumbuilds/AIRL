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
}
