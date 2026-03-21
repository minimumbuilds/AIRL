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
