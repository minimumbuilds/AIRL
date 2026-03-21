use airl_syntax::sexpr::{SExpr, Atom, AtomKind};

pub fn pretty_print(sexpr: &SExpr) -> String {
    pretty_print_inner(sexpr, 0)
}

fn pretty_print_inner(sexpr: &SExpr, indent: usize) -> String {
    match sexpr {
        SExpr::Atom(a) => format_atom(a),
        SExpr::List(items, _) => {
            if items.is_empty() {
                return "()".to_string();
            }
            let parts: Vec<String> = items
                .iter()
                .map(|s| pretty_print_inner(s, indent + 2))
                .collect();
            let one_line = format!("({})", parts.join(" "));
            if one_line.len() <= 80 {
                one_line
            } else {
                let indent_str = " ".repeat(indent + 1);
                format!("({})", parts.join(&format!("\n{}", indent_str)))
            }
        }
        SExpr::BracketList(items, _) => {
            let parts: Vec<String> = items
                .iter()
                .map(|s| pretty_print_inner(s, indent + 1))
                .collect();
            format!("[{}]", parts.join(" "))
        }
    }
}

fn format_atom(atom: &Atom) -> String {
    match &atom.kind {
        AtomKind::Integer(v) => v.to_string(),
        AtomKind::Float(v) => format!("{}", v),
        AtomKind::Str(v) => format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")),
        AtomKind::Symbol(v) => v.clone(),
        AtomKind::Keyword(v) => format!(":{}", v),
        AtomKind::Bool(true) => "true".to_string(),
        AtomKind::Bool(false) => "false".to_string(),
        AtomKind::Nil => "nil".to_string(),
        AtomKind::Arrow => "->".to_string(),
    }
}

/// Format a source string by parsing to S-expressions and pretty-printing.
pub fn format_source(source: &str) -> Result<String, String> {
    let mut lexer = airl_syntax::Lexer::new(source);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens).map_err(|d| d.message)?;
    let mut output = String::new();
    for (i, sexpr) in sexprs.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&pretty_print(&sexpr));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::Span;

    fn atom(kind: AtomKind) -> SExpr {
        SExpr::Atom(Atom {
            kind,
            span: Span::dummy(),
        })
    }

    fn list(items: Vec<SExpr>) -> SExpr {
        SExpr::List(items, Span::dummy())
    }

    fn bracket_list(items: Vec<SExpr>) -> SExpr {
        SExpr::BracketList(items, Span::dummy())
    }

    #[test]
    fn print_integer() {
        let s = atom(AtomKind::Integer(42));
        assert_eq!(pretty_print(&s), "42");
    }

    #[test]
    fn print_symbol() {
        let s = atom(AtomKind::Symbol("hello".to_string()));
        assert_eq!(pretty_print(&s), "hello");
    }

    #[test]
    fn print_string() {
        let s = atom(AtomKind::Str("world".to_string()));
        assert_eq!(pretty_print(&s), "\"world\"");
    }

    #[test]
    fn print_keyword() {
        let s = atom(AtomKind::Keyword("name".to_string()));
        assert_eq!(pretty_print(&s), ":name");
    }

    #[test]
    fn print_bool() {
        assert_eq!(pretty_print(&atom(AtomKind::Bool(true))), "true");
        assert_eq!(pretty_print(&atom(AtomKind::Bool(false))), "false");
    }

    #[test]
    fn print_nil() {
        assert_eq!(pretty_print(&atom(AtomKind::Nil)), "nil");
    }

    #[test]
    fn print_arrow() {
        assert_eq!(pretty_print(&atom(AtomKind::Arrow)), "->");
    }

    #[test]
    fn print_empty_list() {
        assert_eq!(pretty_print(&list(vec![])), "()");
    }

    #[test]
    fn print_simple_list() {
        let s = list(vec![
            atom(AtomKind::Symbol("+".to_string())),
            atom(AtomKind::Integer(1)),
            atom(AtomKind::Integer(2)),
        ]);
        assert_eq!(pretty_print(&s), "(+ 1 2)");
    }

    #[test]
    fn print_nested_list() {
        let inner = list(vec![
            atom(AtomKind::Symbol("*".to_string())),
            atom(AtomKind::Integer(2)),
            atom(AtomKind::Integer(3)),
        ]);
        let outer = list(vec![
            atom(AtomKind::Symbol("+".to_string())),
            inner,
            atom(AtomKind::Integer(4)),
        ]);
        assert_eq!(pretty_print(&outer), "(+ (* 2 3) 4)");
    }

    #[test]
    fn print_bracket_list() {
        let s = bracket_list(vec![
            atom(AtomKind::Integer(1)),
            atom(AtomKind::Integer(2)),
            atom(AtomKind::Integer(3)),
        ]);
        assert_eq!(pretty_print(&s), "[1 2 3]");
    }

    #[test]
    fn print_string_with_quotes() {
        let s = atom(AtomKind::Str("say \"hi\"".to_string()));
        assert_eq!(pretty_print(&s), r#""say \"hi\"""#);
    }

    #[test]
    fn format_source_roundtrip() {
        let source = "(+ 1 2)";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "(+ 1 2)");
    }

    #[test]
    fn format_source_multiple_exprs() {
        let source = "(+ 1 2)\n(* 3 4)";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "(+ 1 2)\n(* 3 4)");
    }

    #[test]
    fn long_list_wraps() {
        // Build a list long enough to exceed 80 chars
        let mut items = vec![atom(AtomKind::Symbol("some-very-long-function-name".to_string()))];
        for i in 0..10 {
            items.push(atom(AtomKind::Symbol(format!("argument-number-{}", i))));
        }
        let s = list(items);
        let output = pretty_print(&s);
        assert!(output.contains('\n'), "expected multiline output for long list");
    }
}
