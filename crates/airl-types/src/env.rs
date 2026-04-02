use crate::ty::{Ty, Symbol};
use crate::interner::{SymbolId, SymbolInterner};
use std::collections::HashMap;

/// A registered type definition.
#[derive(Debug, Clone)]
pub struct RegisteredType {
    pub name: Symbol,
    pub params: Vec<Symbol>,       // type parameter names
    pub ty: Ty,                     // the full type (with TypeVars for params)
}

/// Scoped type environment for type checking.
///
/// Uses a flat shadow index for O(1) variable lookup. A single HashMap maps
/// SymbolId -> (Ty, depth) so that `lookup` never walks a scope stack.
/// `push_scope` / `pop_scope` maintain the depth counter and a per-depth list
/// of bindings introduced, enabling efficient cleanup on scope exit.
///
/// When a binding shadows an outer one, the previous entry is saved to a
/// restore list so `pop_scope` can reinstate it.
#[derive(Debug)]
pub struct TypeEnv {
    /// Current scope depth (0 = global).
    depth: usize,
    /// Flat shadow index: SymbolId -> (type, depth at which it was bound).
    shadow: HashMap<SymbolId, (Ty, usize)>,
    /// Per-depth list of (symbol, Option<previous_entry>) for rollback on pop.
    scope_bindings: Vec<Vec<(SymbolId, Option<(Ty, usize)>)>>,
    /// Registered named types (ADTs, aliases).
    types: HashMap<Symbol, RegisteredType>,
    /// Symbol interner shared with the checker.
    pub interner: SymbolInterner,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            depth: 0,
            shadow: HashMap::new(),
            scope_bindings: vec![Vec::new()],
            types: HashMap::new(),
            interner: SymbolInterner::new(),
        }
    }

    pub fn push_scope(&mut self) {
        self.depth += 1;
        self.scope_bindings.push(Vec::new());
    }

    pub fn pop_scope(&mut self) {
        if self.depth == 0 {
            return;
        }
        if let Some(bindings) = self.scope_bindings.pop() {
            for (id, prev) in bindings.into_iter().rev() {
                match prev {
                    Some(entry) => {
                        self.shadow.insert(id, entry);
                    }
                    None => {
                        self.shadow.remove(&id);
                    }
                }
            }
        }
        self.depth -= 1;
    }

    /// Bind a name in the current scope. Accepts a string name, interns it,
    /// and inserts into the flat shadow index.
    pub fn bind(&mut self, name: Symbol, ty: Ty) {
        let id = self.interner.intern(&name);
        self.bind_id(id, ty);
    }

    /// Bind using an already-interned SymbolId.
    pub fn bind_id(&mut self, id: SymbolId, ty: Ty) {
        let prev = self.shadow.insert(id, (ty, self.depth));
        if let Some(scope) = self.scope_bindings.last_mut() {
            scope.push((id, prev));
        }
    }

    /// Look up a binding by string name. O(1) via the shadow index.
    pub fn lookup(&self, name: &str) -> Option<&Ty> {
        let id = self.interner.get(name)?;
        self.shadow.get(&id).map(|(ty, _)| ty)
    }

    /// Look up a binding by SymbolId. O(1).
    pub fn lookup_id(&self, id: SymbolId) -> Option<&Ty> {
        self.shadow.get(&id).map(|(ty, _)| ty)
    }

    /// Intern a name without binding it (useful for lookups).
    pub fn intern(&mut self, name: &str) -> SymbolId {
        self.interner.intern(name)
    }

    /// Resolve a SymbolId back to its string.
    pub fn resolve(&self, id: SymbolId) -> &str {
        self.interner.resolve(id)
    }

    pub fn register_type(&mut self, name: Symbol, params: Vec<Symbol>, ty: Ty) {
        self.types.insert(name.clone(), RegisteredType { name, params, ty });
    }

    pub fn lookup_type(&self, name: &str) -> Option<&RegisteredType> {
        self.types.get(name)
    }

    /// Current scope depth.
    pub fn current_depth(&self) -> usize {
        self.depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::PrimTy;

    #[test]
    fn binding_and_lookup() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I32)));
        assert_eq!(env.lookup("y"), None);
    }

    #[test]
    fn scoping_shadows() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));
        env.push_scope();
        env.bind("x".into(), Ty::Prim(PrimTy::F64));
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::F64)));
        env.pop_scope();
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I32)));
    }

    #[test]
    fn inner_scope_sees_outer() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));
        env.push_scope();
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I32)));
    }

    #[test]
    fn register_and_lookup_type() {
        let mut env = TypeEnv::new();
        env.register_type(
            "Result".into(),
            vec!["T".into(), "E".into()],
            Ty::Sum(vec![]),
        );
        assert!(env.lookup_type("Result").is_some());
        assert!(env.lookup_type("Option").is_none());
    }

    // ── New tests for shadow index correctness ──

    #[test]
    fn multiple_scopes_restore_correctly() {
        let mut env = TypeEnv::new();
        env.bind("a".into(), Ty::Prim(PrimTy::I32));
        env.push_scope();
        env.bind("a".into(), Ty::Prim(PrimTy::F64));
        env.bind("b".into(), Ty::Prim(PrimTy::Bool));
        env.push_scope();
        env.bind("a".into(), Ty::Prim(PrimTy::Str));
        assert_eq!(env.lookup("a"), Some(&Ty::Prim(PrimTy::Str)));
        assert_eq!(env.lookup("b"), Some(&Ty::Prim(PrimTy::Bool)));

        env.pop_scope();
        assert_eq!(env.lookup("a"), Some(&Ty::Prim(PrimTy::F64)));
        assert_eq!(env.lookup("b"), Some(&Ty::Prim(PrimTy::Bool)));

        env.pop_scope();
        assert_eq!(env.lookup("a"), Some(&Ty::Prim(PrimTy::I32)));
        assert_eq!(env.lookup("b"), None);
    }

    #[test]
    fn pop_global_scope_is_noop() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));
        env.pop_scope(); // should not panic or remove the binding
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I32)));
    }

    #[test]
    fn bind_id_and_lookup_id() {
        let mut env = TypeEnv::new();
        let id = env.intern("x");
        env.bind_id(id, Ty::Prim(PrimTy::I64));
        assert_eq!(env.lookup_id(id), Some(&Ty::Prim(PrimTy::I64)));
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn shadow_same_scope_overwrites() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));
        env.bind("x".into(), Ty::Prim(PrimTy::F64));
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::F64)));
    }

    #[test]
    fn depth_tracking() {
        let mut env = TypeEnv::new();
        assert_eq!(env.current_depth(), 0);
        env.push_scope();
        assert_eq!(env.current_depth(), 1);
        env.push_scope();
        assert_eq!(env.current_depth(), 2);
        env.pop_scope();
        assert_eq!(env.current_depth(), 1);
        env.pop_scope();
        assert_eq!(env.current_depth(), 0);
    }

    #[test]
    fn same_name_rebind_in_scope_restores_on_pop() {
        let mut env = TypeEnv::new();
        env.bind("x".into(), Ty::Prim(PrimTy::I32));   // depth 0
        env.push_scope();                                // depth 1
        env.bind("x".into(), Ty::Prim(PrimTy::F64));   // first rebind in scope 1
        env.bind("x".into(), Ty::Prim(PrimTy::Str));   // second rebind in scope 1
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::Str)));
        env.pop_scope();
        // Must restore the depth-0 binding, not a phantom from forward iteration
        assert_eq!(env.lookup("x"), Some(&Ty::Prim(PrimTy::I32)));
    }

    #[test]
    fn interned_symbols_are_stable() {
        let mut env = TypeEnv::new();
        let id1 = env.intern("hello");
        let id2 = env.intern("hello");
        assert_eq!(id1, id2);
        assert_eq!(env.resolve(id1), "hello");
    }
}
