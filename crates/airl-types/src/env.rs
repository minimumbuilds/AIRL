use crate::ty::{Ty, Symbol, TyVariant, TyField};
use std::collections::HashMap;

/// A registered type definition.
#[derive(Debug, Clone)]
pub struct RegisteredType {
    pub name: Symbol,
    pub params: Vec<Symbol>,       // type parameter names
    pub ty: Ty,                     // the full type (with TypeVars for params)
}

#[derive(Debug)]
struct Scope {
    bindings: HashMap<Symbol, Ty>,
}

/// Scoped type environment for type checking.
#[derive(Debug)]
pub struct TypeEnv {
    scopes: Vec<Scope>,
    types: HashMap<Symbol, RegisteredType>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope { bindings: HashMap::new() }],
            types: HashMap::new(),
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope { bindings: HashMap::new() });
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn bind(&mut self, name: Symbol, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.bindings.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn register_type(&mut self, name: Symbol, params: Vec<Symbol>, ty: Ty) {
        self.types.insert(name.clone(), RegisteredType { name, params, ty });
    }

    pub fn lookup_type(&self, name: &str) -> Option<&RegisteredType> {
        self.types.get(name)
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
}
