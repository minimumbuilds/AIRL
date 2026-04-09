use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::diagnostic::Diagnostic;

const MAX_TOKEN_COUNT: usize = 10_000_000;
const MAX_STRING_LENGTH: usize = 10_000_000;

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
            let tok_span = tok.span;
            tokens.push(tok);
            if tokens.len() >= MAX_TOKEN_COUNT {
                return Err(Diagnostic::error(
                    format!("token limit exceeded ({})", MAX_TOKEN_COUNT),
                    tok_span,
                ));
            }
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
            b'-' if self.peek_at(1) == Some(b'>') && self.peek_at(2) == Some(b'>') => {
                self.advance();
                self.advance();
                self.advance();
                TokenKind::Symbol("->>".to_string())
            }
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
            if value.len() >= MAX_STRING_LENGTH {
                return Err(Diagnostic::error(
                    format!("string literal exceeds maximum length ({})", MAX_STRING_LENGTH),
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
                    if other < 0x80 {
                        value.push(other as char);
                        self.advance();
                    } else {
                        // Multi-byte UTF-8: use Rust's chars() iterator for safe decoding.
                        // from_utf8 validates the byte sequence; chars().next() is then safe
                        // because we check for Some rather than unwrapping.
                        let seq_len = if other & 0xE0 == 0xC0 { 2 }
                            else if other & 0xF0 == 0xE0 { 3 }
                            else if other & 0xF8 == 0xF0 { 4 }
                            else { 1 };
                        let end = (self.pos + seq_len).min(self.source.len());
                        match std::str::from_utf8(&self.source[self.pos..end]) {
                            Ok(s) => {
                                match s.chars().next() {
                                    Some(ch) => {
                                        value.push(ch);
                                        for _ in 0..ch.len_utf8() {
                                            self.advance();
                                        }
                                    }
                                    None => {
                                        // Empty slice after from_utf8 — treat as invalid byte.
                                        return Err(Diagnostic::error(
                                            format!("invalid UTF-8 byte: 0x{:02X}", other),
                                            Span::new(self.pos, self.pos + 1, self.line, self.col),
                                        ));
                                    }
                                }
                            }
                            Err(_) => {
                                return Err(Diagnostic::error(
                                    format!("invalid UTF-8 byte: 0x{:02X}", other),
                                    Span::new(self.pos, self.pos + 1, self.line, self.col),
                                ));
                            }
                        }
                    }
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
                    self.advance(); self.advance();
                    let hex_start = self.pos;
                    while self.pos < self.source.len() && self.source[self.pos].is_ascii_hexdigit() {
                        self.advance();
                    }
                    let hex_str = std::str::from_utf8(&self.source[hex_start..self.pos])
                        .unwrap_or(""); // slice contains only ASCII hex digits; UTF-8 always valid here
                    let val = i64::from_str_radix(hex_str, 16).map_err(|_| {
                        Diagnostic::error("invalid hex literal", Span::new(start, self.pos, start_line, start_col))
                    })?;
                    return Ok(TokenKind::Integer(if negative { -val } else { val }));
                }
                b'b' | b'B' => {
                    self.advance(); self.advance();
                    let bin_start = self.pos;
                    while self.pos < self.source.len() && (self.source[self.pos] == b'0' || self.source[self.pos] == b'1') {
                        self.advance();
                    }
                    let bin_str = std::str::from_utf8(&self.source[bin_start..self.pos])
                        .unwrap_or(""); // slice contains only ASCII binary digits; UTF-8 always valid here
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

        if self.pos < self.source.len() && self.source[self.pos] == b'.'
            && self.peek_at(1).map_or(false, |c| c.is_ascii_digit())
        {
            is_float = true;
            self.advance();
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.advance();
            }
        }

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

        if self.pos < self.source.len() && self.source[self.pos] == b'f' {
            is_float = true;
            self.advance();
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.advance();
            }
        }

        let text = std::str::from_utf8(&self.source[start..self.pos])
            .unwrap_or(""); // slice contains only ASCII digits/dots; UTF-8 always valid here
        if is_float {
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
            let name = std::str::from_utf8(&self.source[start..self.pos])
                .unwrap_or("") // slice contains only ASCII symbol chars; UTF-8 always valid here
                .to_string();
            TokenKind::Keyword(name)
        } else {
            TokenKind::Colon
        }
    }

    fn lex_symbol(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.source.len() && is_symbol_char(self.source[self.pos]) {
            if self.source[self.pos] == b':' {
                if self.peek_at(1).map_or(false, |c| is_symbol_start(c)) {
                    self.advance();
                    continue;
                } else {
                    break;
                }
            }
            self.advance();
        }
        let text = std::str::from_utf8(&self.source[start..self.pos])
            .unwrap_or(""); // slice contains only ASCII symbol chars; UTF-8 always valid here
        match text {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "nil" => TokenKind::Nil,
            _ => TokenKind::Symbol(text.to_string()),
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), Diagnostic> {
        loop {
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                self.advance();
            }

            if self.pos >= self.source.len() {
                break;
            }

            if self.source[self.pos] == b';' {
                while self.pos < self.source.len() && self.source[self.pos] != b'\n' {
                    self.advance();
                }
                continue;
            }

            if self.pos + 1 < self.source.len() && self.source[self.pos] == b'#' && self.source[self.pos + 1] == b'|' {
                let start = self.pos;
                let start_line = self.line;
                let start_col = self.col;
                self.advance(); self.advance();
                let mut depth = 1u32;
                while self.pos < self.source.len() && depth > 0 {
                    if self.pos + 1 < self.source.len() && self.source[self.pos] == b'#' && self.source[self.pos + 1] == b'|' {
                        depth += 1;
                        self.advance(); self.advance();
                    } else if self.pos + 1 < self.source.len() && self.source[self.pos] == b'|' && self.source[self.pos + 1] == b'#' {
                        depth -= 1;
                        self.advance(); self.advance();
                    } else {
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
        assert_eq!(lex("agent:orchestrator"), vec![TokenKind::Symbol("agent:orchestrator".into())]);
    }

    #[test]
    fn lex_error_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        assert!(lexer.lex_all().is_err());
    }

    // ── Safety / bounds tests ─────────────────────────────

    #[test]
    fn lex_utf8_multibyte_in_string() {
        // Two-byte sequence (U+00E9 = é)
        let input = "\"\u{00E9}\"";
        assert_eq!(lex(input), vec![TokenKind::Str("\u{00E9}".into())]);
    }

    #[test]
    fn lex_utf8_three_byte_in_string() {
        // Three-byte sequence (U+4E2D = 中)
        let input = "\"\u{4E2D}\"";
        assert_eq!(lex(input), vec![TokenKind::Str("\u{4E2D}".into())]);
    }

    #[test]
    fn lex_utf8_four_byte_in_string() {
        // Four-byte sequence (U+1F600 = 😀)
        let input = "\"\u{1F600}\"";
        assert_eq!(lex(input), vec![TokenKind::Str("\u{1F600}".into())]);
    }

    #[test]
    fn lex_invalid_utf8_byte_in_string_is_error() {
        // 0xFF is not a valid UTF-8 start byte — should produce an error, not panic.
        let raw: Vec<u8> = vec![b'"', 0xFF, b'"'];
        let s = unsafe { std::str::from_utf8_unchecked(&raw) };
        let mut lexer = Lexer::new(s);
        assert!(lexer.lex_all().is_err());
    }

    #[test]
    fn lex_hex_literal_safety() {
        // Ensure hex parsing works and does not panic
        assert_eq!(lex("0x0"), vec![TokenKind::Integer(0)]);
        assert_eq!(lex("0xDEADBEEF"), vec![TokenKind::Integer(0xDEAD_BEEF)]);
    }

    #[test]
    fn lex_binary_literal_safety() {
        assert_eq!(lex("0b0"), vec![TokenKind::Integer(0)]);
        assert_eq!(lex("0b11111111"), vec![TokenKind::Integer(255)]);
    }

    #[test]
    fn lex_error_unexpected_char() {
        // A bare non-symbol byte (like 0x01) outside a string should produce an error.
        let raw: Vec<u8> = vec![0x01];
        let s = unsafe { std::str::from_utf8_unchecked(&raw) };
        let mut lexer = Lexer::new(s);
        assert!(lexer.lex_all().is_err());
    }
}
