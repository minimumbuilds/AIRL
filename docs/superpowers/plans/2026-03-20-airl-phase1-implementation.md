# AIRL Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete tree-walking interpreter for AIRL in Rust — parser, type checker, contract verifier, evaluator, and inter-agent communication runtime.

**Architecture:** Cargo workspace with 6 crates following the compilation pipeline: `airl-syntax → airl-types → airl-contracts → airl-runtime → airl-agent → airl-driver`. Each crate has a single responsibility and clear public API. Zero external dependencies.

**Tech Stack:** Rust (stable), std only, no external crates.

**Spec:** `docs/superpowers/specs/2026-03-20-airl-phase1-design.md`
**Language Spec:** `AIRL-Language-Specification-v0.1.0.md`

---

## File Map

```
airl/
├── Cargo.toml                          # workspace root
├── crates/
│   ├── airl-syntax/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # re-exports
│   │       ├── span.rs                 # Span, source location tracking
│   │       ├── token.rs                # Token enum, TokenKind
│   │       ├── lexer.rs                # hand-written scanner
│   │       ├── sexpr.rs                # SExpr type, S-expression parser
│   │       ├── ast.rs                  # typed AST nodes (TopLevel, Expr, Type, etc.)
│   │       ├── parser.rs              # form parser: SExpr → AST
│   │       └── diagnostic.rs           # Diagnostic, Severity, error collection
│   │
│   ├── airl-types/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ty.rs                   # Ty enum, PrimTy, DimExpr, TyArg
│   │       ├── env.rs                  # TypeEnv, scoped symbol table
│   │       ├── checker.rs              # type checking pass
│   │       ├── unify.rs                # DimExpr unification for dependent types
│   │       ├── linearity.rs            # OwnershipState, borrow checker
│   │       └── exhaustiveness.rs       # match arm exhaustiveness checking
│   │
│   ├── airl-contracts/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── checked.rs              # runtime contract assertion engine
│   │       ├── prover.rs               # stub symbolic prover
│   │       ├── trusted.rs              # trusted mode (no-op with logging)
│   │       └── violation.rs            # ContractViolation type, formatting
│   │
│   ├── airl-runtime/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── value.rs                # Value enum, Display impls
│   │       ├── tensor.rs               # TensorValue, TensorData, tensor ops
│   │       ├── env.rs                  # runtime Env, Frame, Slot
│   │       ├── eval.rs                 # tree-walking evaluator
│   │       ├── pattern.rs              # Pattern enum, pattern matching
│   │       ├── builtins.rs             # builtin function registry and impls
│   │       └── error.rs                # RuntimeError type
│   │
│   ├── airl-agent/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── identity.rs             # AgentId, Capability, TrustLevel
│   │       ├── transport.rs            # Transport trait, framing protocol
│   │       ├── stdio_transport.rs      # StdioTransport implementation
│   │       ├── tcp_transport.rs        # TcpTransport implementation
│   │       ├── unix_transport.rs       # UnixTransport implementation
│   │       ├── registry.rs             # AgentRegistry, capability-based lookup
│   │       ├── task.rs                 # TaskDef runtime, lifecycle, deadline
│   │       └── runtime.rs              # AgentRuntime, main receive loop
│   │
│   └── airl-driver/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # CLI arg parsing, mode dispatch
│           ├── pipeline.rs             # orchestrate lex → parse → check → eval
│           ├── repl.rs                 # REPL loop, paren balancing
│           └── fmt.rs                  # S-expression pretty printer
│
├── tests/
│   ├── fixtures/
│   │   ├── valid/
│   │   │   ├── literals.airl
│   │   │   ├── arithmetic.airl
│   │   │   ├── control_flow.airl
│   │   │   ├── types_tensor.airl
│   │   │   ├── types_algebraic.airl
│   │   │   ├── ownership.airl
│   │   │   ├── dependent_dims.airl
│   │   │   ├── contracts.airl
│   │   │   ├── safe_divide.airl
│   │   │   ├── matrix_multiply.airl
│   │   │   ├── modules.airl
│   │   │   ├── tensor_ops.airl
│   │   │   ├── quantifier_contracts.airl
│   │   │   ├── try_propagation.airl
│   │   │   ├── higher_order.airl
│   │   │   └── module_imports.airl
│   │   ├── type_errors/
│   │   │   ├── dim_mismatch.airl
│   │   │   ├── wrong_arg_type.airl
│   │   │   └── missing_contracts.airl
│   │   ├── contract_errors/
│   │   │   ├── precondition.airl
│   │   │   └── postcondition.airl
│   │   ├── linearity_errors/
│   │   │   ├── use_after_move.airl
│   │   │   ├── double_mut_borrow.airl
│   │   │   └── move_while_borrowed.airl
│   │   └── agent/
│   │       ├── task_roundtrip.airl
│   │       ├── capability_routing.airl
│   │       ├── parallel_fanout.airl
│   │       ├── broadcast.airl
│   │       └── await_timeout.airl
│   └── e2e/
│       └── fixture_runner.rs           # test harness for .airl fixtures
└── docs/
```

---

## Task 1: Workspace Scaffolding

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/airl-syntax/Cargo.toml`
- Create: `crates/airl-syntax/src/lib.rs`
- Create: `crates/airl-types/Cargo.toml`
- Create: `crates/airl-types/src/lib.rs`
- Create: `crates/airl-contracts/Cargo.toml`
- Create: `crates/airl-contracts/src/lib.rs`
- Create: `crates/airl-runtime/Cargo.toml`
- Create: `crates/airl-runtime/src/lib.rs`
- Create: `crates/airl-agent/Cargo.toml`
- Create: `crates/airl-agent/src/lib.rs`
- Create: `crates/airl-driver/Cargo.toml`
- Create: `crates/airl-driver/src/main.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/airl-syntax",
    "crates/airl-types",
    "crates/airl-contracts",
    "crates/airl-runtime",
    "crates/airl-agent",
    "crates/airl-driver",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
```

- [ ] **Step 2: Create each crate's Cargo.toml and stub lib.rs/main.rs**

`crates/airl-syntax/Cargo.toml`:
```toml
[package]
name = "airl-syntax"
version.workspace = true
edition.workspace = true

[dependencies]
```

`crates/airl-syntax/src/lib.rs`:
```rust
pub mod span;
pub mod token;
pub mod lexer;
pub mod sexpr;
pub mod ast;
pub mod parser;
pub mod diagnostic;
```

`crates/airl-types/Cargo.toml`:
```toml
[package]
name = "airl-types"
version.workspace = true
edition.workspace = true

[dependencies]
airl-syntax = { path = "../airl-syntax" }
```

`crates/airl-types/src/lib.rs`:
```rust
pub mod ty;
pub mod env;
pub mod checker;
pub mod unify;
pub mod linearity;
pub mod exhaustiveness;
```

`crates/airl-contracts/Cargo.toml`:
```toml
[package]
name = "airl-contracts"
version.workspace = true
edition.workspace = true

[dependencies]
airl-syntax = { path = "../airl-syntax" }
airl-types = { path = "../airl-types" }
```

`crates/airl-contracts/src/lib.rs`:
```rust
pub mod checked;
pub mod prover;
pub mod trusted;
pub mod violation;
```

`crates/airl-runtime/Cargo.toml`:
```toml
[package]
name = "airl-runtime"
version.workspace = true
edition.workspace = true

[dependencies]
airl-syntax = { path = "../airl-syntax" }
airl-types = { path = "../airl-types" }
airl-contracts = { path = "../airl-contracts" }
```

`crates/airl-runtime/src/lib.rs`:
```rust
pub mod value;
pub mod tensor;
pub mod env;
pub mod eval;
pub mod pattern;
pub mod builtins;
pub mod error;
```

`crates/airl-agent/Cargo.toml`:
```toml
[package]
name = "airl-agent"
version.workspace = true
edition.workspace = true

[dependencies]
airl-syntax = { path = "../airl-syntax" }
airl-types = { path = "../airl-types" }
airl-contracts = { path = "../airl-contracts" }
airl-runtime = { path = "../airl-runtime" }
```

`crates/airl-agent/src/lib.rs`:
```rust
pub mod identity;
pub mod transport;
pub mod stdio_transport;
pub mod tcp_transport;
pub mod unix_transport;
pub mod registry;
pub mod task;
pub mod runtime;
```

`crates/airl-driver/Cargo.toml`:
```toml
[package]
name = "airl-driver"
version.workspace = true
edition.workspace = true

[[bin]]
name = "airl"
path = "src/main.rs"

[dependencies]
airl-syntax = { path = "../airl-syntax" }
airl-types = { path = "../airl-types" }
airl-contracts = { path = "../airl-contracts" }
airl-runtime = { path = "../airl-runtime" }
airl-agent = { path = "../airl-agent" }
```

`crates/airl-driver/src/main.rs`:
```rust
fn main() {
    println!("airl interpreter v0.1.0");
}
```

- [ ] **Step 3: Verify workspace compiles**

Run: `cargo build`
Expected: successful compilation of all 6 crates

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/
git commit -m "scaffold: initialize Cargo workspace with 6 crates"
```

---

## Task 2: Span and Diagnostic Types (`airl-syntax`)

