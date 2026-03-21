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
    Keyword(String),     // colon prefix stripped: ":sig" → "sig"

    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,

    // Special
    Colon,
    Arrow,               // ->
    Comma,

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
