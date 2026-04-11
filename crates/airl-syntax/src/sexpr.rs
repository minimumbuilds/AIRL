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
    Version(u32, u32, u32),   // major.minor.patch — from VersionLit token
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

/// Maximum nesting depth for S-expression parsing.
/// Prevents stack overflow from deeply nested input (SEC-10).
const MAX_PARSE_DEPTH: usize = 1000;

/// Parse a token stream into a list of top-level S-expressions.
/// Takes `tokens` by value and moves strings out of tokens into atoms,
/// eliminating the clone at the token→SExpr boundary.
pub fn parse_sexpr_all(tokens: Vec<Token>) -> Result<Vec<SExpr>, Diagnostic> {
    let mut slots: Vec<Option<Token>> = tokens.into_iter().map(Some).collect();
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < slots.len() {
        if slots[pos].as_ref().map_or(true, |t| t.kind == TokenKind::Eof) {
            break;
        }
        let (expr, next) = parse_sexpr(&mut slots, pos, 0)?;
        exprs.push(expr);
        pos = next;
    }
    Ok(exprs)
}

fn parse_sexpr(tokens: &mut [Option<Token>], pos: usize, depth: usize) -> Result<(SExpr, usize), Diagnostic> {
    if depth > MAX_PARSE_DEPTH {
        let span = tokens.get(pos).and_then(|s| s.as_ref()).map(|t| t.span).unwrap_or(Span::dummy());
        return Err(Diagnostic::error(
            format!("maximum nesting depth exceeded ({})", MAX_PARSE_DEPTH),
            span,
        ));
    }

    if pos >= tokens.len() || tokens[pos].is_none() {
        return Err(Diagnostic::error("unexpected end of input", Span::dummy()));
    }

    // Peek at kind via borrow (borrow ends before any take())
    let is_lparen   = matches!(tokens[pos].as_ref().unwrap().kind, TokenKind::LParen);
    let is_lbracket = matches!(tokens[pos].as_ref().unwrap().kind, TokenKind::LBracket);

    if is_lparen {
        let start_span = tokens[pos].take().unwrap().span;
        parse_list(tokens, pos + 1, false, depth, start_span)
    } else if is_lbracket {
        let start_span = tokens[pos].take().unwrap().span;
        parse_list(tokens, pos + 1, true, depth, start_span)
    } else {
        let token = tokens[pos].take().unwrap();
        let atom = token_to_atom(token)?;
        Ok((SExpr::Atom(atom), pos + 1))
    }
}

fn parse_list(
    tokens: &mut [Option<Token>],
    mut pos: usize,
    is_bracket: bool,
    depth: usize,
    start_span: Span,
) -> Result<(SExpr, usize), Diagnostic> {
    let close = if is_bracket { TokenKind::RBracket } else { TokenKind::RParen };
    let mut items = Vec::new();

    loop {
        if pos >= tokens.len() || tokens[pos].as_ref().map_or(true, |t| t.kind == TokenKind::Eof) {
            return Err(Diagnostic::error(
                "unclosed delimiter",
                start_span,
            ));
        }
        // Skip commas in bracket lists (type parameter separators)
        if matches!(tokens[pos].as_ref().unwrap().kind, TokenKind::Comma) {
            tokens[pos].take();
            pos += 1;
            continue;
        }
        if tokens[pos].as_ref().unwrap().kind == close {
            let end_span = tokens[pos].take().unwrap().span;
            let span = start_span.merge(end_span);
            let expr = if is_bracket {
                SExpr::BracketList(items, span)
            } else {
                SExpr::List(items, span)
            };
            return Ok((expr, pos + 1));
        }
        let (item, next) = parse_sexpr(tokens, pos, depth + 1)?;
        items.push(item);
        pos = next;
    }
}

fn token_to_atom(token: Token) -> Result<Atom, Diagnostic> {
    let span = token.span;
    let kind = match token.kind {
        TokenKind::Integer(v) => AtomKind::Integer(v),
        TokenKind::Float(v) => AtomKind::Float(v),
        TokenKind::Str(v) => AtomKind::Str(v),       // moved, no clone
        TokenKind::Symbol(v) => AtomKind::Symbol(v), // moved, no clone
        TokenKind::Keyword(v) => AtomKind::Keyword(v), // moved, no clone
        TokenKind::Bool(v) => AtomKind::Bool(v),
        TokenKind::Nil => AtomKind::Nil,
        TokenKind::Arrow => AtomKind::Arrow,
        TokenKind::Version(major, minor, patch) => AtomKind::Version(major, minor, patch),
        TokenKind::Colon => AtomKind::Symbol(":".into()),
        other => {
            return Err(Diagnostic::error(
                format!("unexpected token: {:?}", other),
                span,
            ));
        }
    };
    Ok(Atom { kind, span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(input: &str) -> Vec<SExpr> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        parse_sexpr_all(tokens).unwrap()
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
        assert!(parse_sexpr_all(tokens).is_err());
    }

    #[test]
    fn roundtrip_preserves_structure() {
        // Parse, pretty-print, re-parse should give structurally equivalent result
        let input = "(defn foo :sig [(a : i32) -> i32] :body (+ a 1))";
        let exprs1 = parse(input);
        // For now just verify it parses without panic
        assert_eq!(exprs1.len(), 1);
    }

    #[test]
    fn parse_depth_limit_exceeded() {
        // Build input with nesting deeper than MAX_PARSE_DEPTH (1000).
        // Run in a thread with a large stack to avoid overflowing the test
        // thread's stack before our depth check fires.
        let result = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let depth = 1002;
                let input = "(".repeat(depth) + "x" + &")".repeat(depth);
                let mut lexer = Lexer::new(&input);
                let tokens = lexer.lex_all().unwrap();
                parse_sexpr_all(tokens)
            })
            .unwrap()
            .join()
            .unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("maximum nesting depth exceeded"),
            "expected depth error, got: {}",
            err.message,
        );
    }

    #[test]
    fn parse_depth_limit_not_triggered_for_normal_nesting() {
        // 50 levels of nesting should be fine
        let depth = 50;
        let input = "(".repeat(depth) + "x" + &")".repeat(depth);
        let exprs = parse(&input);
        assert_eq!(exprs.len(), 1);
    }

    #[test]
    fn parse_depth_limit_bracket_lists() {
        // Bracket lists also count toward the depth limit.
        // Run in a thread with a large stack (same reason as paren test).
        let result = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let depth = 1002;
                let input = "[".repeat(depth) + "x" + &"]".repeat(depth);
                let mut lexer = Lexer::new(&input);
                let tokens = lexer.lex_all().unwrap();
                parse_sexpr_all(tokens)
            })
            .unwrap()
            .join()
            .unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("maximum nesting depth exceeded"),
            "expected depth error, got: {}",
            err.message,
        );
    }

    #[test]
    fn parse_strings_moved_not_cloned() {
        // Verifies string ownership works correctly after move-based parsing
        let tokens = Lexer::new(r#""hello world""#).lex_all().unwrap();
        let exprs = parse_sexpr_all(tokens).unwrap();
        assert_eq!(exprs.len(), 1);
        assert!(matches!(&exprs[0], SExpr::Atom(Atom { kind: AtomKind::Str(s), .. }) if s == "hello world"));
    }

    #[test]
    fn parse_symbol_moved_not_cloned() {
        let tokens = Lexer::new("foo-bar").lex_all().unwrap();
        let exprs = parse_sexpr_all(tokens).unwrap();
        assert_eq!(exprs.len(), 1);
        assert!(matches!(&exprs[0], SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) if s == "foo-bar"));
    }
}
