pub struct TrustedVerifier;

impl TrustedVerifier {
    pub fn note(&self, fn_name: &str) -> String {
        format!("note: trusting contracts for `{}`", fn_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_produces_correct_string() {
        let verifier = TrustedVerifier;
        assert_eq!(
            verifier.note("my_function"),
            "note: trusting contracts for `my_function`"
        );
    }

    #[test]
    fn note_with_empty_name() {
        let verifier = TrustedVerifier;
        assert_eq!(
            verifier.note(""),
            "note: trusting contracts for ``"
        );
    }

    #[test]
    fn note_with_complex_name() {
        let verifier = TrustedVerifier;
        let note = verifier.note("module::inner::my_fn");
        assert!(note.contains("module::inner::my_fn"));
        assert!(note.starts_with("note: trusting contracts for"));
    }

    #[test]
    fn note_format_matches_spec() {
        let verifier = TrustedVerifier;
        let note = verifier.note("foo");
        assert_eq!(note, "note: trusting contracts for `foo`");
    }
}
