use crate::value::Value;

/// Simplified IR node — no source positions, no contracts, no type annotations.
#[derive(Debug, Clone)]
pub enum IRNode {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,

    // Variables
    Load(String),

    // Control flow
    If(Box<IRNode>, Box<IRNode>, Box<IRNode>),
    Do(Vec<IRNode>),
    Let(Vec<IRBinding>, Box<IRNode>),

    // Functions
    Func(String, Vec<String>, Box<IRNode>),       // name, params, body
    Lambda(Vec<String>, Box<IRNode>),              // params, body
    Call(String, Vec<IRNode>),                     // named call
    CallExpr(Box<IRNode>, Vec<IRNode>),            // computed callee

    // Data
    List(Vec<IRNode>),
    Variant(String, Vec<IRNode>),                  // tag, args

    // Pattern matching
    Match(Box<IRNode>, Vec<IRArm>),

    // Error handling
    Try(Box<IRNode>),
}

#[derive(Debug, Clone)]
pub struct IRBinding {
    pub name: String,
    pub expr: IRNode,
}

#[derive(Debug, Clone)]
pub struct IRArm {
    pub pattern: IRPattern,
    pub body: IRNode,
}

#[derive(Debug, Clone)]
pub enum IRPattern {
    Wild,
    Bind(String),
    Lit(Value),
    Variant(String, Vec<IRPattern>),
}

/// A compiled function stored in the VM's function table.
#[derive(Debug, Clone)]
pub struct IRFunc {
    pub name: String,
    pub params: Vec<String>,
    pub body: IRNode,
}

/// A closure value: lambda + captured environment.
#[derive(Debug, Clone)]
pub struct IRClosureValue {
    pub params: Vec<String>,
    pub body: Box<IRNode>,
    pub captured_env: Vec<(String, Value)>,
}
