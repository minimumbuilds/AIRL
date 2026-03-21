use airl_syntax::ast::{Pattern, PatternKind, LitPattern};
use crate::value::Value;

/// Attempt to match a pattern against a value.
/// Returns `Some(bindings)` on success, `None` on failure.
/// Bindings are `(name, value)` pairs captured during matching.
pub fn try_match(pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    match &pattern.kind {
        PatternKind::Wildcard => Some(vec![]),

        PatternKind::Binding(name) => {
            Some(vec![(name.clone(), value.clone())])
        }

        PatternKind::Literal(lit) => {
            if lit_matches(lit, value) {
                Some(vec![])
            } else {
                None
            }
        }

        PatternKind::Variant(name, sub_patterns) => {
            if let Value::Variant(vname, inner) = value {
                if vname != name {
                    return None;
                }
                // Single sub-pattern: match against inner value
                // No sub-patterns: match if variant name matches and inner is Unit
                // Multiple sub-patterns: inner should be a Tuple
                match sub_patterns.len() {
                    0 => {
                        // Nullary variant — inner must be Unit
                        if matches!(inner.as_ref(), Value::Unit) {
                            Some(vec![])
                        } else {
                            None
                        }
                    }
                    1 => {
                        try_match(&sub_patterns[0], inner)
                    }
                    _ => {
                        // Multiple sub-patterns: inner should be a Tuple
                        if let Value::Tuple(items) = inner.as_ref() {
                            if items.len() != sub_patterns.len() {
                                return None;
                            }
                            let mut bindings = Vec::new();
                            for (pat, val) in sub_patterns.iter().zip(items.iter()) {
                                match try_match(pat, val) {
                                    Some(mut bs) => bindings.append(&mut bs),
                                    None => return None,
                                }
                            }
                            Some(bindings)
                        } else {
                            None
                        }
                    }
                }
            } else {
                None
            }
        }
    }
}

