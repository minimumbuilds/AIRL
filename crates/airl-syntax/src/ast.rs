use crate::span::Span;

pub type Symbol = String;

// ── Top Level ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevel {
    Module(ModuleDef),
    Defn(FnDef),
    DefType(TypeDef),
    Task(TaskDef),
    UseDecl(UseDef),
    Import {
        path: String,
        alias: Option<String>,
        only: Option<Vec<String>>,
        span: Span,
    },
    Expr(Expr), // bare expression at top level (REPL)
}

// ── Module ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: Symbol,
    pub version: Option<Version>,
    pub requires: Vec<Symbol>,
    pub provides: Vec<Symbol>,
    pub verify: VerifyLevel,
    pub execute_on: Option<ExecTarget>,
    pub body: Vec<TopLevel>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyLevel {
    Checked,
    Proven,
    Trusted,
}

impl Default for VerifyLevel {
    fn default() -> Self { Self::Checked }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecTarget {
    Cpu,
    Gpu,
    Any,
    Agent(Symbol),
}

// ── Function ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub name: Symbol,
    pub params: Vec<Param>,
    pub return_type: AstType,
    pub intent: Option<String>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub invariants: Vec<Expr>,
    pub body: Expr,
    pub execute_on: Option<ExecTarget>,
    pub priority: Option<Priority>,
    pub is_public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ownership: Ownership,
    pub name: Symbol,
    pub ty: AstType,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Own,
    Ref,
    Mut,
    Copy,
    Default, // no explicit annotation = Own
}

// ── Type Definitions ────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: Symbol,
    pub type_params: Vec<TypeParam>,
    pub body: TypeDefBody,
    pub is_public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Symbol,
    pub bound: AstType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeDefBody {
    Sum(Vec<Variant>),
    Product(Vec<Field>),
    Alias(AstType),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: Symbol,
    pub fields: Vec<AstType>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: Symbol,
    pub ty: AstType,
    pub span: Span,
}

// ── Types (AST-level) ───────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct AstType {
    pub kind: AstTypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstTypeKind {
    Named(Symbol),                              // i32, bool, String
    App(Symbol, Vec<AstType>),                  // Result[i32, DivError], tensor[f32, 64, 64]
    Func(Vec<AstType>, Box<AstType>),           // (-> [i32 i32] i32)
    Nat(NatExpr),                               // type-level number
}

