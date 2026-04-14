use std::collections::HashMap;
use std::path::Path;
use std::io::{BufRead, Write};
use crate::{VerifyResult, FunctionVerification};

/// Serializable cache entry for a single function's Z3 results.
#[derive(Debug)]
struct CacheEntry {
    function_name: String,
    ensures_results: Vec<(String, String)>,     // (clause_text, result_tag)
    invariants_results: Vec<(String, String)>,
}

/// Disk-backed cache of Z3 verification results.
pub struct DiskCache {
    entries: HashMap<u64, CacheEntry>,
    dirty: bool,
}

impl DiskCache {
    pub fn new() -> Self {
        DiskCache { entries: HashMap::new(), dirty: false }
    }

    pub fn load(path: &Path) -> Self {
        let mut entries = HashMap::new();
        if let Ok(file) = std::fs::File::open(path) {
            let reader = std::io::BufReader::new(file);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if let Some(entry) = Self::parse_line(&line) {
                        entries.insert(entry.0, entry.1);
                    }
                }
            }
        }
        DiskCache { entries, dirty: false }
    }

    fn parse_line(line: &str) -> Option<(u64, CacheEntry)> {
        // NDJSON format: {"key":12345,"fn":"name","ensures":[["clause","Proven"]],"invariants":[]}
        // Hand-parse to avoid serde dependency
        let key = Self::extract_u64(line, "\"key\":")?;
        let fn_name = Self::extract_string(line, "\"fn\":\"")?;
        let ensures = Self::extract_clause_results(line, "\"ensures\":")?;
        let invariants = Self::extract_clause_results(line, "\"invariants\":")?;
        Some((key, CacheEntry {
            function_name: fn_name,
            ensures_results: ensures,
            invariants_results: invariants,
        }))
    }

    fn extract_u64(s: &str, prefix: &str) -> Option<u64> {
        let start = s.find(prefix)? + prefix.len();
        let end = s[start..].find(|c: char| !c.is_ascii_digit())? + start;
        s[start..end].parse().ok()
    }

    fn extract_string(s: &str, prefix: &str) -> Option<String> {
        let start = s.find(prefix)? + prefix.len();
        let end = s[start..].find('"')? + start;
        Some(s[start..end].to_string())
    }

    fn extract_clause_results(s: &str, prefix: &str) -> Option<Vec<(String, String)>> {
        let start = s.find(prefix)? + prefix.len();
        let bracket_start = s[start..].find('[')? + start;
        // Find matching close bracket
        let mut depth = 0;
        let mut end = bracket_start;
        for (i, c) in s[bracket_start..].chars().enumerate() {
            match c {
                '[' => depth += 1,
                ']' => { depth -= 1; if depth == 0 { end = bracket_start + i; break; } }
                _ => {}
            }
        }
        let inner = &s[bracket_start+1..end];
        if inner.trim().is_empty() { return Some(vec![]); }

        let mut results = vec![];
        // Parse pairs like ["clause","Proven"]
        let mut pos = 0;
        while let Some(pair_start) = inner[pos..].find('[') {
            let abs_start = pos + pair_start;
            if let Some(pair_end) = inner[abs_start..].find(']') {
                let pair = &inner[abs_start+1..abs_start+pair_end];
                let parts: Vec<&str> = pair.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let clause = parts[0].trim().trim_matches('"').to_string();
                    let result = parts[1].trim().trim_matches('"').to_string();
                    results.push((clause, result));
                }
                pos = abs_start + pair_end + 1;
            } else {
                break;
            }
        }
        Some(results)
    }

    pub fn get(&self, key: u64) -> Option<FunctionVerification> {
        let entry = self.entries.get(&key)?;
        Some(FunctionVerification {
            function_name: entry.function_name.clone(),
            ensures_results: entry.ensures_results.iter().map(|(c, r)| {
                (c.clone(), Self::parse_result_tag(r))
            }).collect(),
            invariants_results: entry.invariants_results.iter().map(|(c, r)| {
                (c.clone(), Self::parse_result_tag(r))
            }).collect(),
        })
    }

    fn parse_result_tag(tag: &str) -> VerifyResult {
        match tag {
            "Proven" => VerifyResult::Proven,
            "Unknown" => VerifyResult::Unknown("cached".to_string()),
            "TranslationError" => VerifyResult::TranslationError("cached".to_string()),
            _ => VerifyResult::Unknown(format!("unknown cache tag: {}", tag)),
        }
        // Note: Disproven results are NOT cached (they should fail compilation)
    }

    pub fn insert(&mut self, key: u64, verification: &FunctionVerification) {
        // Never cache Disproven results — they cause hard compilation errors
        // and must always be re-detected on recompilation.
        if verification.has_disproven() {
            return;
        }
        self.entries.insert(key, CacheEntry {
            function_name: verification.function_name.clone(),
            ensures_results: verification.ensures_results.iter()
                .filter(|(_, r)| !matches!(r, VerifyResult::Disproven { .. }))
                .map(|(c, r)| (c.clone(), Self::result_tag(r)))
                .collect(),
            invariants_results: verification.invariants_results.iter()
                .filter(|(_, r)| !matches!(r, VerifyResult::Disproven { .. }))
                .map(|(c, r)| (c.clone(), Self::result_tag(r)))
                .collect(),
        });
        self.dirty = true;
    }

    fn result_tag(r: &VerifyResult) -> String {
        match r {
            VerifyResult::Proven => "Proven".to_string(),
            VerifyResult::Disproven { .. } => "Disproven".to_string(),
            VerifyResult::Unknown(_) => "Unknown".to_string(),
            VerifyResult::TranslationError(_) => "TranslationError".to_string(),
        }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if !self.dirty { return Ok(()); }
        let mut file = std::fs::File::create(path)?;
        for (key, entry) in &self.entries {
            let ensures_json = Self::clauses_to_json(&entry.ensures_results);
            let invariants_json = Self::clauses_to_json(&entry.invariants_results);
            writeln!(file, "{{\"key\":{},\"fn\":\"{}\",\"ensures\":{},\"invariants\":{}}}",
                key, entry.function_name, ensures_json, invariants_json)?;
        }
        Ok(())
    }

    fn clauses_to_json(clauses: &[(String, String)]) -> String {
        if clauses.is_empty() { return "[]".to_string(); }
        let pairs: Vec<String> = clauses.iter().map(|(c, r)| {
            format!("[\"{}\",\"{}\"]", c.replace('"', "\\\""), r.replace('"', "\\\""))
        }).collect();
        format!("[{}]", pairs.join(","))
    }

    /// Remove entries for functions not in the current source.
    pub fn evict_stale(&mut self, current_keys: &[u64]) {
        let current_set: std::collections::HashSet<u64> = current_keys.iter().copied().collect();
        self.entries.retain(|k, _| current_set.contains(k));
        self.dirty = true;
    }
}
