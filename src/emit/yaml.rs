use crate::model::Value;

pub fn to_yaml(value: &Value) -> String {
    render(value, 0)
}

fn render(value: &Value, depth: usize) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.clone(),
        Value::String(s) => quote_string(s),
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let indent = "  ".repeat(depth);
            let next_indent = "  ".repeat(depth + 1);
            let mut out = String::new();
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        out.push_str(&format!("{indent}-\n"));
                        out.push_str(&next_indent);
                        out.push_str(
                            &render(item, depth + 1).replace('\n', &format!("\n{next_indent}")),
                        );
                    }
                    _ => {
                        out.push_str(&format!("{indent}- {}", render(item, depth + 1)));
                    }
                }
                if i + 1 < items.len() {
                    out.push('\n');
                }
            }
            out
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let indent = "  ".repeat(depth);
            let next_indent = "  ".repeat(depth + 1);
            let mut out = String::new();
            let len = map.len();
            for (i, (k, v)) in map.iter().enumerate() {
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        out.push_str(&format!("{indent}{k}:\n"));
                        out.push_str(&next_indent);
                        out.push_str(
                            &render(v, depth + 1).replace('\n', &format!("\n{next_indent}")),
                        );
                    }
                    _ => out.push_str(&format!("{indent}{k}: {}", render(v, depth + 1))),
                }
                if i + 1 < len {
                    out.push('\n');
                }
            }
            out
        }
    }
}

fn quote_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
