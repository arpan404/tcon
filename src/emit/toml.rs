use crate::model::Value;

pub fn to_toml(value: &Value) -> Result<String, String> {
    let Value::Object(map) = value else {
        return Err("toml emitter requires root object".to_string());
    };
    let mut out = String::new();
    write_table(map, &mut out, &[])?;
    Ok(out.trim_end().to_string())
}

/// Quote a TOML key if it contains characters outside the bare-key alphabet `[A-Za-z0-9_-]`.
fn toml_quote_key(k: &str) -> String {
    let is_bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if is_bare {
        k.to_string()
    } else {
        format!("\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn write_table(
    map: &std::collections::BTreeMap<String, Value>,
    out: &mut String,
    path: &[String],
) -> Result<(), String> {
    let mut nested: Vec<(&String, &Value)> = Vec::new();
    for (k, v) in map {
        match v {
            Value::Object(_) => nested.push((k, v)),
            _ => out.push_str(&format!(
                "{} = {}\n",
                toml_quote_key(k),
                scalar_or_inline(v)?
            )),
        }
    }
    for (k, v) in nested {
        let Value::Object(child) = v else {
            continue;
        };
        let mut child_path = path.to_vec();
        child_path.push(toml_quote_key(k));
        let header = child_path.join(".");
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{header}]\n"));
        write_table(child, out, &child_path)?;
    }
    Ok(())
}

fn scalar_or_inline(v: &Value) -> Result<String, String> {
    match v {
        Value::Null => Err("toml emitter cannot represent null".to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.clone()),
        Value::String(s) => Ok(format!(
            "\"{}\"",
            s.replace('\\', "\\\\").replace('"', "\\\"")
        )),
        Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(scalar_or_inline(item)?);
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for (k, v) in map {
                parts.push(format!("{} = {}", toml_quote_key(k), scalar_or_inline(v)?));
            }
            Ok(format!("{{ {} }}", parts.join(", ")))
        }
    }
}
