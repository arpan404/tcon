pub fn format_source_error(file_name: &str, src: &str, offset: usize, message: &str) -> String {
    let clamped = offset.min(src.len());
    let mut line = 1usize;
    let mut col = 1usize;
    let mut line_start = 0usize;

    for (idx, ch) in src.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
            line_start = idx + 1;
        } else {
            col += 1;
        }
    }

    let mut line_end = src.len();
    for (idx, ch) in src[line_start..].char_indices() {
        if ch == '\n' {
            line_end = line_start + idx;
            break;
        }
    }

    let line_text = &src[line_start..line_end];
    let mut out = String::new();
    out.push_str(&format!("error: {message}\n"));
    out.push_str(&format!("  --> {file_name}:{line}:{col}\n"));
    out.push_str(&format!("{line:>4} | {line_text}\n"));
    out.push_str("     | ");
    out.push_str(&" ".repeat(col.saturating_sub(1)));
    out.push('^');
    out
}
