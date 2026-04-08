pub fn describe_drift(actual: &str, expected: &str) -> String {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    let mut out = String::new();
    out.push_str("  --- actual\n");
    out.push_str("  +++ expected\n");

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
        out.push_str(&format!("  @@ -{}, +{} @@\n", i + 1, j + 1));

        let mut printed = 0usize;
        while (i < a.len() || j < e.len()) && printed < 4 {
            if i < a.len() && j < e.len() && a[i] == e[j] {
                break;
            }
            if i + 1 < a.len() && j < e.len() && a[i + 1] == e[j] {
                out.push_str(&format!("  -{}\n", a[i]));
                i += 1;
                printed += 1;
                continue;
            }
            if j + 1 < e.len() && i < a.len() && a[i] == e[j + 1] {
                out.push_str(&format!("  +{}\n", e[j]));
                j += 1;
                printed += 1;
                continue;
            }
            if i < a.len() {
                out.push_str(&format!("  -{}\n", a[i]));
                i += 1;
                printed += 1;
            }
            if j < e.len() && printed < 4 {
                out.push_str(&format!("  +{}\n", e[j]));
                j += 1;
                printed += 1;
            }
        }
        if i < a.len() && j < e.len() && a[i] == e[j] {
            out.push_str(&format!("   {}\n", a[i]));
            i += 1;
            j += 1;
        }
    }
    if hunks == 0 {
        return "  content differs".to_string();
    }
    if i < a.len() || j < e.len() {
        out.push_str("  ... truncated ...\n");
    }
    out.trim_end().to_string()
}