/// Check if a literal pattern matches a value.
fn lit_matches(lit: &LitPattern, value: &Value) -> bool {
    match (lit, value) {
        (LitPattern::Int(a), Value::Int(b)) => a == b,
        (LitPattern::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        (LitPattern::Str(a), Value::Str(b)) => a == b,
        (LitPattern::Bool(a), Value::Bool(b)) => a == b,
        (LitPattern::Nil, Value::Nil) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::Span;

    fn pat(kind: PatternKind) -> Pattern {
        Pattern { kind, span: Span::dummy() }
    }

    #[test]
    fn wildcard_matches_anything() {
        let p = pat(PatternKind::Wildcard);
        assert_eq!(try_match(&p, &Value::Int(42)), Some(vec![]));
        assert_eq!(try_match(&p, &Value::Str("hello".into())), Some(vec![]));
        assert_eq!(try_match(&p, &Value::Nil), Some(vec![]));
    }

    #[test]
    fn binding_captures_value() {
        let p = pat(PatternKind::Binding("x".into()));
        let result = try_match(&p, &Value::Int(42));
        assert_eq!(result, Some(vec![("x".into(), Value::Int(42))]));
    }

    #[test]
    fn binding_captures_any_type() {
        let p = pat(PatternKind::Binding("val".into()));
        let v = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let result = try_match(&p, &v).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "val");
    }

    #[test]
    fn literal_int_match() {
        let p = pat(PatternKind::Literal(LitPattern::Int(42)));
        assert!(try_match(&p, &Value::Int(42)).is_some());
        assert!(try_match(&p, &Value::Int(43)).is_none());
    }

    #[test]
    fn literal_str_match() {
        let p = pat(PatternKind::Literal(LitPattern::Str("hello".into())));
        assert!(try_match(&p, &Value::Str("hello".into())).is_some());
        assert!(try_match(&p, &Value::Str("world".into())).is_none());
    }

    #[test]
    fn literal_bool_match() {
        let p = pat(PatternKind::Literal(LitPattern::Bool(true)));
        assert!(try_match(&p, &Value::Bool(true)).is_some());
        assert!(try_match(&p, &Value::Bool(false)).is_none());
    }

    #[test]
    fn literal_nil_match() {
        let p = pat(PatternKind::Literal(LitPattern::Nil));
        assert!(try_match(&p, &Value::Nil).is_some());
        assert!(try_match(&p, &Value::Int(0)).is_none());
    }

    #[test]
    fn literal_type_mismatch() {
        let p = pat(PatternKind::Literal(LitPattern::Int(1)));
        assert!(try_match(&p, &Value::Str("1".into())).is_none());
        assert!(try_match(&p, &Value::Bool(true)).is_none());
    }

    #[test]
    fn variant_match_with_binding() {
        let p = pat(PatternKind::Variant(
            "Ok".into(),
            vec![pat(PatternKind::Binding("x".into()))],
        ));
        let v = Value::Variant("Ok".into(), Box::new(Value::Int(42)));
        let result = try_match(&p, &v).unwrap();
        assert_eq!(result, vec![("x".into(), Value::Int(42))]);
    }

    #[test]
    fn variant_name_mismatch() {
        let p = pat(PatternKind::Variant(
            "Ok".into(),
            vec![pat(PatternKind::Wildcard)],
        ));
        let v = Value::Variant("Err".into(), Box::new(Value::Str("bad".into())));
        assert!(try_match(&p, &v).is_none());
    }

    #[test]
    fn variant_on_non_variant_value() {
        let p = pat(PatternKind::Variant(
            "Ok".into(),
            vec![pat(PatternKind::Wildcard)],
        ));
        assert!(try_match(&p, &Value::Int(42)).is_none());
    }

    #[test]
    fn nullary_variant_match() {
        let p = pat(PatternKind::Variant("None".into(), vec![]));
        let v = Value::Variant("None".into(), Box::new(Value::Unit));
        assert!(try_match(&p, &v).is_some());
    }

    #[test]
    fn nullary_variant_non_unit_inner_fails() {
        let p = pat(PatternKind::Variant("None".into(), vec![]));
        let v = Value::Variant("None".into(), Box::new(Value::Int(1)));
        assert!(try_match(&p, &v).is_none());
    }

    #[test]
    fn nested_variant_pattern() {
        // (Ok (Ok x))
        let inner_pat = pat(PatternKind::Variant(
            "Ok".into(),
            vec![pat(PatternKind::Binding("x".into()))],
        ));
        let outer_pat = pat(PatternKind::Variant("Ok".into(), vec![inner_pat]));

        let inner_val = Value::Variant("Ok".into(), Box::new(Value::Int(7)));
        let outer_val = Value::Variant("Ok".into(), Box::new(inner_val));

        let result = try_match(&outer_pat, &outer_val).unwrap();
        assert_eq!(result, vec![("x".into(), Value::Int(7))]);
    }

    #[test]
    fn variant_with_literal_subpattern() {
        let p = pat(PatternKind::Variant(
            "Ok".into(),
            vec![pat(PatternKind::Literal(LitPattern::Int(42)))],
        ));
        let v_match = Value::Variant("Ok".into(), Box::new(Value::Int(42)));
        let v_no = Value::Variant("Ok".into(), Box::new(Value::Int(99)));
        assert!(try_match(&p, &v_match).is_some());
        assert!(try_match(&p, &v_no).is_none());
    }

    #[test]
    fn multi_field_variant() {
        // (Pair x y) matching (Pair (1 2)) where inner is Tuple
        let p = pat(PatternKind::Variant(
            "Pair".into(),
            vec![
                pat(PatternKind::Binding("a".into())),
                pat(PatternKind::Binding("b".into())),
            ],
        ));
        let v = Value::Variant(
            "Pair".into(),
            Box::new(Value::Tuple(vec![Value::Int(1), Value::Int(2)])),
        );
        let result = try_match(&p, &v).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("a".into(), Value::Int(1)));
        assert_eq!(result[1], ("b".into(), Value::Int(2)));
    }
}
