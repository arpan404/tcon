use crate::model::Value;

pub fn to_pretty_json(value: &Value) -> String {
    render(value, 0)
}

fn render(value: &Value, depth: usize) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.clone(),
        Value::String(s) => format!("\"{}\"", escape_json(s)),
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let indent = "  ".repeat(depth + 1);
            let close_indent = "  ".repeat(depth);
            let mut out = String::from("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&indent);
                out.push_str(&render(item, depth + 1));
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&close_indent);
            out.push(']');
            out
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let indent = "  ".repeat(depth + 1);
            let close_indent = "  ".repeat(depth);
            let mut out = String::from("{\n");
            let len = map.len();
            for (idx, (k, v)) in map.iter().enumerate() {
                out.push_str(&indent);
                out.push('"');
                out.push_str(&escape_json(k));
                out.push_str("\": ");
                out.push_str(&render(v, depth + 1));
                if idx + 1 < len {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&close_indent);
            out.push('}');
            out
        }
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
