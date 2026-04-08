pub fn describe_drift(actual: &str, expected: &str) -> String {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    let max = a.len().max(e.len());
    for i in 0..max {
        let al = a.get(i).copied().unwrap_or("<missing>");
        let el = e.get(i).copied().unwrap_or("<missing>");
        if al != el {
            return format!(
                "  first difference at line {}\n  actual:   {}\n  expected: {}",
                i + 1,
                al,
                el
            );
        }
    }
    "  content differs".to_string()
}