**Files:**
- Create: `crates/airl-syntax/src/span.rs`
- Create: `crates/airl-syntax/src/diagnostic.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Write failing tests for Span**

In `crates/airl-syntax/src/span.rs`:
```rust
/// Byte-offset range in source text with line/column information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(start: usize, end: usize, line: u32, col: u32) -> Self {
        Self { start, end, line, col }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0, line: 0, col: 0 }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line.min(other.line),
            col: if self.line <= other.line { self.col } else { other.col },
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge_takes_outer_bounds() {
        let a = Span::new(5, 10, 1, 5);
        let b = Span::new(15, 20, 2, 3);
        let merged = a.merge(b);
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 20);
    }

    #[test]
    fn span_len() {
        let s = Span::new(3, 10, 1, 3);
        assert_eq!(s.len(), 7);
    }
}
```

- [ ] **Step 2: Write Diagnostic types with tests**

In `crates/airl-syntax/src/diagnostic.rs`:
```rust
use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub notes: Vec<(Span, String)>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, span: Span, message: impl Into<String>) -> Self {
        self.notes.push((span, message.into()));
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Collects diagnostics from all compilation phases.
#[derive(Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, diag: Diagnostic) {
        self.items.push(diag);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.is_error())
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.is_error())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_tracks_errors() {
        let mut diags = Diagnostics::new();
        diags.add(Diagnostic::error("bad", Span::dummy()));
        diags.add(Diagnostic::warning("eh", Span::dummy()));
        assert!(diags.has_errors());
        assert_eq!(diags.errors().count(), 1);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn diagnostic_with_notes() {
        let d = Diagnostic::error("use after move", Span::new(10, 15, 2, 5))
            .with_note(Span::new(5, 8, 1, 5), "moved here");
        assert_eq!(d.notes.len(), 1);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-syntax`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-syntax/src/span.rs crates/airl-syntax/src/diagnostic.rs
git commit -m "feat(syntax): add Span and Diagnostic types"
```

---

## Task 3: Token Types and Lexer (`airl-syntax`)

**Files:**
- Create: `crates/airl-syntax/src/token.rs`
- Create: `crates/airl-syntax/src/lexer.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Define Token types**

In `crates/airl-syntax/src/token.rs`:
```rust
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Integer(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,

    // Identifiers
    Symbol(String),
    Keyword(String),     // includes the colon prefix stripped: ":sig" → "sig"

    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,

    // Special
    Colon,               // standalone colon in type annotations
    Arrow,               // -> in signatures
    Comma,               // , in type parameter lists

    Eof,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn token_equality() {
        let a = Token::new(TokenKind::Integer(42), Span::new(0, 2, 1, 0));
        let b = Token::new(TokenKind::Integer(42), Span::new(0, 2, 1, 0));
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Write failing lexer tests**

In `crates/airl-syntax/src/lexer.rs`, write the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        tokens.into_iter().map(|t| t.kind).filter(|k| *k != TokenKind::Eof).collect()
    }

    #[test]
    fn lex_integer_decimal() {
        assert_eq!(lex("42"), vec![TokenKind::Integer(42)]);
    }

    #[test]
    fn lex_integer_hex() {
        assert_eq!(lex("0xFF"), vec![TokenKind::Integer(255)]);
    }

    #[test]
    fn lex_integer_binary() {
        assert_eq!(lex("0b1010"), vec![TokenKind::Integer(10)]);
    }

    #[test]
    fn lex_float() {
        assert_eq!(lex("3.14"), vec![TokenKind::Float(3.14)]);
    }

    #[test]
    fn lex_float_exponent() {
        assert_eq!(lex("1e-7"), vec![TokenKind::Float(1e-7)]);
    }

    #[test]
    fn lex_string() {
        assert_eq!(lex(r#""hello""#), vec![TokenKind::Str("hello".into())]);
    }

    #[test]
    fn lex_string_escapes() {
        assert_eq!(lex(r#""line\n""#), vec![TokenKind::Str("line\n".into())]);
    }

    #[test]
    fn lex_symbol() {
        assert_eq!(lex("matrix-multiply"), vec![TokenKind::Symbol("matrix-multiply".into())]);
    }

    #[test]
    fn lex_symbol_with_dots() {
        assert_eq!(lex("tensor.contract"), vec![TokenKind::Symbol("tensor.contract".into())]);
    }

    #[test]
    fn lex_keyword() {
        assert_eq!(lex(":sig"), vec![TokenKind::Keyword("sig".into())]);
    }

    #[test]
    fn lex_booleans() {
        assert_eq!(lex("true false"), vec![TokenKind::Bool(true), TokenKind::Bool(false)]);
    }

    #[test]
    fn lex_nil() {
        assert_eq!(lex("nil"), vec![TokenKind::Nil]);
    }

    #[test]
    fn lex_parens_and_brackets() {
        assert_eq!(lex("([])"), vec![
            TokenKind::LParen, TokenKind::LBracket,
            TokenKind::RBracket, TokenKind::RParen,
        ]);
    }

    #[test]
    fn lex_arrow() {
        assert_eq!(lex("->"), vec![TokenKind::Arrow]);
    }

    #[test]
    fn lex_line_comment_skipped() {
        assert_eq!(lex("; this is a comment\n42"), vec![TokenKind::Integer(42)]);
    }

    #[test]
    fn lex_block_comment_skipped() {
        assert_eq!(lex("#| block |# 42"), vec![TokenKind::Integer(42)]);
    }

    #[test]
    fn lex_nested_block_comment() {
        assert_eq!(lex("#| outer #| inner |# still comment |# 42"), vec![TokenKind::Integer(42)]);
    }

    #[test]
    fn lex_full_expression() {
        let tokens = lex("(+ 1 2)");
        assert_eq!(tokens, vec![
            TokenKind::LParen,
            TokenKind::Symbol("+".into()),
            TokenKind::Integer(1),
            TokenKind::Integer(2),
            TokenKind::RParen,
        ]);
    }

    #[test]
    fn lex_defn_snippet() {
        let tokens = lex(r#"(defn safe-divide :sig [(a : i32) -> i32] :body (/ a 1))"#);
        assert!(tokens.len() > 5);
        assert_eq!(tokens[0], TokenKind::LParen);
        assert_eq!(tokens[1], TokenKind::Symbol("defn".into()));
        assert_eq!(tokens[2], TokenKind::Symbol("safe-divide".into()));
    }

    #[test]
    fn lex_agent_ref() {
        // agent:orchestrator should lex as a single symbol
        assert_eq!(lex("agent:orchestrator"), vec![TokenKind::Symbol("agent:orchestrator".into())]);
    }

    #[test]
    fn lex_error_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        assert!(lexer.lex_all().is_err());
    }
}
```

- [ ] **Step 3: Implement the Lexer**

In `crates/airl-syntax/src/lexer.rs`, above the tests:
```rust
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::diagnostic::Diagnostic;

pub struct Lexer<'src> {
    source: &'src [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 0,
        }
    }

    pub fn lex_all(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            if tok.kind == TokenKind::Eof {
                tokens.push(tok);
                break;
            }
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        self.skip_whitespace_and_comments()?;

        if self.pos >= self.source.len() {
            return Ok(Token::new(TokenKind::Eof, self.current_span(0)));
        }

        let start = self.pos;
        let start_line = self.line;
        let start_col = self.col;
        let ch = self.source[self.pos];

        let kind = match ch {
            b'(' => { self.advance(); TokenKind::LParen }
            b')' => { self.advance(); TokenKind::RParen }
            b'[' => { self.advance(); TokenKind::LBracket }
            b']' => { self.advance(); TokenKind::RBracket }
            b',' => { self.advance(); TokenKind::Comma }
            b'"' => self.lex_string()?,
            b':' => self.lex_keyword_or_colon(),
            b'-' if self.peek_at(1) == Some(b'>') => {
                self.advance();
                self.advance();
                TokenKind::Arrow
            }
            b'0'..=b'9' => self.lex_number()?,
            b'-' if self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) => self.lex_number()?,
            _ if is_symbol_start(ch) => self.lex_symbol(),
            _ => {
                return Err(Diagnostic::error(
                    format!("unexpected character: '{}'", ch as char),
                    Span::new(start, start + 1, start_line, start_col),
                ));
            }
        };

        Ok(Token::new(kind, Span::new(start, self.pos, start_line, start_col)))
    }

    fn lex_string(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.pos;
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // skip opening "
        let mut value = String::new();
        loop {
            if self.pos >= self.source.len() {
                return Err(Diagnostic::error(
                    "unterminated string literal",
                    Span::new(start, self.pos, start_line, start_col),
                ));
            }
            match self.source[self.pos] {
                b'"' => {
                    self.advance();
                    return Ok(TokenKind::Str(value));
                }
                b'\\' => {
                    self.advance();
                    if self.pos >= self.source.len() {
                        return Err(Diagnostic::error(
                            "unterminated escape sequence",
                            Span::new(start, self.pos, start_line, start_col),
                        ));
                    }
                    match self.source[self.pos] {
                        b'n' => { value.push('\n'); self.advance(); }
                        b't' => { value.push('\t'); self.advance(); }
                        b'r' => { value.push('\r'); self.advance(); }
                        b'\\' => { value.push('\\'); self.advance(); }
                        b'"' => { value.push('"'); self.advance(); }
                        b'0' => { value.push('\0'); self.advance(); }
                        other => {
                            return Err(Diagnostic::error(
                                format!("unknown escape sequence: \\{}", other as char),
                                Span::new(self.pos - 1, self.pos + 1, self.line, self.col),
                            ));
                        }
                    }
                }
                other => {
                    value.push(other as char);
                    self.advance();
                }
            }
        }
    }

    fn lex_number(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.pos;
        let start_line = self.line;
        let start_col = self.col;
        let negative = if self.source[self.pos] == b'-' {
            self.advance();
            true
        } else {
            false
        };

        // Check for hex/binary prefix
        if self.source[self.pos] == b'0' && self.pos + 1 < self.source.len() {
            match self.source[self.pos + 1] {
                b'x' | b'X' => {
                    self.advance(); self.advance(); // skip 0x
                    let hex_start = self.pos;
                    while self.pos < self.source.len() && self.source[self.pos].is_ascii_hexdigit() {
                        self.advance();
                    }
                    let hex_str = std::str::from_utf8(&self.source[hex_start..self.pos]).unwrap();
                    let val = i64::from_str_radix(hex_str, 16).map_err(|_| {
                        Diagnostic::error("invalid hex literal", Span::new(start, self.pos, start_line, start_col))
                    })?;
                    return Ok(TokenKind::Integer(if negative { -val } else { val }));
                }
                b'b' | b'B' => {
                    self.advance(); self.advance(); // skip 0b
                    let bin_start = self.pos;
                    while self.pos < self.source.len() && (self.source[self.pos] == b'0' || self.source[self.pos] == b'1') {
                        self.advance();
                    }
                    let bin_str = std::str::from_utf8(&self.source[bin_start..self.pos]).unwrap();
                    let val = i64::from_str_radix(bin_str, 2).map_err(|_| {
                        Diagnostic::error("invalid binary literal", Span::new(start, self.pos, start_line, start_col))
                    })?;
                    return Ok(TokenKind::Integer(if negative { -val } else { val }));
                }
                _ => {}
            }
        }

        // Decimal integer or float
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
            self.advance();
        }

        let mut is_float = false;

        // Check for dot (float)
        if self.pos < self.source.len() && self.source[self.pos] == b'.'
            && self.peek_at(1).map_or(false, |c| c.is_ascii_digit())
        {
            is_float = true;
            self.advance(); // skip dot
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.advance();
            }
        }

        // Check for exponent
        if self.pos < self.source.len() && (self.source[self.pos] == b'e' || self.source[self.pos] == b'E') {
            is_float = true;
            self.advance();
            if self.pos < self.source.len() && (self.source[self.pos] == b'+' || self.source[self.pos] == b'-') {
                self.advance();
            }
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.advance();
            }
        }

        // Skip optional float suffix (f32, f64)
        if self.pos < self.source.len() && self.source[self.pos] == b'f' {
            is_float = true;
            self.advance();
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.advance();
            }
        }

        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        if is_float {
            // Strip optional float suffix (e.g., "3.14f32" → "3.14")
            let parse_text = if let Some(idx) = text.find('f') {
                &text[..idx]
            } else {
                text
            };
            let val: f64 = parse_text.parse().map_err(|_| {
                Diagnostic::error("invalid float literal", Span::new(start, self.pos, start_line, start_col))
            })?;
            Ok(TokenKind::Float(val))
        } else {
            let val: i64 = text.parse().map_err(|_| {
                Diagnostic::error("invalid integer literal", Span::new(start, self.pos, start_line, start_col))
            })?;
            Ok(TokenKind::Integer(val))
        }
    }

    fn lex_keyword_or_colon(&mut self) -> TokenKind {
        self.advance(); // skip ':'
        if self.pos < self.source.len() && is_symbol_char(self.source[self.pos]) {
            let start = self.pos;
            while self.pos < self.source.len() && is_symbol_char(self.source[self.pos]) {
                self.advance();
            }
            let name = std::str::from_utf8(&self.source[start..self.pos]).unwrap().to_string();
            TokenKind::Keyword(name)
        } else {
            TokenKind::Colon
        }
    }

    fn lex_symbol(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.source.len() && is_symbol_char(self.source[self.pos]) {
            // Allow : inside symbols for agent:name syntax, but only if followed by a symbol char
            if self.source[self.pos] == b':' {
                if self.peek_at(1).map_or(false, |c| is_symbol_start(c)) {
                    self.advance(); // consume the colon
                    continue;
                } else {
                    break;
                }
            }
            self.advance();
        }
        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        match text {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "nil" => TokenKind::Nil,
            _ => TokenKind::Symbol(text.to_string()),
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), Diagnostic> {
        loop {
            // Skip whitespace
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                self.advance();
            }

            if self.pos >= self.source.len() {
                break;
            }

            // Line comment
            if self.source[self.pos] == b';' {
                while self.pos < self.source.len() && self.source[self.pos] != b'\n' {
                    self.advance();
                }
                continue;
            }

            // Block comment #| ... |#
            if self.pos + 1 < self.source.len() && self.source[self.pos] == b'#' && self.source[self.pos + 1] == b'|' {
                let start = self.pos;
                let start_line = self.line;
                let start_col = self.col;
                self.advance(); self.advance(); // skip #|
                let mut depth = 1u32;
                while self.pos < self.source.len() && depth > 0 {
                    if self.pos + 1 < self.source.len() && self.source[self.pos] == b'#' && self.source[self.pos + 1] == b'|' {
                        depth += 1;
                        self.advance(); self.advance();
                    } else if self.pos + 1 < self.source.len() && self.source[self.pos] == b'|' && self.source[self.pos + 1] == b'#' {
                        depth -= 1;
                        self.advance(); self.advance();
                    } else {
                        if self.source[self.pos] == b'\n' {
                            self.line += 1;
                            self.col = 0;
                        }
                        self.advance();
                    }
                }
                if depth > 0 {
                    return Err(Diagnostic::error(
                        "unterminated block comment",
                        Span::new(start, self.pos, start_line, start_col),
                    ));
                }
                continue;
            }

            break;
        }
        Ok(())
    }

    fn advance(&mut self) {
        if self.pos < self.source.len() {
            if self.source[self.pos] == b'\n' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.source.get(self.pos + offset).copied()
    }

    fn current_span(&self, len: usize) -> Span {
        Span::new(self.pos, self.pos + len, self.line, self.col)
    }
}

