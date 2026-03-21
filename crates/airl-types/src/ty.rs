pub type Symbol = String;

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Prim(PrimTy),
    Tensor { elem: Box<Ty>, shape: Vec<DimExpr> },
    Func { params: Vec<Ty>, ret: Box<Ty> },
    Named { name: Symbol, args: Vec<TyArg> },
    Sum(Vec<TyVariant>),
    Product(Vec<TyField>),
    TypeVar(Symbol),
    Nat(DimExpr),
    Unit,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimTy {
    Bool,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F16, F32, F64,
    BF16,
    Nat,
    Str,
}

impl PrimTy {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "bool" => Some(Self::Bool),
            "i8" => Some(Self::I8), "i16" => Some(Self::I16),
            "i32" => Some(Self::I32), "i64" => Some(Self::I64),
            "u8" => Some(Self::U8), "u16" => Some(Self::U16),
            "u32" => Some(Self::U32), "u64" => Some(Self::U64),
            "f16" => Some(Self::F16), "f32" => Some(Self::F32), "f64" => Some(Self::F64),
            "bf16" => Some(Self::BF16),
            "Nat" => Some(Self::Nat),
            "String" => Some(Self::Str),
            _ => None,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64
            | Self::U8 | Self::U16 | Self::U32 | Self::U64)
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::F16 | Self::F32 | Self::F64 | Self::BF16)
    }

    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DimExpr {
    Lit(u64),
    Var(Symbol),
    BinOp(DimOp, Box<DimExpr>, Box<DimExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimOp { Add, Sub, Mul }

#[derive(Debug, Clone, PartialEq)]
pub enum TyArg {
    Type(Ty),
    Nat(DimExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TyVariant {
    pub name: Symbol,
    pub fields: Vec<Ty>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TyField {
    pub name: Symbol,
    pub ty: Ty,
}

/// Whether a type supports Copy semantics.
pub fn is_copy(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(p) => *p != PrimTy::Str, // all primitives except String
        Ty::Unit => true,
        Ty::Nat(_) => true,
        _ => false, // tensors, functions, named types are not copy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_from_name() {
        assert_eq!(PrimTy::from_name("i32"), Some(PrimTy::I32));
        assert_eq!(PrimTy::from_name("bf16"), Some(PrimTy::BF16));
        assert_eq!(PrimTy::from_name("garbage"), None);
    }

    #[test]
    fn numeric_classification() {
        assert!(PrimTy::I32.is_integer());
        assert!(!PrimTy::I32.is_float());
        assert!(PrimTy::F64.is_float());
        assert!(PrimTy::F64.is_numeric());
        assert!(!PrimTy::Bool.is_numeric());
    }

    #[test]
    fn copy_semantics() {
        assert!(is_copy(&Ty::Prim(PrimTy::I32)));
        assert!(is_copy(&Ty::Unit));
        assert!(!is_copy(&Ty::Prim(PrimTy::Str)));
        assert!(!is_copy(&Ty::Tensor { elem: Box::new(Ty::Prim(PrimTy::F32)), shape: vec![] }));
    }
}
