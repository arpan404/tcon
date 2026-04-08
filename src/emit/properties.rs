use crate::model::Value;

pub fn to_properties(value: &Value) -> Result<String, String> {
    let mut lines = Vec::new();
    flatten(value, "", &mut lines)?;
    lines.sort();
    Ok(lines.join("\n"))
}

fn flatten(value: &Value, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(v, &next, out)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                let next = if prefix.is_empty() {
                    idx.to_string()
                } else {
                    format!("{prefix}.{idx}")
                };
                flatten(item, &next, out)?;
            }
            Ok(())
        }
        Value::Null => Err(format!(
            "properties emitter cannot represent null at '{prefix}'"
        )),
        Value::Bool(b) => {
            out.push(format!("{prefix}={b}"));
            Ok(())
        }
        Value::Number(n) => {
            out.push(format!("{prefix}={n}"));
            Ok(())
        }
        Value::String(s) => {
            out.push(format!("{prefix}={}", escape_value(s)));
            Ok(())
        }
    }
}

fn escape_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('=', "\\=")
}
