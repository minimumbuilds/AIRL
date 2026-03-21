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
}