#[derive(Debug, Clone, PartialEq)]
pub enum NatExpr {
    Lit(u64),
    Var(Symbol),
    BinOp(NatOp, Box<NatExpr>, Box<NatExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatOp {
    Add, Sub, Mul,
}

// ── Expressions ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // Atoms
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    BoolLit(bool),
    NilLit,
    SymbolRef(Symbol),
    KeywordLit(String),

    // Compound
    If(Box<Expr>, Box<Expr>, Box<Expr>),       // (if cond then else)
    Let(Vec<LetBinding>, Box<Expr>),            // (let (x : T v) ... body)
    Do(Vec<Expr>),                              // (do e1 e2 ... en)
    Match(Box<Expr>, Vec<MatchArm>),            // (match expr arms...)
    Lambda(Vec<Param>, Box<Expr>),              // (fn [params] body)
    FnCall(Box<Expr>, Vec<Expr>),               // (f a b c)
    Try(Box<Expr>),                             // (try expr)
    Forall(Box<Param>, Option<Box<Expr>>, Box<Expr>), // (forall [i : Nat] (where guard) body)
    Exists(Box<Param>, Option<Box<Expr>>, Box<Expr>), // (exists [i : Nat] (where guard) body)

    // Constructor
    VariantCtor(Symbol, Vec<Expr>),             // (Ok val), (Err reason)
    StructLit(Symbol, Vec<(Symbol, Expr)>),     // (AgentMessage :id "x" ...)
    ListLit(Vec<Expr>),                         // [1 2 3]
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: Symbol,
    pub ty: AstType,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    Wildcard,                                   // _
    Binding(Symbol),                            // x
    Literal(LitPattern),                        // 42, "hello"
    Variant(Symbol, Vec<Pattern>),              // (Ok x), (Err _)
}

#[derive(Debug, Clone, PartialEq)]
pub enum LitPattern {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
}

// ── Task ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TaskDef {
    pub id: String,
    pub from: Expr,
    pub to: Expr,
    pub deadline: Option<Expr>,
    pub intent: Option<String>,
    pub input: Vec<Param>,
    pub expected_output: Option<ExpectedOutput>,
    pub constraints: Vec<Constraint>,
    pub on_success: Option<Expr>,
    pub on_failure: Option<Expr>,
    pub on_timeout: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedOutput {
    pub params: Vec<Param>,
    pub ensures: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub kind: ConstraintKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintKind {
    MaxMemory(Expr),
    MaxTokens(Expr),
    NoNetwork(bool),
    Custom(Symbol, Expr),
}

// ── Use Declaration ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct UseDef {
    pub module: Symbol,
    pub kind: UseKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseKind {
    Symbols(Vec<Symbol>),
    Prefixed(Symbol),
    All,
}

// ── Pretty-printing Expr back to AIRL S-expression syntax ───

impl Expr {
    /// Convert an expression AST node back to readable AIRL source text.
    pub fn to_airl(&self) -> String {
        match &self.kind {
            ExprKind::IntLit(v) => v.to_string(),
            ExprKind::FloatLit(v) => format!("{}", v),
            ExprKind::StrLit(v) => format!("\"{}\"", v),
            ExprKind::BoolLit(true) => "true".into(),
            ExprKind::BoolLit(false) => "false".into(),
            ExprKind::NilLit => "nil".into(),
            ExprKind::SymbolRef(s) => s.clone(),
            ExprKind::KeywordLit(k) => format!(":{}", k),

            ExprKind::FnCall(callee, args) => {
                let mut parts = vec![callee.to_airl()];
                for a in args {
                    parts.push(a.to_airl());
                }
                format!("({})", parts.join(" "))
            }

            ExprKind::If(cond, then_b, else_b) => {
                format!("(if {} {} {})", cond.to_airl(), then_b.to_airl(), else_b.to_airl())
            }

            ExprKind::Let(bindings, body) => {
                let bs: Vec<String> = bindings.iter()
                    .map(|b| format!("({} : {} {})", b.name, b.ty.to_airl(), b.value.to_airl()))
                    .collect();
                format!("(let {} {})", bs.join(" "), body.to_airl())
            }

            ExprKind::Do(exprs) => {
                let parts: Vec<String> = exprs.iter().map(|e| e.to_airl()).collect();
                format!("(do {})", parts.join(" "))
            }

            ExprKind::Match(scrutinee, arms) => {
                let mut s = format!("(match {}", scrutinee.to_airl());
                for arm in arms {
                    s.push_str(&format!(" {} {}", arm.pattern.to_airl(), arm.body.to_airl()));
                }
                s.push(')');
                s
            }

            ExprKind::Lambda(params, body) => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                format!("(fn [{}] {})", ps.join(" "), body.to_airl())
            }

            ExprKind::Try(inner) => format!("(try {})", inner.to_airl()),

            ExprKind::Forall(param, where_c, body) => {
                let mut s = format!("(forall [{} : {}]", param.name, param.ty.to_airl());
                if let Some(guard) = where_c {
                    s.push_str(&format!(" (where {})", guard.to_airl()));
                }
                s.push_str(&format!(" {})", body.to_airl()));
                s
            }

            ExprKind::Exists(param, where_c, body) => {
                let mut s = format!("(exists [{} : {}]", param.name, param.ty.to_airl());
                if let Some(guard) = where_c {
                    s.push_str(&format!(" (where {})", guard.to_airl()));
                }
                s.push_str(&format!(" {})", body.to_airl()));
                s
            }

            ExprKind::VariantCtor(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let parts: Vec<String> = args.iter().map(|a| a.to_airl()).collect();
                    format!("({} {})", name, parts.join(" "))
                }
            }

            ExprKind::StructLit(name, fields) => {
                let fs: Vec<String> = fields.iter()
                    .map(|(k, v)| format!(":{} {}", k, v.to_airl()))
                    .collect();
                format!("({} {})", name, fs.join(" "))
            }

            ExprKind::ListLit(items) => {
                let parts: Vec<String> = items.iter().map(|e| e.to_airl()).collect();
                format!("[{}]", parts.join(" "))
            }
        }
    }
}

