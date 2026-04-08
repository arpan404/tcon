use crate::model::Value;
use std::collections::BTreeMap;

pub fn to_env(value: &Value) -> Result<String, String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    flatten(value, "", &mut pairs)?;
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    detect_key_collision(&pairs)?;
    let lines: Vec<String> = pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    Ok(lines.join("\n"))
}

/// Fail if two different config paths normalize to the same env key.
fn detect_key_collision(pairs: &[(String, String)]) -> Result<(), String> {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    for (k, _) in pairs {
        if seen.contains_key(k.as_str()) {
            return Err(format!(
                "env key collision: multiple config fields normalize to the same env variable '{k}' \
                 (e.g. keys like 'a-b' and 'a_b' both become 'A_B')"
            ));
        }
        seen.insert(k.as_str(), k.as_str());
    }
    Ok(())
}

fn flatten(value: &Value, prefix: &str, out: &mut Vec<(String, String)>) -> Result<(), String> {
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
            out.push((prefix.to_string(), s.clone()));
            Ok(())
        }
        Value::Number(n) => {
            out.push((prefix.to_string(), n.clone()));
            Ok(())
        }
        Value::Bool(b) => {
            out.push((prefix.to_string(), b.to_string()));
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
