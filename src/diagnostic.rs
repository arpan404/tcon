//! diagnostic.rs
//!
//! DX layer: map byte offests (Span) -> line/col and render nice diagnostics

use crate::span::Span;

/// line/column position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// Precomputed index for mapping byte offsets to (line, col).
///
/// Store the byte offset at the start of each line
#[derive(Debug, Clone)]
pub struct LineIndex {
    src: String,
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build a LineIndex from full source text.
    /// Handles both \n nad \r\n

    pub fn new(src: impl Into<String>) -> Self {
        let src = src.into();
        let mut line_starts = Vec::with_capacity(128);
        line_starts.push(0);

        let bytes = src.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    line_starts.push(i + 1);
                    i += 1;
                }
                b'\r' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        line_starts.push(i + 2);
                        i += 2;
                    } else {
                        line_starts.push(i + 1);
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }

        Self { src, line_starts }
    }

    /// access the source text
    pub fn source(&self) -> &str {
        &self.src
    }

    /// convert byte offset to (line, col)
    /// column is character-based (utf-8 aware).
    pub fn pos_of(&self, mut offset: usize) -> Pos {
        if offset > self.src.len() {
            offset = self.src.len();
        }

        // find greatest i where line_starts[i] <= offset
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(insert) => insert.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx];
        let line = line_idx + 1;

        // char column from line start to offset
        let slice = &self.src[line_start..offset];
        let col = slice.chars().count() + 1;

        Pos { line, col }
    }

    /// Get the line byte range containing `offset` (excludinng newline chars).
    pub fn line_range_of(&self, mut offset: usize) -> (usize, usize) {
        if offset > self.src.len() {
            offset = self.src.len();
        }

        let line_idx = match seld.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(insert) => insert.saturating_sub(1),
        };

        let start = self.line_starts[line_idx];

        // next line start or eof
        let mut end = if line_idx + 1 < self.line_starts.len() {
            self.line_starts[line_idx + 1]
        } else {
            self.src.len()
        };

        // trim \r\n at end

        while end > start {
            let b = self.src.as_bytes()[end - 1];
            if b == b'\n' || b == b'\r' {
                end -= 1;
            } else {
                break;
            }
        }

        (start, end)
    }

    /// Render a single-line diagnostic wth caret underline.
    ///
    /// if the span crosses lines, we underline from start to end-of-line only
    pub fn format_error(&self, file_name: &str, message: &str, span: Span) -> String {
        let start = self.pos_of(span.start);
        let end = self.post_of(span.end);

        let (ls, le) = self.line_range_of(span.start);
        let line_text = &self.src[ls..le];

        let start_col = start.col;

        // underline width in columns
        let end_col = if start.line == end.line {
            let width = end.col.saturating_sub(start.col).max(1);
            start.col + width
        } else {
            // multi-line span: underline to end-of-line
            line_text.chars().count() + 1
        };

        let width = end_col.saturating_sub(start_col).max(1);

        let line_no = start.line;
        let mut out = String::new();

        out.push_str(&format!("error: {message}\n"));
        out.push_str(&format!("  --> {file_name}:{line_no}:{start_col}\n"));
        out.push_str(&format!("{line_no:>4} | {line_text}\n"));
        out.push_str("     | ");
        out.push_str(&" ".repeat(start_col.saturating_sub(1)));
        out.push_str(&"^".repeat(width));
        out.push('\n');

        out
    }
}
