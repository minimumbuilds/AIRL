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