fn is_symbol_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || matches!(ch, b'_' | b'+' | b'-' | b'*' | b'/' | b'%'
        | b'<' | b'>' | b'=' | b'!' | b'&' | b'|' | b'?' | b'.')
}

fn is_symbol_char(ch: u8) -> bool {
    is_symbol_start(ch) || ch.is_ascii_digit() || ch == b'-' || ch == b':'
}
```

Note: The lexer uses a single `advance()` method that handles newline tracking. Block comments also call `advance()` which correctly tracks line/col through multi-line comments.

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-syntax`
Expected: all lexer tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-syntax/src/token.rs crates/airl-syntax/src/lexer.rs
git commit -m "feat(syntax): add token types and hand-written lexer"
```

---

## Task 4: S-Expression Parser (`airl-syntax`)

**Files:**
- Create: `crates/airl-syntax/src/sexpr.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write SExpr type and tests**

In `crates/airl-syntax/src/sexpr.rs`:
```rust
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::diagnostic::Diagnostic;

/// A generic S-expression: either an atom or a list of S-expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum SExpr {
    Atom(Atom),
    List(Vec<SExpr>, Span),        // span covers the outer parens
    BracketList(Vec<SExpr>, Span), // [...] form
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub kind: AtomKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AtomKind {
    Integer(i64),
    Float(f64),
    Str(String),
    Symbol(String),
    Keyword(String),
    Bool(bool),
    Nil,
    Arrow,  // -> preserved as atom for signature parsing
}

impl SExpr {
    pub fn span(&self) -> Span {
        match self {
            SExpr::Atom(a) => a.span,
            SExpr::List(_, s) | SExpr::BracketList(_, s) => *s,
        }
    }

    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            SExpr::Atom(Atom { kind: AtomKind::Keyword(s), .. }) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Parse a token stream into a list of top-level S-expressions.
pub fn parse_sexpr_all(tokens: &[Token]) -> Result<Vec<SExpr>, Diagnostic> {
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < tokens.len() && tokens[pos].kind != TokenKind::Eof {
        let (expr, next) = parse_sexpr(tokens, pos)?;
        exprs.push(expr);
        pos = next;
    }
    Ok(exprs)
}

fn parse_sexpr(tokens: &[Token], pos: usize) -> Result<(SExpr, usize), Diagnostic> {
    if pos >= tokens.len() {
        return Err(Diagnostic::error("unexpected end of input", Span::dummy()));
    }

    let token = &tokens[pos];
    match &token.kind {
        TokenKind::LParen => parse_list(tokens, pos, TokenKind::LParen, TokenKind::RParen, false),
        TokenKind::LBracket => parse_list(tokens, pos, TokenKind::LBracket, TokenKind::RBracket, true),
        _ => {
            let atom = token_to_atom(token)?;
            Ok((SExpr::Atom(atom), pos + 1))
        }
    }
}

fn parse_list(
    tokens: &[Token],
    pos: usize,
    _open: TokenKind,
    close: TokenKind,
    is_bracket: bool,
) -> Result<(SExpr, usize), Diagnostic> {
    let start_span = tokens[pos].span;
    let mut pos = pos + 1; // skip opener
    let mut items = Vec::new();

    loop {
        if pos >= tokens.len() || tokens[pos].kind == TokenKind::Eof {
            return Err(Diagnostic::error(
                "unclosed delimiter",
                start_span,
            ));
        }
        // Skip commas in bracket lists (type parameter separators)
        if tokens[pos].kind == TokenKind::Comma {
            pos += 1;
            continue;
        }
        if tokens[pos].kind == close {
            let end_span = tokens[pos].span;
            let span = start_span.merge(end_span);
            let expr = if is_bracket {
                SExpr::BracketList(items, span)
            } else {
                SExpr::List(items, span)
            };
            return Ok((expr, pos + 1));
        }
        let (item, next) = parse_sexpr(tokens, pos)?;
        items.push(item);
        pos = next;
    }
}

fn token_to_atom(token: &Token) -> Result<Atom, Diagnostic> {
    let kind = match &token.kind {
        TokenKind::Integer(v) => AtomKind::Integer(*v),
        TokenKind::Float(v) => AtomKind::Float(*v),
        TokenKind::Str(v) => AtomKind::Str(v.clone()),
        TokenKind::Symbol(v) => AtomKind::Symbol(v.clone()),
        TokenKind::Keyword(v) => AtomKind::Keyword(v.clone()),
        TokenKind::Bool(v) => AtomKind::Bool(*v),
        TokenKind::Nil => AtomKind::Nil,
        TokenKind::Arrow => AtomKind::Arrow,
        TokenKind::Colon => AtomKind::Symbol(":".into()),
        other => {
            return Err(Diagnostic::error(
                format!("unexpected token: {:?}", other),
                token.span,
            ));
        }
    };
    Ok(Atom { kind, span: token.span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(input: &str) -> Vec<SExpr> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        parse_sexpr_all(&tokens).unwrap()
    }

    #[test]
    fn parse_single_atom() {
        let exprs = parse("42");
        assert_eq!(exprs.len(), 1);
        assert!(matches!(&exprs[0], SExpr::Atom(Atom { kind: AtomKind::Integer(42), .. })));
    }

    #[test]
    fn parse_simple_list() {
        let exprs = parse("(+ 1 2)");
        assert_eq!(exprs.len(), 1);
        if let SExpr::List(items, _) = &exprs[0] {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].as_symbol(), Some("+"));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn parse_nested_list() {
        let exprs = parse("(+ (* 2 3) 4)");
        assert_eq!(exprs.len(), 1);
        if let SExpr::List(items, _) = &exprs[0] {
            assert_eq!(items.len(), 3);
            assert!(matches!(&items[1], SExpr::List(..)));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn parse_bracket_list() {
        let exprs = parse("[a b c]");
        assert_eq!(exprs.len(), 1);
        assert!(matches!(&exprs[0], SExpr::BracketList(items, _) if items.len() == 3));
    }

    #[test]
    fn parse_bracket_with_commas() {
        // Type params: [T, E] — commas are separator noise
        let exprs = parse("[T, E]");
        if let SExpr::BracketList(items, _) = &exprs[0] {
            assert_eq!(items.len(), 2);
        } else {
            panic!("expected bracket list");
        }
    }

    #[test]
    fn parse_multiple_top_level() {
        let exprs = parse("(a) (b) (c)");
        assert_eq!(exprs.len(), 3);
    }

    #[test]
    fn parse_defn_structure() {
        let exprs = parse(r#"(defn foo :sig [(a : i32) -> i32] :body (+ a 1))"#);
        assert_eq!(exprs.len(), 1);
        if let SExpr::List(items, _) = &exprs[0] {
            assert_eq!(items[0].as_symbol(), Some("defn"));
            assert_eq!(items[1].as_symbol(), Some("foo"));
            assert_eq!(items[2].as_keyword(), Some("sig"));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn parse_unclosed_paren_error() {
        let mut lexer = Lexer::new("(+ 1 2");
        let tokens = lexer.lex_all().unwrap();
        assert!(parse_sexpr_all(&tokens).is_err());
    }

    #[test]
    fn roundtrip_preserves_structure() {
        // Parse, pretty-print, re-parse should give structurally equivalent result
        let input = "(defn foo :sig [(a : i32) -> i32] :body (+ a 1))";
        let exprs1 = parse(input);
        // For now just verify it parses without panic
        assert_eq!(exprs1.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-syntax`
Expected: all S-expr parser tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-syntax/src/sexpr.rs
git commit -m "feat(syntax): add S-expression parser"
```

---

## Task 5: AST Types (`airl-syntax`)

**Files:**
- Create: `crates/airl-syntax/src/ast.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Define all AST node types**

In `crates/airl-syntax/src/ast.rs`:
```rust
use crate::span::Span;
use std::collections::BTreeMap;

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
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-syntax`
Expected: pass (this is mostly type definitions)

- [ ] **Step 3: Commit**

```bash
git add crates/airl-syntax/src/ast.rs
git commit -m "feat(syntax): add complete AST type definitions"
```

---

## Task 6: Form Parser — SExpr to AST (`airl-syntax`)

**Files:**
- Create: `crates/airl-syntax/src/parser.rs`
- Test: inline `#[cfg(test)]` module

This is the largest single file. It walks the S-expression tree and recognizes AIRL-specific forms.

- [ ] **Step 1: Write failing tests for all major forms**

