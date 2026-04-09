use crate::ty::{DimExpr, DimOp, Symbol};
use std::collections::HashMap;

/// Substitution map from dimension variables to concrete expressions.
pub type DimSubst = HashMap<Symbol, DimExpr>;

/// Unify two dimension expressions, producing substitutions.
/// Returns Ok(substitutions) on success, Err(message) on failure.
pub fn unify_dim(a: &DimExpr, b: &DimExpr, subst: &mut DimSubst) -> Result<(), String> {
    let a = apply_subst(a, subst);
    let b = apply_subst(b, subst);

    match (&a, &b) {
        // Both literals — must be equal
        (DimExpr::Lit(x), DimExpr::Lit(y)) => {
            if x == y { Ok(()) }
            else { Err(format!("dimension mismatch: {} vs {}", x, y)) }
        }
        // Variable unifies with anything
        (DimExpr::Var(v), other) | (other, DimExpr::Var(v)) => {
            if let DimExpr::Var(w) = other {
                if v == w { return Ok(()); }
            }
            // Apply substitutions to other before occurs check to detect indirect cycles
            let other_substituted = apply_subst(other, subst);
            if occurs(*v, &other_substituted) {
                return Err(format!("circular dimension: {} occurs in {:?}", v, other_substituted));
            }
            subst.insert(*v, other_substituted);
            Ok(())
        }
        // BinOp — try structural match
        (DimExpr::BinOp(op1, l1, r1), DimExpr::BinOp(op2, l2, r2)) if op1 == op2 => {
            unify_dim(l1, l2, subst)?;
            unify_dim(r1, r2, subst)
        }
        // Try evaluating to literals
        _ => {
            if let (Some(x), Some(y)) = (eval_dim(&a), eval_dim(&b)) {
                if x == y { Ok(()) }
                else { Err(format!("dimension mismatch: {} vs {}", x, y)) }
            } else {
                Err(format!("cannot unify dimensions: {:?} vs {:?}", a, b))
            }
        }
    }
}

/// Apply substitutions to a dim expression.
pub fn apply_subst(dim: &DimExpr, subst: &DimSubst) -> DimExpr {
    match dim {
        DimExpr::Var(v) => {
            if let Some(replacement) = subst.get(v) {
                apply_subst(replacement, subst)
            } else {
                dim.clone()
            }
        }
        DimExpr::BinOp(op, l, r) => {
            DimExpr::BinOp(*op, Box::new(apply_subst(l, subst)), Box::new(apply_subst(r, subst)))
        }
        DimExpr::Lit(_) => dim.clone(),
    }
}

/// Try to evaluate a dim expression to a concrete u64.
pub fn eval_dim(dim: &DimExpr) -> Option<u64> {
    match dim {
        DimExpr::Lit(v) => Some(*v),
        DimExpr::Var(_) => None,
        DimExpr::BinOp(op, l, r) => {
            let lv = eval_dim(l)?;
            let rv = eval_dim(r)?;
            match op {
                DimOp::Add => Some(lv + rv),
                DimOp::Sub => lv.checked_sub(rv),
                DimOp::Mul => Some(lv * rv),
            }
        }
    }
}

fn occurs(var: Symbol, dim: &DimExpr) -> bool {
    match dim {
        DimExpr::Var(v) => *v == var,
        DimExpr::Lit(_) => false,
        DimExpr::BinOp(_, l, r) => occurs(var, l) || occurs(var, r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner::SymbolInterner;

    /// Helper: create a DimSubst and SymbolInterner together for tests.
    fn make_interner() -> SymbolInterner {
        SymbolInterner::new()
    }

    #[test]
    fn unify_equal_lits() {
        let mut s = DimSubst::new();
        assert!(unify_dim(&DimExpr::Lit(64), &DimExpr::Lit(64), &mut s).is_ok());
    }

    #[test]
    fn unify_different_lits_fails() {
        let mut s = DimSubst::new();
        assert!(unify_dim(&DimExpr::Lit(32), &DimExpr::Lit(64), &mut s).is_err());
    }

    #[test]
    fn unify_var_with_lit() {
        let mut interner = make_interner();
        let m = interner.intern("M");
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var(m), &DimExpr::Lit(64), &mut s).unwrap();
        assert_eq!(s.get(&m), Some(&DimExpr::Lit(64)));
    }

    #[test]
    fn unify_shared_dimension() {
        // Matrix multiply: tensor[f32 M K] * tensor[f32 K N]
        // K must unify across both
        let mut interner = make_interner();
        let k = interner.intern("K");
        let mut s = DimSubst::new();
        // First call: K unifies with Lit(32)
        unify_dim(&DimExpr::Var(k), &DimExpr::Lit(32), &mut s).unwrap();
        // Second call: K (now 32) must match Lit(32)
        unify_dim(&DimExpr::Var(k), &DimExpr::Lit(32), &mut s).unwrap();
        assert_eq!(s.get(&k), Some(&DimExpr::Lit(32)));
    }

    #[test]
    fn unify_shared_dimension_mismatch() {
        let mut interner = make_interner();
        let k = interner.intern("K");
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var(k), &DimExpr::Lit(32), &mut s).unwrap();
        // K is now 32, trying to unify with 64 should fail
        assert!(unify_dim(&DimExpr::Var(k), &DimExpr::Lit(64), &mut s).is_err());
    }

    #[test]
    fn unify_two_vars() {
        let mut interner = make_interner();
        let m = interner.intern("M");
        let n = interner.intern("N");
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var(m), &DimExpr::Var(n), &mut s).unwrap();
        // M → N or N → M
        assert!(s.contains_key(&m) || s.contains_key(&n));
    }

    #[test]
    fn eval_binop() {
        let expr = DimExpr::BinOp(DimOp::Add, Box::new(DimExpr::Lit(3)), Box::new(DimExpr::Lit(4)));
        assert_eq!(eval_dim(&expr), Some(7));
    }

    #[test]
    fn eval_with_var_returns_none() {
        let mut interner = make_interner();
        let m = interner.intern("M");
        assert_eq!(eval_dim(&DimExpr::Var(m)), None);
    }
}
