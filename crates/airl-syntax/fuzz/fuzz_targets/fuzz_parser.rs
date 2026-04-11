#![no_main]
use libfuzzer_sys::fuzz_target;
use airl_syntax::diagnostic::Diagnostics;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let tokens = match airl_syntax::lexer::Lexer::new(s).lex_all() {
            Ok(t) => t,
            Err(_) => return,
        };
        let sexprs = match airl_syntax::sexpr::parse_sexpr_all(tokens) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut diags = Diagnostics::new();
        for sexpr in &sexprs {
            let _ = airl_syntax::parser::parse_top_level(sexpr, &mut diags);
        }
    }
});
