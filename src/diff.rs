/// Unified diff text; set `color` to paint `-` / `+` lines (respect `NO_COLOR` / TTY in the caller).
pub fn describe_drift(actual: &str, expected: &str, color: bool) -> String {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    let (head_a, head_e, sep, minus, plus, trunc, reset) = if color {
        (
            "\x1b[2m  --- actual\x1b[0m\n",
            "\x1b[2m  +++ expected\x1b[0m\n",
            "\x1b[2m",
            "\x1b[31m",
            "\x1b[32m",
            "\x1b[2m  ... truncated ...\x1b[0m\n",
            "\x1b[0m",
        )
    } else {
        (
            "  --- actual\n",
            "  +++ expected\n",
            "",
            "",
            "",
            "  ... truncated ...\n",
            "",
        )
    };

    let mut out = String::new();
    out.push_str(head_a);
    out.push_str(head_e);

    let mut i = 0usize;
    let mut j = 0usize;
    let mut hunks = 0usize;
    while (i < a.len() || j < e.len()) && hunks < 8 {
        if i < a.len() && j < e.len() && a[i] == e[j] {
            i += 1;
            j += 1;
            continue;
        }

        hunks += 1;
        out.push_str(&format!(
            "{sep}  @@ -{}, +{} @@{reset}\n",
            i + 1,
            j + 1,
            sep = sep,
            reset = reset
        ));

        let mut printed = 0usize;
        while (i < a.len() || j < e.len()) && printed < 4 {
            if i < a.len() && j < e.len() && a[i] == e[j] {
                break;
            }
            if i + 1 < a.len() && j < e.len() && a[i + 1] == e[j] {
                out.push_str(&format!("  {minus}-{reset}{}\n", a[i], minus = minus, reset = reset));
                i += 1;
                printed += 1;
                continue;
            }
            if j + 1 < e.len() && i < a.len() && a[i] == e[j + 1] {
                out.push_str(&format!("  {plus}+{reset}{}\n", e[j], plus = plus, reset = reset));
                j += 1;
                printed += 1;
                continue;
            }
            if i < a.len() {
                out.push_str(&format!("  {minus}-{reset}{}\n", a[i], minus = minus, reset = reset));
                i += 1;
                printed += 1;
            }
            if j < e.len() && printed < 4 {
                out.push_str(&format!("  {plus}+{reset}{}\n", e[j], plus = plus, reset = reset));
                j += 1;
                printed += 1;
            }
        }
        if i < a.len() && j < e.len() && a[i] == e[j] {
            if color {
                out.push_str(&format!("   \x1b[2m{}\x1b[0m\n", a[i]));
            } else {
                out.push_str(&format!("   {}\n", a[i]));
            }
            i += 1;
            j += 1;
        }
    }
    if hunks == 0 {
        return if color {
            "\x1b[2m  content differs\x1b[0m".to_string()
        } else {
            "  content differs".to_string()
        };
    }
    if i < a.len() || j < e.len() {
        out.push_str(trunc);
    }
    out.trim_end().to_string()
}
