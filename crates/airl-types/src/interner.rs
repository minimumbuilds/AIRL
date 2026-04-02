use std::collections::HashMap;

/// Compact identifier for an interned string. Cheap to copy, hash, and compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Interns strings so that repeated lookups use a u32 key instead of hashing
/// the full string each time. Cost is paid once per unique name; all subsequent
/// operations use the SymbolId.
#[derive(Debug)]
pub struct SymbolInterner {
    strings: Vec<String>,
    lookup: HashMap<String, SymbolId>,
}

impl SymbolInterner {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Intern a string, returning its SymbolId. If already interned, returns
    /// the existing id without allocating.
    pub fn intern(&mut self, name: &str) -> SymbolId {
        if let Some(&id) = self.lookup.get(name) {
            return id;
        }
        let id = SymbolId(self.strings.len() as u32);
        self.strings.push(name.to_string());
        self.lookup.insert(name.to_string(), id);
        id
    }

    /// Resolve a SymbolId back to its string. Panics if the id is invalid.
    pub fn resolve(&self, id: SymbolId) -> &str {
        &self.strings[id.0 as usize]
    }

    /// Number of unique symbols interned.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether the interner is empty.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Look up a string without interning it. Returns None if not yet interned.
    pub fn get(&self, name: &str) -> Option<SymbolId> {
        self.lookup.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_same_id() {
        let mut interner = SymbolInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn different_strings_get_different_ids() {
        let mut interner = SymbolInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("world");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_roundtrip() {
        let mut interner = SymbolInterner::new();
        let id = interner.intern("foo");
        assert_eq!(interner.resolve(id), "foo");
    }

    #[test]
    fn len_tracks_unique_strings() {
        let mut interner = SymbolInterner::new();
        interner.intern("a");
        interner.intern("b");
        interner.intern("a"); // duplicate
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn empty_interner() {
        let interner = SymbolInterner::new();
        assert!(interner.is_empty());
        assert_eq!(interner.len(), 0);
    }

    #[test]
    fn sequential_ids() {
        let mut interner = SymbolInterner::new();
        assert_eq!(interner.intern("a"), SymbolId(0));
        assert_eq!(interner.intern("b"), SymbolId(1));
        assert_eq!(interner.intern("c"), SymbolId(2));
    }
}
