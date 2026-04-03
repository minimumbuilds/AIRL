// TODO: type Symbol = SymbolId (u32 index into intern table) for O(1) comparisons
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

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Prim(p) => write!(f, "{}", p),
            Ty::Tensor { elem, shape } => {
                let dims: Vec<String> = shape.iter().map(|d| format!("{:?}", d)).collect();
                write!(f, "tensor[{} {}]", elem, dims.join(" "))
            }
            Ty::Func { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| format!("{}", p)).collect();
                write!(f, "({} -> {})", ps.join(" "), ret)
            }
            Ty::Named { name, args } => {
                if args.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let as_: Vec<String> = args.iter().map(|a| match a {
                        TyArg::Type(t) => format!("{}", t),
                        TyArg::Nat(d) => format!("{:?}", d),
                    }).collect();
                    write!(f, "{}[{}]", name, as_.join(", "))
                }
            }
            Ty::Sum(variants) => {
                let vs: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                write!(f, "({})", vs.join(" | "))
            }
            Ty::Product(fields) => {
                let fs: Vec<String> = fields.iter()
                    .map(|fld| format!("{}: {}", fld.name, fld.ty))
                    .collect();
                write!(f, "{{{}}}", fs.join(", "))
            }
            Ty::TypeVar(s) => write!(f, "{}", s),
            Ty::Nat(d) => write!(f, "{:?}", d),
            Ty::Unit => write!(f, "()"),
            Ty::Never => write!(f, "!"),
        }
    }
}

impl std::fmt::Display for PrimTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PrimTy::Bool => "bool",
            PrimTy::I8 => "i8", PrimTy::I16 => "i16",
            PrimTy::I32 => "i32", PrimTy::I64 => "i64",
            PrimTy::U8 => "u8", PrimTy::U16 => "u16",
            PrimTy::U32 => "u32", PrimTy::U64 => "u64",
            PrimTy::F16 => "f16", PrimTy::F32 => "f32", PrimTy::F64 => "f64",
            PrimTy::BF16 => "bf16",
            PrimTy::Nat => "Nat",
            PrimTy::Str => "String",
        };
        write!(f, "{}", name)
    }
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
