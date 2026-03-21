use airl_syntax::ast::{AstType, AstTypeKind, FnDef};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type as CraneliftType;

/// Map an AIRL type name to a Cranelift IR type.
/// Returns None if the type is not a supported primitive.
pub fn airl_type_to_cranelift(name: &str) -> Option<CraneliftType> {
    match name {
        "i32" => Some(types::I32),
        "i64" => Some(types::I64),
        "f32" => Some(types::F32),
        "f64" => Some(types::F64),
        "bool" => Some(types::I8),
        _ => None,
    }
}

/// Map an AST type to a Cranelift type.
pub fn resolve_ast_type(ty: &AstType) -> Option<CraneliftType> {
    match &ty.kind {
        AstTypeKind::Named(name) => airl_type_to_cranelift(name),
        _ => None,
    }
}

/// Check if a function's signature is all-primitive (eligible for JIT).
pub fn is_jit_eligible(def: &FnDef) -> bool {
    // All params must have primitive types
    for param in &def.params {
        if resolve_ast_type(&param.ty).is_none() {
            return false;
        }
    }
    // Return type must be primitive
    resolve_ast_type(&def.return_type).is_some()
}

/// Returns true if a Cranelift type is floating point.
pub fn is_float_type(ty: CraneliftType) -> bool {
    ty == types::F32 || ty == types::F64
}

/// Returns true if a Cranelift type is integer (includes bool as I8).
pub fn is_int_type(ty: CraneliftType) -> bool {
    ty == types::I8 || ty == types::I32 || ty == types::I64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_primitive_types() {
        assert_eq!(airl_type_to_cranelift("i32"), Some(types::I32));
        assert_eq!(airl_type_to_cranelift("i64"), Some(types::I64));
        assert_eq!(airl_type_to_cranelift("f32"), Some(types::F32));
        assert_eq!(airl_type_to_cranelift("f64"), Some(types::F64));
        assert_eq!(airl_type_to_cranelift("bool"), Some(types::I8));
    }

    #[test]
    fn non_primitive_returns_none() {
        assert_eq!(airl_type_to_cranelift("String"), None);
        assert_eq!(airl_type_to_cranelift("List"), None);
        assert_eq!(airl_type_to_cranelift("tensor"), None);
    }

    #[test]
    fn float_int_classification() {
        assert!(is_float_type(types::F64));
        assert!(!is_float_type(types::I64));
        assert!(is_int_type(types::I32));
        assert!(!is_int_type(types::F32));
    }
}
