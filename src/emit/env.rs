use crate::model::Value;

pub fn to_env(value: &Value) -> Result<String, String> {
    let mut lines = Vec::new();
    flatten(value, "", &mut lines)?;
    lines.sort();
    Ok(lines.join("\n"))
}

fn flatten(value: &Value, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = normalize_key(k);
                let next = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix}_{key}")
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
                    format!("{prefix}_{idx}")
                };
                flatten(item, &next, out)?;
            }
            Ok(())
        }
        Value::String(s) => {
            out.push(format!("{prefix}={s}"));
            Ok(())
        }
        Value::Number(n) => {
            out.push(format!("{prefix}={n}"));
            Ok(())
        }
        Value::Bool(b) => {
            out.push(format!("{prefix}={b}"));
            Ok(())
        }
        Value::Null => Err(format!("cannot emit null value for env key '{prefix}'")),
    }
}

fn normalize_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    out
}
