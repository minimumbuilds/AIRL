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
            col: if self.line < other.line { self.col }
                 else if self.line > other.line { other.col }
                 else { self.col.min(other.col) },
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