In `crates/airl-syntax/src/parser.rs`, write the test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::sexpr::parse_sexpr_all;

    fn parse_top(input: &str) -> Vec<TopLevel> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let tops: Vec<_> = sexprs.iter().map(|s| parse_top_level(s, &mut diags)).collect::<Result<_, _>>().unwrap();
        assert!(!diags.has_errors(), "unexpected errors: {:?}", diags);
        tops
    }

    fn parse_expr_str(input: &str) -> Expr {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        parse_expr(&sexprs[0], &mut diags).unwrap()
    }

    #[test]
    fn parse_defn_safe_divide() {
        let tops = parse_top(r#"
            (defn safe-divide
              :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
              :intent "Divide a by b"
              :requires [(valid a) (valid b)]
              :ensures [(pure)]
              :body (if (= b 0) (Err :division-by-zero) (Ok (/ a b))))
        "#);
        assert_eq!(tops.len(), 1);
        if let TopLevel::Defn(f) = &tops[0] {
            assert_eq!(f.name, "safe-divide");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.intent.as_deref(), Some("Divide a by b"));
            assert_eq!(f.requires.len(), 2);
            assert_eq!(f.ensures.len(), 1);
        } else {
            panic!("expected Defn");
        }
    }

    #[test]
    fn parse_deftype_sum() {
        let tops = parse_top(r#"
            (deftype Result [T : Type, E : Type]
              (| (Ok T) (Err E)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "Result");
            assert_eq!(td.type_params.len(), 2);
            assert!(matches!(td.body, TypeDefBody::Sum(_)));
        } else {
            panic!("expected DefType");
        }
    }

    #[test]
    fn parse_deftype_product() {
        let tops = parse_top(r#"
            (deftype AgentMessage
              (& (id : String) (from : AgentId)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "AgentMessage");
            if let TypeDefBody::Product(fields) = &td.body {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "id");
            } else {
                panic!("expected Product");
            }
        } else {
            panic!("expected DefType");
        }
    }

    #[test]
    fn parse_module() {
        let tops = parse_top(r#"
            (module my-service
              :version 0.1.0
              :requires [tensor]
              :provides [public-fn]
              :verify proven
              (defn public-fn
                :sig [(x : i32) -> i32]
                :intent "identity"
                :requires [(valid x)]
                :ensures [(= result x)]
                :body x))
        "#);
        if let TopLevel::Module(m) = &tops[0] {
            assert_eq!(m.name, "my-service");
            assert_eq!(m.verify, VerifyLevel::Proven);
            assert_eq!(m.body.len(), 1);
        } else {
            panic!("expected Module");
        }
    }

    #[test]
    fn parse_use_symbols() {
        let tops = parse_top("(use tensor [matmul transpose])");
        if let TopLevel::UseDecl(u) = &tops[0] {
            assert_eq!(u.module, "tensor");
            assert!(matches!(&u.kind, UseKind::Symbols(syms) if syms.len() == 2));
        } else {
            panic!("expected UseDecl");
        }
    }

    #[test]
    fn parse_use_prefixed() {
        let tops = parse_top("(use agent :as ag)");
        if let TopLevel::UseDecl(u) = &tops[0] {
            assert!(matches!(&u.kind, UseKind::Prefixed(p) if p == "ag"));
        } else {
            panic!("expected UseDecl");
        }
    }

    #[test]
    fn parse_if_expr() {
        let e = parse_expr_str("(if true 1 2)");
        assert!(matches!(e.kind, ExprKind::If(..)));
    }

    #[test]
    fn parse_let_expr() {
        let e = parse_expr_str("(let (x : i32 42) (+ x 1))");
        if let ExprKind::Let(bindings, _body) = &e.kind {
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].name, "x");
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_multi_binding() {
        let e = parse_expr_str("(let (x : i32 1) (y : i32 2) (+ x y))");
        if let ExprKind::Let(bindings, _) = &e.kind {
            assert_eq!(bindings.len(), 2);
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_match_expr() {
        let e = parse_expr_str("(match x (Ok v) (use-v v) (Err e) (handle e))");
        assert!(matches!(e.kind, ExprKind::Match(..)));
    }

    #[test]
    fn parse_do_expr() {
        let e = parse_expr_str("(do (step1) (step2) (step3))");
        if let ExprKind::Do(exprs) = &e.kind {
            assert_eq!(exprs.len(), 3);
        } else {
            panic!("expected Do");
        }
    }

    #[test]
    fn parse_lambda() {
        let e = parse_expr_str("(fn [a b] (+ a b))");
        assert!(matches!(e.kind, ExprKind::Lambda(..)));
    }

    #[test]
    fn parse_try() {
        let e = parse_expr_str("(try (parse data))");
        assert!(matches!(e.kind, ExprKind::Try(..)));
    }

    #[test]
    fn parse_fn_call() {
        let e = parse_expr_str("(+ 1 2)");
        if let ExprKind::FnCall(callee, args) = &e.kind {
            assert_eq!(args.len(), 2);
            assert!(matches!(callee.kind, ExprKind::SymbolRef(ref s) if s == "+"));
        } else {
            panic!("expected FnCall");
        }
    }

    #[test]
    fn parse_task() {
        let tops = parse_top(r#"
            (task "research-kv-cache"
              :from agent:orchestrator
              :to agent:research
              :intent "Find papers"
              :input [(query : String "test")]
              :on-success (send agent:orchestrator result))
        "#);
        if let TopLevel::Task(t) = &tops[0] {
            assert_eq!(t.id, "research-kv-cache");
            assert!(t.intent.is_some());
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_missing_contracts_is_error() {
        let input = r#"(defn bad :sig [(x : i32) -> i32] :body x)"#;
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let _ = parse_top_level(&sexprs[0], &mut diags);
        assert!(diags.has_errors(), "expected error for missing contracts");
    }
}
```

- [ ] **Step 2: Implement the form parser**

In `crates/airl-syntax/src/parser.rs`, implement:
- `parse_top_level(sexpr, diags) -> Result<TopLevel, Diagnostic>` — dispatches on first symbol
- `parse_defn(items, span, diags) -> Result<FnDef, Diagnostic>` — walks keyword-value pairs
- `parse_deftype(items, span, diags) -> Result<TypeDef, Diagnostic>`
- `parse_module(items, span, diags) -> Result<ModuleDef, Diagnostic>`
- `parse_task(items, span, diags) -> Result<TaskDef, Diagnostic>`
- `parse_use(items, span, diags) -> Result<UseDef, Diagnostic>`
- `parse_expr(sexpr, diags) -> Result<Expr, Diagnostic>` — dispatches on form keywords
- `parse_type(sexpr, diags) -> Result<AstType, Diagnostic>`
- `parse_param(sexpr, diags) -> Result<Param, Diagnostic>`
- `parse_pattern(sexpr, diags) -> Result<Pattern, Diagnostic>`

Key implementation notes:
- `defn` parser must reject functions missing `:requires` AND `:ensures` (contracts mandatory per spec §4.1). `:intent` is strongly encouraged but the compiler emits a warning (not error) if missing. `:invariant` is optional.
- The form parser pattern: walk the items list, match keywords, collect attributes. Unknown keywords produce a warning.
- For `let`, detect multi-binding by checking if there are multiple `(name : type value)` forms before the final body expression.
- For `match`, arms come in pairs: `pattern body pattern body ...`

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-syntax`
Expected: all form parser tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-syntax/src/parser.rs
git commit -m "feat(syntax): add form parser (SExpr → AST)"
```

---

## Task 7: Update lib.rs Exports and Compile Check (`airl-syntax`)

**Files:**
- Modify: `crates/airl-syntax/src/lib.rs`

- [ ] **Step 1: Ensure lib.rs exports all modules and key types**

```rust
pub mod span;
pub mod token;
pub mod lexer;
pub mod sexpr;
pub mod ast;
pub mod parser;
pub mod diagnostic;

// Convenience re-exports
pub use span::Span;
pub use token::{Token, TokenKind};
pub use lexer::Lexer;
pub use sexpr::{SExpr, parse_sexpr_all};
pub use ast::*;
pub use parser::parse_top_level;
pub use diagnostic::{Diagnostic, Diagnostics, Severity};
```

- [ ] **Step 2: Verify full crate compiles and tests pass**

Run: `cargo test -p airl-syntax`
Expected: all tests pass, no warnings

- [ ] **Step 3: Commit**

```bash
git add crates/airl-syntax/src/lib.rs
git commit -m "feat(syntax): finalize airl-syntax public API"
```

---

## Task 8: Type Representation (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/ty.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define Ty, PrimTy, DimExpr**

In `crates/airl-types/src/ty.rs`:
```rust
use airl_syntax::Span;

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
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-types/src/ty.rs
git commit -m "feat(types): add Ty, PrimTy, DimExpr type representation"
```

---

## Task 9: Type Environment (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/env.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement TypeEnv with scoping**

In `crates/airl-types/src/env.rs`:
```rust
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-types/src/env.rs
git commit -m "feat(types): add scoped TypeEnv"
```

---

## Task 10: DimExpr Unification (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/unify.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement dimension unification**

This is the core of dependent type checking. When two tensor types must be compatible (e.g., matrix multiply), their dimension expressions must unify.

```rust
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
            // Occurs check
            if occurs(v, other) {
                return Err(format!("circular dimension: {} occurs in {:?}", v, other));
            }
            subst.insert(v.clone(), other.clone());
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

fn occurs(var: &str, dim: &DimExpr) -> bool {
    match dim {
        DimExpr::Var(v) => v == var,
        DimExpr::Lit(_) => false,
        DimExpr::BinOp(_, l, r) => occurs(var, l) || occurs(var, r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var("M".into()), &DimExpr::Lit(64), &mut s).unwrap();
        assert_eq!(s.get("M"), Some(&DimExpr::Lit(64)));
    }

    #[test]
    fn unify_shared_dimension() {
        // Matrix multiply: tensor[f32 M K] * tensor[f32 K N]
        // K must unify across both
        let mut s = DimSubst::new();
        // First call: K unifies with Lit(32)
        unify_dim(&DimExpr::Var("K".into()), &DimExpr::Lit(32), &mut s).unwrap();
        // Second call: K (now 32) must match Lit(32)
        unify_dim(&DimExpr::Var("K".into()), &DimExpr::Lit(32), &mut s).unwrap();
        assert_eq!(s.get("K"), Some(&DimExpr::Lit(32)));
    }

    #[test]
    fn unify_shared_dimension_mismatch() {
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var("K".into()), &DimExpr::Lit(32), &mut s).unwrap();
        // K is now 32, trying to unify with 64 should fail
        assert!(unify_dim(&DimExpr::Var("K".into()), &DimExpr::Lit(64), &mut s).is_err());
    }

    #[test]
    fn unify_two_vars() {
        let mut s = DimSubst::new();
        unify_dim(&DimExpr::Var("M".into()), &DimExpr::Var("N".into()), &mut s).unwrap();
        // M → N or N → M
        assert!(s.contains_key("M") || s.contains_key("N"));
    }

    #[test]
    fn eval_binop() {
        let expr = DimExpr::BinOp(DimOp::Add, Box::new(DimExpr::Lit(3)), Box::new(DimExpr::Lit(4)));
        assert_eq!(eval_dim(&expr), Some(7));
    }

    #[test]
    fn eval_with_var_returns_none() {
        assert_eq!(eval_dim(&DimExpr::Var("M".into())), None);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-types/src/unify.rs
git commit -m "feat(types): add DimExpr unification for dependent types"
```

---

## Task 11a: Type Checker — Type Resolution and Expression Checking (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/checker.rs`
- Test: inline `#[cfg(test)]`

This is the first half of the type checker: resolving AST types to internal Ty, and checking basic expressions (literals, let, if, do).

- [ ] **Step 1: Write failing tests for type resolution and basic expression checking**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn check_expr_type(input: &str) -> Result<Ty, String> {
        // Helper: parse input as an expression, type-check it, return type
        ...
    }

    #[test]
    fn resolve_primitive_types() {
        let mut checker = TypeChecker::new();
        let ty = checker.resolve_type_name("i32").unwrap();
        assert_eq!(ty, Ty::Prim(PrimTy::I32));
    }

    #[test]
    fn resolve_tensor_type() {
        let mut checker = TypeChecker::new();
        // tensor[f32 64 64] → Tensor { elem: Prim(F32), shape: [Lit(64), Lit(64)] }
        ...
    }

    #[test]
    fn check_int_literal() {
        assert_eq!(check_expr_type("42"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_string_literal() {
        assert_eq!(check_expr_type(r#""hello""#), Ok(Ty::Prim(PrimTy::Str)));
    }

    #[test]
    fn check_arithmetic_same_type() {
        // (+ 1 2) → i64
        assert_eq!(check_expr_type("(+ 1 2)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_arithmetic_type_mismatch() {
        // Can't add int and string
        assert!(check_expr_type(r#"(+ 1 "hello")"#).is_err());
    }

    #[test]
    fn check_let_binding_type() {
        // (let (x : i32 42) x) → i32
        assert_eq!(check_expr_type("(let (x : i32 42) x)"), Ok(Ty::Prim(PrimTy::I32)));
    }

    #[test]
    fn check_if_branches_same_type() {
        assert_eq!(check_expr_type("(if true 1 2)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_if_branches_different_type() {
        assert!(check_expr_type(r#"(if true 1 "hello")"#).is_err());
    }
}
```

- [ ] **Step 2: Implement TypeChecker struct, resolve_type, and check_expr for basic forms**

```rust
use airl_syntax::{ast, Span, Diagnostic, Diagnostics};
use crate::ty::*;
use crate::env::TypeEnv;
use crate::unify::DimSubst;

pub struct TypeChecker {
    pub env: TypeEnv,
    pub dim_subst: DimSubst,
    diags: Diagnostics,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            dim_subst: DimSubst::new(),
            diags: Diagnostics::new(),
        }
    }

    /// Resolve an AST type name to an internal Ty.
    pub fn resolve_type_name(&self, name: &str) -> Result<Ty, ()> {
        if let Some(prim) = PrimTy::from_name(name) {
            return Ok(Ty::Prim(prim));
        }
        match name {
            "Unit" => Ok(Ty::Unit),
            "Never" => Ok(Ty::Never),
            _ => {
                if let Some(reg) = self.env.lookup_type(name) {
                    Ok(reg.ty.clone())
                } else {
                    Err(())
                }
            }
        }
    }

    /// Resolve a full AST type node to internal Ty.
    pub fn resolve_type(&mut self, ast_ty: &ast::AstType) -> Result<Ty, ()> {
        match &ast_ty.kind {
            ast::AstTypeKind::Named(name) => self.resolve_type_name(name),
            ast::AstTypeKind::App(name, args) => {
                if name == "tensor" {
                    // tensor[ElemType Dim1 Dim2 ...]
                    let elem = self.resolve_type(&args[0])?;
                    let shape = args[1..].iter().map(|a| self.resolve_dim(a)).collect::<Result<_, _>>()?;
                    Ok(Ty::Tensor { elem: Box::new(elem), shape })
                } else {
                    // Named type application: Result[i32, DivError]
                    let resolved_args = args.iter().map(|a| {
                        self.resolve_type(a).map(TyArg::Type)
                    }).collect::<Result<_, _>>()?;
                    Ok(Ty::Named { name: name.clone(), args: resolved_args })
                }
            }
            ast::AstTypeKind::Func(params, ret) => {
                let param_tys = params.iter().map(|p| self.resolve_type(p)).collect::<Result<_, _>>()?;
                let ret_ty = self.resolve_type(ret)?;
                Ok(Ty::Func { params: param_tys, ret: Box::new(ret_ty) })
            }
            ast::AstTypeKind::Nat(nat) => Ok(Ty::Nat(self.ast_nat_to_dim(nat))),
        }
    }

    /// Check an expression and return its type.
    pub fn check_expr(&mut self, expr: &ast::Expr) -> Result<Ty, ()> {
        match &expr.kind {
            ast::ExprKind::IntLit(_) => Ok(Ty::Prim(PrimTy::I64)),
            ast::ExprKind::FloatLit(_) => Ok(Ty::Prim(PrimTy::F64)),
            ast::ExprKind::BoolLit(_) => Ok(Ty::Prim(PrimTy::Bool)),
            ast::ExprKind::StrLit(_) => Ok(Ty::Prim(PrimTy::Str)),
            ast::ExprKind::NilLit => Ok(Ty::Unit),
            ast::ExprKind::SymbolRef(name) => {
                self.env.lookup(name).cloned().ok_or_else(|| {
                    self.diags.add(Diagnostic::error(
                        format!("undefined symbol: `{}`", name), expr.span));
                })
            }
            ast::ExprKind::If(cond, then, else_) => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Ty::Prim(PrimTy::Bool) {
                    self.diags.add(Diagnostic::error("if condition must be bool", cond.span));
                    return Err(());
                }
                let then_ty = self.check_expr(then)?;
                let else_ty = self.check_expr(else_)?;
                if then_ty != else_ty {
                    self.diags.add(Diagnostic::error(
                        format!("if branches have different types: {:?} vs {:?}", then_ty, else_ty),
                        expr.span));
                    return Err(());
                }
                Ok(then_ty)
            }
            ast::ExprKind::Let(bindings, body) => {
                self.env.push_scope();
                for b in bindings {
                    let declared = self.resolve_type(&b.ty)?;
                    let actual = self.check_expr(&b.value)?;
                    // TODO: check declared == actual
                    self.env.bind(b.name.clone(), declared);
                }
                let body_ty = self.check_expr(body)?;
                self.env.pop_scope();
                Ok(body_ty)
            }
            ast::ExprKind::Do(exprs) => {
                let mut ty = Ty::Unit;
                for e in exprs { ty = self.check_expr(e)?; }
                Ok(ty)
            }
            // FnCall, Match, Lambda, Try handled in Task 11b
            _ => {
                self.diags.add(Diagnostic::error(
                    format!("type checking not yet implemented for {:?}", expr.kind), expr.span));
                Err(())
            }
        }
    }

    fn resolve_dim(&mut self, ast_ty: &ast::AstType) -> Result<DimExpr, ()> { ... }
    fn ast_nat_to_dim(&self, nat: &ast::NatExpr) -> DimExpr { ... }

    pub fn into_diagnostics(self) -> Diagnostics { self.diags }
    pub fn has_errors(&self) -> bool { self.diags.has_errors() }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-types/src/checker.rs
git commit -m "feat(types): add type checker — type resolution and basic expression checking"
```

---

## Task 11b: Type Checker — Function Calls, Match, and Dimension Unification (`airl-types`)

**Files:**
- Modify: `crates/airl-types/src/checker.rs`
- Test: inline `#[cfg(test)]`

This extends the type checker with function call checking (including tensor dim unification), match, lambda, and try.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn check_fn_call_correct_types() {
    // Define a function (a : i32) (b : i32) -> i32, call with two ints
    ...
}

#[test]
fn check_fn_call_wrong_arg_type() {
    // Call an i32 function with a string → error
    ...
}

#[test]
fn check_tensor_dim_unification() {
    // matrix-multiply: tensor[f32 M K] * tensor[f32 K N] → tensor[f32 M N]
    // Calling with tensor[f32 3 4] and tensor[f32 4 5] → K unifies to 4
    ...
}

#[test]
fn check_tensor_dim_mismatch() {
    // tensor[f32 3 4] * tensor[f32 5 6] → K mismatch (4 vs 5) → error
    ...
}

#[test]
fn check_match_type() {
    // match returns the common type of all arms
    ...
}

#[test]
fn check_lambda_type() {
    // (fn [x] (+ x 1)) → (-> [i64] i64)
    ...
}

#[test]
fn check_try_unwraps_result() {
    // (try (Ok 42)) inside a function returning Result → type is i32
    ...
}
```

- [ ] **Step 2: Extend check_expr with FnCall, Match, Lambda, Try**

```rust
// Add to check_expr match arms:

ast::ExprKind::FnCall(callee, args) => {
    let callee_ty = self.check_expr(callee)?;
    match callee_ty {
        Ty::Func { params, ret } => {
            if args.len() != params.len() {
                self.diags.add(Diagnostic::error(...));
                return Err(());
            }
            for (arg, param_ty) in args.iter().zip(params.iter()) {
                let arg_ty = self.check_expr(arg)?;
                self.check_assignable(&arg_ty, param_ty, arg.span)?;
            }
            Ok(*ret)
        }
        _ => { /* check builtins, report error */ }
    }
}

ast::ExprKind::Match(scrutinee, arms) => {
    let scrut_ty = self.check_expr(scrutinee)?;
    let mut result_ty = None;
    for arm in arms {
        self.env.push_scope();
        self.check_pattern(&arm.pattern, &scrut_ty)?;
        let arm_ty = self.check_expr(&arm.body)?;
        self.env.pop_scope();
        if let Some(ref prev) = result_ty {
            if arm_ty != *prev {
                self.diags.add(Diagnostic::error("match arms have different types", arm.span));
                return Err(());
            }
        } else {
            result_ty = Some(arm_ty);
        }
    }
    result_ty.ok_or(())
}
```

Also add `check_top_level` and `check_fn` methods:

```rust
pub fn check_top_level(&mut self, top: &ast::TopLevel) -> Result<(), ()> {
    match top {
        ast::TopLevel::Defn(f) => { self.check_fn(f)?; Ok(()) }
        ast::TopLevel::DefType(td) => { self.register_type_def(td)?; Ok(()) }
        ast::TopLevel::Module(m) => {
            for item in &m.body { self.check_top_level(item)?; }
            Ok(())
        }
        _ => Ok(())
    }
}

pub fn check_fn(&mut self, f: &ast::FnDef) -> Result<Ty, ()> {
    self.env.push_scope();
    let mut param_tys = Vec::new();
    for p in &f.params {
        let ty = self.resolve_type(&p.ty)?;
        self.env.bind(p.name.clone(), ty.clone());
        param_tys.push(ty);
    }
    let declared_ret = self.resolve_type(&f.return_type)?;
    let body_ty = self.check_expr(&f.body)?;
    // check body_ty is assignable to declared_ret
    self.check_assignable(&body_ty, &declared_ret, f.body.span)?;
    self.env.pop_scope();
    let fn_ty = Ty::Func { params: param_tys, ret: Box::new(declared_ret) };
    self.env.bind(f.name.clone(), fn_ty.clone());
    Ok(fn_ty)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-types/src/checker.rs
git commit -m "feat(types): add function call checking, match, and tensor dim unification"
```

---

## Task 12: Match Exhaustiveness Checker (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/exhaustiveness.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write tests and implement exhaustiveness checking**

For sum types, verify that all variants are covered. For bool, verify true+false. For wildcard/binding, always exhaustive.

```rust
pub fn check_exhaustiveness(
    scrutinee_ty: &Ty,
    arms: &[&PatternKind],
    env: &TypeEnv,
) -> Result<(), Vec<String>> {
    // Returns Ok if exhaustive, Err with missing patterns
    ...
}
```

Test cases:
- Result with Ok+Err → exhaustive ✓
- Result with only Ok → missing Err ✓
- bool with true+false → exhaustive ✓
- Wildcard pattern → always exhaustive ✓
- Nested patterns → check inner exhaustiveness ✓

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-types/src/exhaustiveness.rs
git commit -m "feat(types): add match exhaustiveness checker"
```

---

## Task 13: Linearity Checker (`airl-types`)

**Files:**
- Create: `crates/airl-types/src/linearity.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for ownership violations**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn check_linearity(input: &str) -> Result<(), Vec<String>> {
        // Parse, type-check, then linearity-check the input
        ...
    }

    #[test]
    fn valid_owned_use() {
        // Using an owned value once is fine
        assert!(check_linearity("(let (x : i32 42) x)").is_ok());
    }

    #[test]
    fn use_after_move() {
        // Passing x to consume (own), then using x again → error
        let result = check_linearity(r#"
            (let (x : i32 42)
              (do (consume x) x))
        "#);
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("moved"));
    }

    #[test]
    fn double_mut_borrow() {
        let result = check_linearity(r#"
            (let (x : i32 42)
              (do (mutate &mut x) (mutate &mut x)))
        "#);
        // Two sequential &mut borrows are fine (first ends before second)
        // But simultaneous would fail — tested via nested calls
        assert!(result.is_ok());
    }

    #[test]
    fn mut_borrow_while_ref_exists() {
        let result = check_linearity(r#"
            (let (x : i32 42)
              (use-both (&ref x) (&mut x)))
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn move_while_borrowed() {
        let result = check_linearity(r#"
            (let (x : i32 42)
              (let (r : &i32 (&ref x))
                (consume x)))
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn explicit_copy_allowed() {
        assert!(check_linearity(r#"
            (let (x : i32 42)
              (do (consume (copy x)) x))
        "#).is_ok());
    }

    #[test]
    fn branches_must_agree() {
        // If one branch moves x and other doesn't → error
        let result = check_linearity(r#"
            (let (x : i32 42)
              (do
                (if true (consume x) nil)
                x))
        "#);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Implement LinearityChecker**

```rust
use airl_syntax::{ast, Span, Diagnostic, Diagnostics};
use crate::ty::{Ty, is_copy};
use crate::env::TypeEnv;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind { Immutable, Mutable }

#[derive(Debug, Clone, PartialEq)]
pub enum OwnershipState {
    Owned,
    Borrowed { kind: BorrowKind, count: usize },
    Moved { moved_at: Span },
    Dropped,
}

pub struct LinearityChecker {
    /// Maps binding names to their ownership state
    states: Vec<HashMap<String, OwnershipState>>, // scoped stack
    diags: Diagnostics,
}

impl LinearityChecker {
    pub fn new() -> Self {
        Self { states: vec![HashMap::new()], diags: Diagnostics::new() }
    }

    pub fn introduce(&mut self, name: String) {
        if let Some(scope) = self.states.last_mut() {
            scope.insert(name, OwnershipState::Owned);
        }
    }

    pub fn track_move(&mut self, name: &str, span: Span) -> Result<(), ()> {
        let state = self.lookup_state(name);
        match state {
            Some(OwnershipState::Owned) => {
                self.set_state(name, OwnershipState::Moved { moved_at: span });
                Ok(())
            }
            Some(OwnershipState::Moved { moved_at }) => {
                self.diags.add(
                    Diagnostic::error(format!("use of moved value `{}`", name), span)
                        .with_note(moved_at, "value moved here".into())
                );
                Err(())
            }
            Some(OwnershipState::Borrowed { .. }) => {
                self.diags.add(Diagnostic::error(
                    format!("cannot move `{}` while borrowed", name), span));
                Err(())
            }
            _ => {
                self.diags.add(Diagnostic::error(
                    format!("use of undefined value `{}`", name), span));
                Err(())
            }
        }
    }

    pub fn track_borrow(&mut self, name: &str, kind: BorrowKind, span: Span) -> Result<(), ()> {
        let state = self.lookup_state(name);
        match (state, kind) {
            (Some(OwnershipState::Owned), BorrowKind::Immutable) => {
                self.set_state(name, OwnershipState::Borrowed { kind, count: 1 });
                Ok(())
            }
            (Some(OwnershipState::Borrowed { kind: BorrowKind::Immutable, count }), BorrowKind::Immutable) => {
                self.set_state(name, OwnershipState::Borrowed { kind: BorrowKind::Immutable, count: count + 1 });
                Ok(())
            }
            (Some(OwnershipState::Owned), BorrowKind::Mutable) => {
                self.set_state(name, OwnershipState::Borrowed { kind, count: 1 });
                Ok(())
            }
            (Some(OwnershipState::Borrowed { .. }), BorrowKind::Mutable) => {
                self.diags.add(Diagnostic::error(
                    format!("cannot mutably borrow `{}` — already borrowed", name), span));
                Err(())
            }
            (Some(OwnershipState::Moved { moved_at }), _) => {
                self.diags.add(
                    Diagnostic::error(format!("use of moved value `{}`", name), span)
                        .with_note(moved_at, "value moved here".into()));
                Err(())
            }
            _ => Err(())
        }
    }

    pub fn track_copy(&mut self, name: &str, ty: &Ty, span: Span) -> Result<(), ()> {
        if !is_copy(ty) {
            self.diags.add(Diagnostic::error(
                format!("type {:?} does not implement Copy", ty), span));
            return Err(());
        }
        // Copy doesn't change ownership state
        Ok(())
    }

    fn lookup_state(&self, name: &str) -> Option<OwnershipState> {
        for scope in self.states.iter().rev() {
            if let Some(s) = scope.get(name) { return Some(s.clone()); }
        }
        None
    }

    fn set_state(&mut self, name: &str, state: OwnershipState) {
        for scope in self.states.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), state);
                return;
            }
        }
    }

    pub fn push_scope(&mut self) { self.states.push(HashMap::new()); }
    pub fn pop_scope(&mut self) { if self.states.len() > 1 { self.states.pop(); } }

    /// Snapshot current state for branch checking
    pub fn snapshot(&self) -> Vec<HashMap<String, OwnershipState>> { self.states.clone() }
    pub fn restore(&mut self, snap: Vec<HashMap<String, OwnershipState>>) { self.states = snap; }

    pub fn into_diagnostics(self) -> Diagnostics { self.diags }
    pub fn has_errors(&self) -> bool { self.diags.has_errors() }
}
```

Key rules:
- When a binding is passed to a function with `own` ownership → mark as Moved
- Use after Moved → error with "moved at" span
- `&ref` borrow → increment immutable borrow count
- `&mut` borrow while immutable borrows > 0 → error
- Multiple `&mut` borrows → error
- `copy` on non-Copy type → error
- For `if`/`match`: check both branches independently, then merge states (both must agree)

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-types/src/linearity.rs
git commit -m "feat(types): add linearity checker (borrow checking)"
```

---

## Task 14: Update airl-types lib.rs and Integration Tests

**Files:**
- Modify: `crates/airl-types/src/lib.rs`
- Create: `crates/airl-types/tests/integration.rs`

- [ ] **Step 1: Update lib.rs with re-exports**

- [ ] **Step 2: Write integration tests that parse AIRL source → type check**

Test the full pipeline from source text through type checking:
```rust
fn check(input: &str) -> Diagnostics {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.lex_all().unwrap();
    let sexprs = parse_sexpr_all(&tokens).unwrap();
    let mut diags = Diagnostics::new();
    let tops = sexprs.iter().map(|s| parse_top_level(s, &mut diags)).collect();
    let mut checker = TypeChecker::new();
    for top in tops { checker.check_top_level(&top); }
    checker.into_diagnostics()
}
```

Test cases from the spec:
- `safe-divide` example (§4.2) type checks cleanly
- `matrix-multiply` example (§3.5) with matching K dims
- `matrix-multiply` with mismatched dims → type error
- Missing contracts → error

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-types`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-types/
git commit -m "feat(types): finalize airl-types with integration tests"
```

---

## Task 15: Contract Violation Type (`airl-contracts`)

**Files:**
- Create: `crates/airl-contracts/src/violation.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement ContractViolation matching spec §9.2**

```rust
use airl_syntax::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractKind { Requires, Ensures, Invariant }

#[derive(Debug, Clone)]
pub struct ContractViolation {
    pub function: String,
    pub contract_kind: ContractKind,
    pub clause_source: String,
    pub bindings: Vec<(String, String)>,  // (name, value_display)
    pub evaluated: String,
    pub span: Span,
}

impl std::fmt::Display for ContractViolation { ... }
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(contracts): add ContractViolation type"
```

---

## Task 16: Runtime Contract Assertions — Checked Mode (`airl-contracts`)

**Files:**
- Create: `crates/airl-contracts/src/checked.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define the ContractEvaluator trait and checked implementation**

The checked mode evaluator takes contract expressions and a binding context, evaluates them as booleans, and returns violations.

This depends on having a way to evaluate expressions — it will need a callback or trait for expression evaluation since the actual evaluator lives in `airl-runtime`. Design as a trait:

```rust
/// Trait for evaluating expressions to check contracts.
pub trait ExprEvaluator {
    fn eval_bool(&self, expr: &ast::Expr, bindings: &[(String, Box<dyn std::fmt::Debug>)]) -> Result<bool, String>;
    fn format_value(&self, expr: &ast::Expr) -> String;
}

pub struct CheckedVerifier;

impl CheckedVerifier {
    pub fn check_requires(&self, contracts: &[ast::Expr], eval: &dyn ExprEvaluator, fn_name: &str) -> Result<(), ContractViolation> { ... }
    pub fn check_ensures(&self, contracts: &[ast::Expr], eval: &dyn ExprEvaluator, fn_name: &str) -> Result<(), ContractViolation> { ... }
}
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(contracts): add checked mode contract evaluator"
```

---

## Task 17: Stub Prover — Proven Mode (`airl-contracts`)

**Files:**
- Create: `crates/airl-contracts/src/prover.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn prove_constant_arithmetic() {
    // (= (+ 2 3) 5) → Proven
}

#[test]
fn prove_identity() {
    // (= x x) → Proven
}

#[test]
fn prove_inequality_from_context() {
    // requires: (> n 0), prove: (>= n 1) → Proven
}

#[test]
fn prove_tautology() {
    // (or a (not a)) → Proven
}

#[test]
fn unknown_complex_property() {
    // (forall [i] ...) → Unknown
}
```

- [ ] **Step 2: Implement StubProver**

```rust
pub enum ProofResult {
    Proven,
    Disproven(String),
    Unknown(String),
}

pub struct StubProver {
    assumptions: Vec<ast::Expr>,  // from :requires
}

impl StubProver {
    pub fn new(assumptions: Vec<ast::Expr>) -> Self { ... }
    pub fn prove(&self, claim: &ast::Expr) -> ProofResult { ... }
    fn try_constant_eval(&self, expr: &ast::Expr) -> Option<bool> { ... }
    fn try_identity(&self, expr: &ast::Expr) -> Option<bool> { ... }
    fn try_from_assumptions(&self, expr: &ast::Expr) -> Option<bool> { ... }
    fn try_tautology(&self, expr: &ast::Expr) -> Option<bool> { ... }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat(contracts): add stub symbolic prover"
```

---

## Task 18: Trusted Mode (`airl-contracts`)

**Files:**
- Create: `crates/airl-contracts/src/trusted.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement TrustedVerifier**

Simple: record contracts, never evaluate, emit a note.

```rust
pub struct TrustedVerifier;

impl TrustedVerifier {
    pub fn note(&self, fn_name: &str) -> String {
        format!("note: trusting contracts for `{}`", fn_name)
    }
}
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(contracts): add trusted mode verifier"
```

---

## Task 19: Value and Error Types (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/value.rs`
- Create: `crates/airl-runtime/src/error.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define Value enum with Display**

```rust
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    Unit,
    Tensor(TensorValue),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Variant(String, Box<Value>),
    Struct(BTreeMap<String, Value>),
    Function(FnValue),
    Lambda(LambdaValue),
    BuiltinFn(String),
    AgentId(AgentIdValue),
    TaskResult(TaskResultValue),
}

impl std::fmt::Display for Value { ... }
```

Define `RuntimeError`:
```rust
#[derive(Debug)]
pub enum RuntimeError {
    TypeError(String),
    UseAfterMove { name: String, moved_at: Span },
    ContractViolation(ContractViolation),
    DivisionByZero,
    IndexOutOfBounds { index: usize, len: usize },
    ShapeMismatch { expected: Vec<usize>, got: Vec<usize> },
    UndefinedSymbol(String),
    NotCallable(String),
    Custom(String),
}
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(runtime): add Value and RuntimeError types"
```

---

## Task 20: TensorValue and Tensor Operations (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/tensor.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for tensor ops**

Key tests:
- `tensor.zeros` creates correct shape with all zeros
- `tensor.identity` creates NxN identity matrix
- `tensor.add` element-wise addition with shape validation
- `tensor.matmul` correct result for known inputs (e.g., 2x3 * 3x2)
- `tensor.matmul` shape mismatch → error
- `tensor.reshape` preserves data, validates total size
- `tensor.softmax` output sums to 1.0

- [ ] **Step 2: Implement TensorValue and operations**

```rust
pub struct TensorValue {
    pub dtype: PrimTy,
    pub shape: Vec<usize>,
    pub data: TensorData,
}

pub enum TensorData {
    F32(Vec<f32>),
    F64(Vec<f64>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    // ... other types
}

impl TensorValue {
    pub fn zeros(dtype: PrimTy, shape: Vec<usize>) -> Self { ... }
    pub fn ones(dtype: PrimTy, shape: Vec<usize>) -> Self { ... }
    pub fn rand(dtype: PrimTy, shape: Vec<usize>) -> Self { ... }
    pub fn alloc(dtype: PrimTy, shape: Vec<usize>) -> Self { ... }  // uninitialized
    pub fn identity(dtype: PrimTy, n: usize) -> Self { ... }
    pub fn add(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> { ... }
    pub fn mul(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> { ... }  // element-wise
    pub fn matmul(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> { ... }
    pub fn contract(&self, other: &TensorValue, over: usize) -> Result<TensorValue, RuntimeError> { ... }
    pub fn reshape(&self, new_shape: Vec<usize>) -> Result<TensorValue, RuntimeError> { ... }
    pub fn transpose(&self, perm: &[usize]) -> Result<TensorValue, RuntimeError> { ... }
    pub fn softmax(&self, dim: i64) -> Result<TensorValue, RuntimeError> { ... }
    pub fn sum(&self, dim: i64) -> Result<TensorValue, RuntimeError> { ... }
    pub fn max(&self, dim: i64) -> Result<TensorValue, RuntimeError> { ... }
    pub fn slice(&self, dim: usize, start: usize, end: usize) -> Result<TensorValue, RuntimeError> { ... }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(runtime): add TensorValue with tensor operations"
```

---

## Task 21: Runtime Environment (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/env.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement runtime Env with ownership tracking**

```rust
pub struct Env {
    frames: Vec<Frame>,
}

pub struct Frame {
    bindings: HashMap<String, Slot>,
    kind: FrameKind,
}

pub struct Slot {
    pub value: Value,
    pub ownership: OwnershipState,
}

pub enum FrameKind { Module, Function, Let, Match }
pub enum OwnershipState { Owned, Moved(Span) }
```

Runtime ownership tracking as a double-check: `get()` returns error if slot is Moved.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(runtime): add runtime Env with ownership tracking"
```

---

## Task 22: Pattern Matching (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/pattern.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write tests and implement pattern matching**

```rust
pub fn try_match(pattern: &ast::Pattern, value: &Value) -> Option<Vec<(String, Value)>> { ... }
```

Test cases:
- Wildcard matches anything, binds nothing
- Binding captures the value
- Literal matches exact value
- Variant matches tag and recurses
- Nested patterns

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(runtime): add pattern matching"
```

---

## Task 23: Builtin Functions (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/builtins.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define builtin registry**

```rust
pub type BuiltinFn = fn(&[Value]) -> Result<Value, RuntimeError>;

pub struct Builtins {
    fns: HashMap<String, BuiltinFn>,
}

impl Builtins {
    pub fn new() -> Self {
        let mut b = Self { fns: HashMap::new() };
        b.register_arithmetic();
        b.register_comparison();
        b.register_logic();
        b.register_tensor();
        b.register_collections();
        b.register_utility();
        b
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinFn> { ... }
}
```

- [ ] **Step 2: Implement all builtin categories**

Arithmetic: `+`, `-`, `*`, `/`, `%` — type-dispatch on Int/UInt/Float
Comparison: `=`, `!=`, `<`, `>`, `<=`, `>=` — return Bool
Logic: `and`, `or`, `not`, `xor`
Tensor: delegate to TensorValue methods
Collections: `length`, `at`, `append`, `map`, `filter`, `fold`
Utility: `print`, `assert`, `type-of`, `shape`

- [ ] **Step 3: Write tests for each builtin category**

- [ ] **Step 4: Run tests, commit**

```bash
git commit -m "feat(runtime): add builtin function registry"
```

---

## Task 24: Tree-Walking Evaluator (`airl-runtime`)

**Files:**
- Create: `crates/airl-runtime/src/eval.rs`
- Test: inline `#[cfg(test)]`

This is the core execution engine.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn eval_integer_literal() {
    assert_eq!(eval_str("42"), Value::Int(42));
}

#[test]
fn eval_arithmetic() {
    assert_eq!(eval_str("(+ 1 2)"), Value::Int(3));
}

#[test]
fn eval_let_binding() {
    assert_eq!(eval_str("(let (x : i32 42) x)"), Value::Int(42));
}

#[test]
fn eval_if_true() {
    assert_eq!(eval_str("(if true 1 2)"), Value::Int(1));
}

#[test]
fn eval_if_false() {
    assert_eq!(eval_str("(if false 1 2)"), Value::Int(2));
}

#[test]
fn eval_nested_let() {
    assert_eq!(eval_str("(let (x : i32 1) (y : i32 2) (+ x y))"), Value::Int(3));
}

#[test]
fn eval_do_block() {
    assert_eq!(eval_str("(do 1 2 3)"), Value::Int(3));
}

#[test]
fn eval_match_ok() {
    assert_eq!(eval_str("(match (Ok 42) (Ok v) v (Err e) 0)"), Value::Int(42));
}

#[test]
fn eval_lambda() {
    assert_eq!(eval_str("(let (f : (-> [i32] i32) (fn [x] (+ x 1))) (f 5))"), Value::Int(6));
}

#[test]
fn eval_try_ok() {
    assert_eq!(eval_str("(try (Ok 42))"), Value::Int(42));
}

#[test]
fn eval_defn_and_call() {
    // Tests function definition, contract checking, and call
    let input = r#"
        (defn add-one
          :sig [(x : i32) -> i32]
          :intent "add one"
          :requires [(valid x)]
          :ensures [(= result (+ x 1))]
          :body (+ x 1))
        (add-one 5)
    "#;
    assert_eq!(eval_str(input), Value::Int(6));
}
```

- [ ] **Step 2: Implement the evaluator**

```rust
pub struct Interpreter {
    env: Env,
    builtins: Builtins,
    verify_level: VerifyLevel,
}

impl Interpreter {
    pub fn new() -> Self { ... }

    pub fn eval_top_level(&mut self, top: &ast::TopLevel) -> Result<Value, RuntimeError> { ... }

    pub fn eval(&mut self, expr: &ast::Expr) -> Result<Value, RuntimeError> {
        match &expr.kind {
            ExprKind::IntLit(v) => Ok(Value::Int(*v)),
            ExprKind::FloatLit(v) => Ok(Value::Float(*v)),
            ExprKind::BoolLit(v) => Ok(Value::Bool(*v)),
            ExprKind::StrLit(v) => Ok(Value::Str(v.clone())),
            ExprKind::NilLit => Ok(Value::Nil),
            ExprKind::SymbolRef(name) => self.env.get(name),
            ExprKind::If(cond, then, else_) => { ... }
            ExprKind::Let(bindings, body) => { ... }
            ExprKind::Do(exprs) => { ... }
            ExprKind::Match(scrutinee, arms) => { ... }
            ExprKind::Lambda(params, body) => { ... }
            ExprKind::FnCall(callee, args) => { ... }
            ExprKind::Try(inner) => { ... }
            ExprKind::VariantCtor(name, args) => { ... }
            _ => Err(RuntimeError::Custom(format!("unimplemented: {:?}", expr.kind))),
        }
    }

    fn call_fn(&mut self, fn_def: &FnDef, args: Vec<Value>) -> Result<Value, RuntimeError> {
        // 1. Check :requires contracts
        // 2. Push frame, bind params
        // 3. Eval body
        // 4. Check :ensures contracts (bind `result`)
        // 5. Pop frame, return
        ...
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(runtime): add tree-walking evaluator"
```

---

## Task 25: Update airl-runtime lib.rs and Integration Tests

**Files:**
- Modify: `crates/airl-runtime/src/lib.rs`
- Create: `crates/airl-runtime/tests/integration.rs`

- [ ] **Step 1: Integration tests running full pipeline on AIRL source strings**

Test every spec example: safe-divide, matrix-multiply with contracts, let/match/do, lambdas.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(runtime): finalize airl-runtime with integration tests"
```

---

## Task 26: Agent Identity and Registry (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/identity.rs`
- Create: `crates/airl-agent/src/registry.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define AgentId, Capability, TrustLevel**

Direct from the design spec. Implement `Eq`, `Hash` on `AgentId`.

- [ ] **Step 2: Implement AgentRegistry with capability-based lookup**

```rust
pub struct AgentRegistry {
    agents: HashMap<String, AgentId>,
}

impl AgentRegistry {
    pub fn register(&mut self, agent: AgentId) { ... }
    pub fn lookup(&self, name: &str) -> Option<&AgentId> { ... }
    pub fn find_by_capability(&self, caps: &[Capability]) -> Vec<&AgentId> { ... }
    pub fn find_any(&self, caps: &[Capability]) -> Option<&AgentId> { ... }
}
```

Test: register agents, lookup by name, find by capability, no match returns empty.

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat(agent): add AgentId, Capability, and AgentRegistry"
```

---

## Task 27: Transport Trait and Framing Protocol (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/transport.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Define Transport trait and framing helpers**

```rust
pub trait Transport: Send {
    fn send_message(&mut self, payload: &str) -> Result<(), TransportError>;
    fn recv_message(&mut self) -> Result<String, TransportError>;
    fn close(&mut self) -> Result<(), TransportError>;
}

/// Write a length-prefixed frame: [u32 BE length][UTF-8 payload]
pub fn write_frame(writer: &mut dyn Write, payload: &str) -> io::Result<()> { ... }

/// Read a length-prefixed frame.
pub fn read_frame(reader: &mut dyn Read) -> io::Result<String> { ... }
```

Test framing round-trip with in-memory buffer.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add Transport trait and framing protocol"
```

---

## Task 28: Stdio Transport (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/stdio_transport.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement StdioTransport**

Wraps a `std::process::Child`'s stdin/stdout with the framing protocol.

```rust
pub struct StdioTransport {
    child: Child,
}

impl StdioTransport {
    pub fn spawn(command: &str, args: &[&str]) -> io::Result<Self> { ... }
}

impl Transport for StdioTransport { ... }
```

Test: spawn a simple echo child process, send/receive message.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add StdioTransport"
```

---

## Task 29: TCP Transport (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/tcp_transport.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement TcpTransport**

```rust
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    pub fn connect(addr: SocketAddr) -> io::Result<Self> { ... }
    pub fn from_stream(stream: TcpStream) -> Self { ... }
}

impl Transport for TcpTransport { ... }
```

Test: bind a listener on localhost, connect, send/receive round-trip.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add TcpTransport"
```

---

## Task 30: Unix Socket Transport (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/unix_transport.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement UnixTransport**

Same framing protocol over `std::os::unix::net::UnixStream`.

Test: create temp socket path, bind listener, connect, round-trip.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add UnixTransport"
```

---

## Task 31: Task Runtime and Lifecycle (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/task.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement task execution lifecycle**

```rust
pub struct TaskExecutor {
    interpreter: Interpreter,
    verify_level: VerifyLevel,
}

impl TaskExecutor {
    pub fn execute(&mut self, task: &TaskDef) -> Result<TaskResult, TaskError> {
        // 1. Validate input types
        // 2. Check constraints
        // 3. Start deadline timer
        // 4. Execute body
        // 5. Validate ensures
        // 6. Return result or invoke failure handler
    }
}

pub struct TaskResult {
    pub id: String,
    pub status: TaskStatus,
    pub payload: Option<Value>,
}

pub enum TaskStatus { Complete, Error(String), Timeout }
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add task execution lifecycle"
```

---

## Task 32: Agent Runtime (`airl-agent`)

**Files:**
- Create: `crates/airl-agent/src/runtime.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement AgentRuntime**

```rust
pub struct AgentRuntime {
    identity: AgentId,
    registry: AgentRegistry,
    pending: HashMap<String, PendingTask>,
    interpreter: Interpreter,
    transport: Box<dyn Transport>,
}

impl AgentRuntime {
    pub fn new(identity: AgentId, transport: Box<dyn Transport>) -> Self { ... }
    pub fn run(&mut self) -> Result<(), AgentError> {
        // Main receive loop:
        // 1. Read message from transport
        // 2. Parse as AIRL S-expression
        // 3. If task → execute with TaskExecutor
        // 4. Send result back
    }
    pub fn send_task(&mut self, task: &TaskDef) -> Result<String, AgentError> { ... }
    pub fn await_result(&mut self, task_id: &str, timeout: Duration) -> Result<TaskResult, AgentError> { ... }
}
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(agent): add AgentRuntime with receive loop"
```

---

## Task 33: Agent Builtins Integration (`airl-agent`)

**Files:**
- Modify: `crates/airl-agent/src/runtime.rs` or new `crates/airl-agent/src/builtins.rs`

- [ ] **Step 1: Register agent builtins with the interpreter**

Add `send`, `await`, `spawn-agent`, `parallel`, `broadcast`, `any-agent`, `retry`, `escalate` as builtins that the interpreter can call. These builtins need access to the AgentRuntime, so they're registered as closures or via a trait.

- [ ] **Step 2: Write integration tests**

Test: two agent runtimes communicating via TCP, one sends a task, the other executes and replies.

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat(agent): integrate agent builtins with interpreter"
```

---

## Task 34: CLI and Pipeline (`airl-driver`)

**Files:**
- Modify: `crates/airl-driver/src/main.rs`
- Create: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Implement CLI argument parsing**

Hand-written (no clap dependency):

```rust
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[2..]),
        Some("check") => cmd_check(&args[2..]),
        Some("repl") => cmd_repl(),
        Some("agent") => cmd_agent(&args[2..]),
        Some("fmt") => cmd_fmt(&args[2..]),
        _ => print_usage(),
    }
}
```

- [ ] **Step 2: Implement pipeline orchestration**

```rust
pub fn run_file(path: &str) -> Result<Value, Vec<Diagnostic>> {
    let source = std::fs::read_to_string(path)?;
    let tokens = Lexer::new(&source).lex_all()?;
    let sexprs = parse_sexpr_all(&tokens)?;
    let mut diags = Diagnostics::new();
    let tops = parse_all(&sexprs, &mut diags)?;
    if diags.has_errors() { return Err(diags); }

    let mut checker = TypeChecker::new();
    for top in &tops { checker.check_top_level(top)?; }
    if checker.has_errors() { return Err(checker.into_diagnostics()); }

    let mut interp = Interpreter::new();
    let mut result = Value::Unit;
    for top in &tops { result = interp.eval_top_level(top)?; }
    Ok(result)
}
```

- [ ] **Step 3: Test with a fixture file**

Create `tests/fixtures/valid/arithmetic.airl`:
```clojure
(defn add
  :sig [(a : i32) (b : i32) -> i32]
  :intent "Add two integers"
  :requires [(valid a) (valid b)]
  :ensures [(= result (+ a b))]
  :body (+ a b))

(add 2 3)
```

Run: `cargo run -- run tests/fixtures/valid/arithmetic.airl`
Expected: outputs `5`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(driver): add CLI, pipeline, and run command"
```

---

## Task 35: REPL (`airl-driver`)

**Files:**
- Create: `crates/airl-driver/src/repl.rs`
- Test: inline `#[cfg(test)]` for paren balancing

- [ ] **Step 1: Implement REPL loop**

```rust
pub fn run_repl() {
    let mut interp = Interpreter::new();
    let mut input = String::new();

    loop {
        let prompt = if input.is_empty() { "airl> " } else { "...   " };
        eprint!("{}", prompt);
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap() == 0 { break; }

        let trimmed = line.trim();
        if trimmed == ":quit" { break; }

        input.push_str(&line);
        if !parens_balanced(&input) { continue; }

        // Process input
        match eval_input(&input, &mut interp) {
            Ok(val) => println!("{}", val),
            Err(diag) => eprintln!("{}", format_diagnostic(&diag)),
        }
        input.clear();
    }
}

fn parens_balanced(input: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for ch in input.chars() {
        if in_string {
            if escape { escape = false; continue; }
            if ch == '\\' { escape = true; continue; }
            if ch == '"' { in_string = false; }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
    }
    depth <= 0
}
```

Handle `:type <expr>` (show type without evaluating) and `:env` (show current bindings).

- [ ] **Step 2: Test paren balancing**

```rust
#[test]
fn balanced_simple() { assert!(parens_balanced("(+ 1 2)")); }
#[test]
fn unbalanced_open() { assert!(!parens_balanced("(+ 1")); }
#[test]
fn balanced_nested() { assert!(parens_balanced("(+ (* 2 3) 4)")); }
#[test]
fn string_parens_ignored() { assert!(parens_balanced(r#"(print "(hello")"#)); }
#[test]
fn escaped_quote_in_string() { assert!(parens_balanced(r#"(print "escaped\"paren(")"#)); }
```

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat(driver): add REPL with paren balancing"
```

---

## Task 36: Pretty Printer / Formatter (`airl-driver`)

**Files:**
- Create: `crates/airl-driver/src/fmt.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement S-expression pretty printer**

```rust
pub fn pretty_print(sexpr: &SExpr, indent: usize) -> String { ... }
```

Formats with consistent indentation. Used by `airl fmt`.

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(driver): add S-expression pretty printer"
```

---

## Task 37: Error Formatting (`airl-driver`)

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Implement Rust-style error formatting with span pointing**

```rust
pub fn format_diagnostic(diag: &Diagnostic, source: &str) -> String {
    // error[E0042]: use of moved value `x`
    //   --> example.airl:12:5
    //    |
    // 12 |     (+ x y)
    //    |        ^ value used after move
    ...
}
```

- [ ] **Step 2: Write snapshot tests for error output**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(driver): add Rust-style error formatting"
```

---

## Task 38: Test Fixtures — Valid Programs

**Files:**
- Create: `tests/fixtures/valid/literals.airl`
- Create: `tests/fixtures/valid/arithmetic.airl`
- Create: `tests/fixtures/valid/control_flow.airl`
- Create: `tests/fixtures/valid/types_tensor.airl`
- Create: `tests/fixtures/valid/types_algebraic.airl`
- Create: `tests/fixtures/valid/ownership.airl`
- Create: `tests/fixtures/valid/dependent_dims.airl`
- Create: `tests/fixtures/valid/contracts.airl`
- Create: `tests/fixtures/valid/safe_divide.airl`
- Create: `tests/fixtures/valid/matrix_multiply.airl`
- Create: `tests/fixtures/valid/modules.airl`
- Create: `tests/fixtures/valid/tensor_ops.airl`
- Create: `tests/fixtures/valid/quantifier_contracts.airl`
- Create: `tests/fixtures/valid/try_propagation.airl`
- Create: `tests/fixtures/valid/higher_order.airl`
- Create: `tests/fixtures/valid/module_imports.airl`

- [ ] **Step 1: Write all valid fixtures from spec examples**

Each fixture is a complete AIRL program that should parse, type-check, and execute successfully. Annotate expected output with `;; EXPECT:` comments.

`tests/fixtures/valid/safe_divide.airl`:
```clojure
;; From spec §4.2
;; EXPECT: (Ok 3)

(defn safe-divide
  :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
  :intent "Divide a by b, returning Err on division by zero"
  :requires [(valid a) (valid b)]
  :ensures
    [(match result
       (Ok v)  (= (* v b) a)
       (Err _) (= b 0))]
  :body
    (if (= b 0)
      (Err :division-by-zero)
      (Ok (/ a b))))

(safe-divide 9 3)
```

- [ ] **Step 2: Commit**

```bash
git commit -m "test: add valid AIRL fixture programs from spec"
```

---

## Task 39: Test Fixtures — Error Programs

**Files:**
- Create: `tests/fixtures/type_errors/dim_mismatch.airl`
- Create: `tests/fixtures/type_errors/wrong_arg_type.airl`
- Create: `tests/fixtures/type_errors/missing_contracts.airl`
- Create: `tests/fixtures/contract_errors/precondition.airl`
- Create: `tests/fixtures/contract_errors/postcondition.airl`
- Create: `tests/fixtures/linearity_errors/use_after_move.airl`
- Create: `tests/fixtures/linearity_errors/double_mut_borrow.airl`
- Create: `tests/fixtures/linearity_errors/move_while_borrowed.airl`

- [ ] **Step 1: Write all error fixtures with expected diagnostics**

Each error fixture contains a `;; ERROR:` annotation specifying the expected error message fragment.

`tests/fixtures/linearity_errors/use_after_move.airl`:
```clojure
;; ERROR: use of moved value `x`

(defn consume
  :sig [(own x : i32) -> i32]
  :intent "consume x"
  :requires [(valid x)]
  :ensures [(= result x)]
  :body x)

(let (x : i32 42)
  (do
    (consume x)
    x))  ;; ERROR: x was moved
```

- [ ] **Step 2: Commit**

```bash
git commit -m "test: add error AIRL fixture programs"
```

---

## Task 40: End-to-End Fixture Test Runner

**Files:**
- Create: `tests/e2e/fixture_runner.rs`

- [ ] **Step 1: Write the test harness**

```rust
//! End-to-end test runner for .airl fixture files.
//!
//! Valid fixtures (tests/fixtures/valid/) are expected to parse, type-check,
//! and execute successfully. Expected output is annotated with ;; EXPECT: comments.
//!
//! Error fixtures (tests/fixtures/{type_errors,contract_errors,linearity_errors}/)
//! are expected to produce diagnostics matching ;; ERROR: annotations.

use std::fs;
use std::path::Path;

fn run_fixture(path: &Path) -> (Result<String, Vec<String>>, Vec<String>) {
    let source = fs::read_to_string(path).unwrap();
    let expected = extract_annotations(&source);
    let result = run_pipeline(&source);
    (result, expected)
}

fn extract_annotations(source: &str) -> Vec<String> {
    source.lines()
        .filter_map(|line| {
            if let Some(idx) = line.find(";; EXPECT:") {
                Some(line[idx + 10..].trim().to_string())
            } else if let Some(idx) = line.find(";; ERROR:") {
                Some(line[idx + 9..].trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn valid_fixtures() {
    for entry in fs::read_dir("tests/fixtures/valid").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |e| e == "airl") {
            let (result, expected) = run_fixture(&path);
            assert!(result.is_ok(), "fixture {} should succeed: {:?}", path.display(), result);
            // Check EXPECT annotations if present
        }
    }
}

#[test]
fn type_error_fixtures() {
    for entry in fs::read_dir("tests/fixtures/type_errors").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |e| e == "airl") {
            let (result, expected) = run_fixture(&path);
            assert!(result.is_err(), "fixture {} should fail", path.display());
            // Check ERROR annotations match
        }
    }
}

// Similar for contract_errors, linearity_errors
```

- [ ] **Step 2: Run all fixtures**

Run: `cargo test --test fixture_runner`
Expected: all fixtures pass/fail as expected

- [ ] **Step 3: Commit**

```bash
git commit -m "test: add end-to-end fixture test runner"
```

---

## Task 41: Multi-Agent Integration Tests

**Files:**
- Create: `tests/fixtures/agent/task_roundtrip.airl`
- Create: `tests/fixtures/agent/capability_routing.airl`
- Create: `tests/fixtures/agent/parallel_fanout.airl`
- Create: `tests/fixtures/agent/broadcast.airl`
- Create: `tests/fixtures/agent/await_timeout.airl`
- Create: `tests/e2e/agent_tests.rs`

- [ ] **Step 1: Write multi-agent test**

Spawn two agent processes (using the `airl agent` command), have one send a task to the other via TCP, verify the result comes back correctly with contracts validated.

```rust
#[test]
#[ignore] // slow test, run with --ignored
fn two_agent_task_roundtrip() {
    // 1. Start agent A on port 9001
    // 2. Start agent B on port 9002
    // 3. Agent A sends task to Agent B
    // 4. Agent B executes and returns result
    // 5. Verify result matches expected-output contracts
}
```

- [ ] **Step 2: Write capability routing test**

Register agents with different capabilities, verify routing resolves correctly.

- [ ] **Step 3: Run tests**

Run: `cargo test --test agent_tests -- --ignored`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git commit -m "test: add multi-agent integration tests"
```

---

## Task 42: Final Polish and Full Test Run

**Files:**
- Various: fix any remaining warnings, add missing re-exports

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Verify no `#[allow(unused)]` in production code**

Run: `grep -r "allow(unused)" crates/ --include="*.rs" | grep -v test | grep -v "#[cfg(test)]"`
Expected: no matches

- [ ] **Step 4: Run the REPL manually to verify interactive experience**

Run: `cargo run -- repl`
Type: `(+ 1 2)` → should print `3`
Type: `:quit` → should exit

- [ ] **Step 5: Final commit**

```bash
git commit -m "chore: final polish, all tests pass"
```
