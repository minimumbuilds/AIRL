use airl_syntax::ast::PatternKind;
use crate::ty::{Ty, PrimTy, TyVariant};
use crate::env::TypeEnv;

/// Check whether a set of match arms exhaustively covers the scrutinee type.
///
/// Returns `Ok(())` if the arms are exhaustive, or `Err` with a list of
/// human-readable descriptions of the missing patterns.
pub fn check_exhaustiveness(
    scrutinee_ty: &Ty,
    arms: &[&PatternKind],
    _env: &TypeEnv,
) -> Result<(), Vec<String>> {
    // A wildcard or binding pattern always makes the match exhaustive.
    for arm in arms {
        match arm {
            PatternKind::Wildcard | PatternKind::Binding(_) => return Ok(()),
            _ => {}
        }
    }

    match scrutinee_ty {
        Ty::Prim(PrimTy::Bool) => check_bool_exhaustiveness(arms),
        Ty::Sum(variants) => check_sum_exhaustiveness(variants, arms),
        // For types we don't yet model exhaustiveness for (integers, strings, etc.),
        // require a wildcard/binding (which was already checked above).
        _ => Err(vec!["non-exhaustive patterns: `_` not covered".to_string()]),
    }
}

fn check_bool_exhaustiveness(arms: &[&PatternKind]) -> Result<(), Vec<String>> {
    let mut has_true = false;
    let mut has_false = false;

    for arm in arms {
        match arm {
            PatternKind::Literal(airl_syntax::ast::LitPattern::Bool(true)) => has_true = true,
            PatternKind::Literal(airl_syntax::ast::LitPattern::Bool(false)) => has_false = true,
            PatternKind::Wildcard | PatternKind::Binding(_) => return Ok(()),
            _ => {}
        }
    }

    let mut missing = Vec::new();
    if !has_true {
        missing.push("true".to_string());
    }
    if !has_false {
        missing.push("false".to_string());
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn check_sum_exhaustiveness(
    variants: &[TyVariant],
    arms: &[&PatternKind],
) -> Result<(), Vec<String>> {
    // Collect the variant names that appear in the arms.
    let mut covered = std::collections::HashSet::new();

    for arm in arms {
        match arm {
            PatternKind::Variant(name, _) => {
                covered.insert(name.as_str());
            }
            PatternKind::Wildcard | PatternKind::Binding(_) => return Ok(()),
            _ => {}
        }
    }

    let missing: Vec<String> = variants
        .iter()
        .filter(|v| !covered.contains(v.name.as_str()))
        .map(|v| v.name.clone())
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::ast::{LitPattern, PatternKind};
    use crate::ty::{Ty, PrimTy, TyVariant};
    use crate::env::TypeEnv;

    fn result_type() -> Ty {
        Ty::Sum(vec![
            TyVariant { name: "Ok".into(), fields: vec![Ty::Prim(PrimTy::I32)] },
            TyVariant { name: "Err".into(), fields: vec![Ty::Prim(PrimTy::Str)] },
        ])
    }

    #[test]
    fn result_ok_and_err_is_exhaustive() {
        let env = TypeEnv::new();
        let ok = PatternKind::Variant("Ok".into(), vec![]);
        let err = PatternKind::Variant("Err".into(), vec![]);
        let arms: Vec<&PatternKind> = vec![&ok, &err];
        assert!(check_exhaustiveness(&result_type(), &arms, &env).is_ok());
    }

    #[test]
    fn result_only_ok_is_missing_err() {
        let env = TypeEnv::new();
        let ok = PatternKind::Variant("Ok".into(), vec![]);
        let arms: Vec<&PatternKind> = vec![&ok];
        let err = check_exhaustiveness(&result_type(), &arms, &env).unwrap_err();
        assert_eq!(err, vec!["Err".to_string()]);
    }

    #[test]
    fn bool_true_and_false_is_exhaustive() {
        let env = TypeEnv::new();
        let t = PatternKind::Literal(LitPattern::Bool(true));
        let f = PatternKind::Literal(LitPattern::Bool(false));
        let arms: Vec<&PatternKind> = vec![&t, &f];
        assert!(check_exhaustiveness(&Ty::Prim(PrimTy::Bool), &arms, &env).is_ok());
    }

    #[test]
    fn bool_only_true_is_missing_false() {
        let env = TypeEnv::new();
        let t = PatternKind::Literal(LitPattern::Bool(true));
        let arms: Vec<&PatternKind> = vec![&t];
        let err = check_exhaustiveness(&Ty::Prim(PrimTy::Bool), &arms, &env).unwrap_err();
        assert_eq!(err, vec!["false".to_string()]);
    }

    #[test]
    fn wildcard_catches_everything() {
        let env = TypeEnv::new();
        let w = PatternKind::Wildcard;
        let arms: Vec<&PatternKind> = vec![&w];
        assert!(check_exhaustiveness(&result_type(), &arms, &env).is_ok());
        assert!(check_exhaustiveness(&Ty::Prim(PrimTy::Bool), &arms, &env).is_ok());
    }

    #[test]
    fn binding_catches_everything() {
        let env = TypeEnv::new();
        let b = PatternKind::Binding("x".into());
        let arms: Vec<&PatternKind> = vec![&b];
        assert!(check_exhaustiveness(&result_type(), &arms, &env).is_ok());
        assert!(check_exhaustiveness(&Ty::Prim(PrimTy::Bool), &arms, &env).is_ok());
    }
}