impl AstType {
    /// Convert a type back to AIRL syntax.
    pub fn to_airl(&self) -> String {
        match &self.kind {
            AstTypeKind::Named(s) => s.clone(),
            AstTypeKind::App(name, args) => {
                let parts: Vec<String> = args.iter().map(|a| a.to_airl()).collect();
                format!("{}[{}]", name, parts.join(", "))
            }
            AstTypeKind::Func(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| p.to_airl()).collect();
                format!("(-> [{}] {})", ps.join(" "), ret.to_airl())
            }
            AstTypeKind::Nat(n) => match n {
                NatExpr::Lit(v) => v.to_string(),
                NatExpr::Var(s) => s.clone(),
                NatExpr::BinOp(op, l, r) => {
                    let op_str = match op {
                        NatOp::Add => "+",
                        NatOp::Sub => "-",
                        NatOp::Mul => "*",
                    };
                    format!("({} {:?} {:?})", op_str, l, r)
                }
            }
        }
    }
}

impl Pattern {
    /// Convert a pattern back to AIRL syntax.
    pub fn to_airl(&self) -> String {
        match &self.kind {
            PatternKind::Wildcard => "_".into(),
            PatternKind::Binding(name) => name.clone(),
            PatternKind::Literal(lit) => match lit {
                LitPattern::Int(v) => v.to_string(),
                LitPattern::Float(v) => format!("{}", v),
                LitPattern::Str(v) => format!("\"{}\"", v),
                LitPattern::Bool(true) => "true".into(),
                LitPattern::Bool(false) => "false".into(),
                LitPattern::Nil => "nil".into(),
            },
            PatternKind::Variant(name, pats) => {
                if pats.is_empty() {
                    format!("({})", name)
                } else {
                    let ps: Vec<String> = pats.iter().map(|p| p.to_airl()).collect();
                    format!("({} {})", name, ps.join(" "))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_level_default_is_checked() {
        assert_eq!(VerifyLevel::default(), VerifyLevel::Checked);
    }

    #[test]
    fn ast_types_are_clone_and_debug() {
        // Compile-time test: all types derive Clone and Debug
        let e = Expr {
            kind: ExprKind::IntLit(42),
            span: Span::dummy(),
        };
        let _ = e.clone();
        let _ = format!("{:?}", e);
    }

    #[test]
    fn import_ast_node_constructable() {
        let import = TopLevel::Import {
            path: "lib/math.airl".to_string(),
            alias: Some("m".to_string()),
            only: None,
            span: Span::dummy(),
        };
        let _ = import.clone();
        let _ = format!("{:?}", import);
    }

    #[test]
    fn fn_def_has_is_public() {
        let f = FnDef {
            name: "test".to_string(),
            params: vec![],
            return_type: AstType { kind: AstTypeKind::Named("Unit".to_string()), span: Span::dummy() },
            intent: None,
            requires: vec![],
            ensures: vec![],
            invariants: vec![],
            body: Expr { kind: ExprKind::NilLit, span: Span::dummy() },
            execute_on: None,
            priority: None,
            is_public: true,
            span: Span::dummy(),
        };
        assert!(f.is_public);
    }
}
