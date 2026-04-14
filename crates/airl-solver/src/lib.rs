pub mod translate;
pub mod prover;
pub mod cache;

use std::collections::HashMap;

/// Content-addressed cache key for a function's verification-relevant content.
pub fn cache_key(def: &airl_syntax::ast::FnDef) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    def.name.hash(&mut h);
    for p in &def.params {
        p.name.hash(&mut h);
        format!("{:?}", p.ty.kind).hash(&mut h);
    }
    format!("{:?}", def.return_type.kind).hash(&mut h);
    for r in &def.requires { format!("{:?}", r).hash(&mut h); }
    for e in &def.ensures { format!("{:?}", e).hash(&mut h); }
    for i in &def.invariants { format!("{:?}", i).hash(&mut h); }
    format!("{:?}", def.body).hash(&mut h);
    h.finish()
}

/// Cache of Z3 verification results, keyed by function name then clause source text.
/// Passed to the bytecode compiler so proven contracts skip opcode emission.
pub struct ProofCache {
    results: HashMap<String, HashMap<String, VerifyResult>>,
}

impl ProofCache {
    pub fn new() -> Self {
        Self { results: HashMap::new() }
    }

    pub fn insert(&mut self, fn_name: &str, clause: &str, result: VerifyResult) {
        self.results
            .entry(fn_name.to_string())
            .or_default()
            .insert(clause.to_string(), result);
    }

    pub fn is_proven(&self, fn_name: &str, clause: &str) -> bool {
        self.results.get(fn_name)
            .and_then(|m| m.get(clause))
            .map_or(false, |r| matches!(r, VerifyResult::Proven))
    }

    /// Extract the set of (fn_name, clause) pairs that were proven.
    /// Used to pass to the bytecode compiler for opcode elision.
    pub fn into_proven_set(self) -> std::collections::HashSet<(String, String)> {
        let mut set = std::collections::HashSet::new();
        for (fn_name, clauses) in self.results {
            for (clause, result) in clauses {
                if matches!(result, VerifyResult::Proven) {
                    set.insert((fn_name.clone(), clause));
                }
            }
        }
        set
    }
}

/// Result of attempting to prove a single contract clause.
#[derive(Debug, Clone)]
pub enum VerifyResult {
    /// Z3 proved the clause holds for all inputs satisfying :requires.
    Proven,
    /// Z3 found inputs that satisfy :requires but violate :ensures.
    Disproven { counterexample: Vec<(String, String)> },
    /// Z3 could not determine — fall back to runtime checking.
    Unknown(String),
    /// The clause could not be translated to Z3 (unsupported expression).
    TranslationError(String),
}

/// Verification results for a complete function.
#[derive(Debug, Clone)]
pub struct FunctionVerification {
    pub function_name: String,
    pub ensures_results: Vec<(String, VerifyResult)>,
    pub invariants_results: Vec<(String, VerifyResult)>,
}

impl FunctionVerification {
    pub fn all_proven(&self) -> bool {
        self.ensures_results.iter().all(|(_, r)| matches!(r, VerifyResult::Proven))
            && self.invariants_results.iter().all(|(_, r)| matches!(r, VerifyResult::Proven))
    }

    pub fn has_disproven(&self) -> bool {
        self.ensures_results.iter().any(|(_, r)| matches!(r, VerifyResult::Disproven { .. }))
            || self.invariants_results.iter().any(|(_, r)| matches!(r, VerifyResult::Disproven { .. }))
    }
}

impl std::fmt::Display for VerifyResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyResult::Proven => write!(f, "proven"),
            VerifyResult::Disproven { counterexample } => {
                write!(f, "disproven")?;
                if !counterexample.is_empty() {
                    write!(f, " (counterexample: ")?;
                    for (i, (name, val)) in counterexample.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{} = {}", name, val)?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            VerifyResult::Unknown(reason) => write!(f, "unknown: {}", reason),
            VerifyResult::TranslationError(msg) => write!(f, "translation error: {}", msg),
        }
    }
}
