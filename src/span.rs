use std::fmt;

/// Span is a byte-rage into the oriignal source string
/// `start` and `end` are byte offeserts (end is exclusive).
/// Usually used to map the compile issues in the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    /// A zero-length span at a single offset
    pub fn at(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Ensure span is non-empty for underline UX.
    pub fn non_empty(self) -> Self {
        // here, we take the ownership, and return it back after processing
        if self.start == self.end {
            Self {
                start: self.start,
                end: self.end.saturating_add(1),
            }
        } else {
            self
        }
    }

    impl fmt::Display for Span {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
            /// use simple formatting here, diagnostics part will handle mapping to line and column
            write!(f, "[{}]..{}]", self.start, self.end)
        }
    }
}
