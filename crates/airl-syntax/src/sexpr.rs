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
