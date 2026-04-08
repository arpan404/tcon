use crate::model::Value;

pub fn to_toml(value: &Value) -> Result<String, String> {
    let Value::Object(map) = value else {
        return Err("toml emitter requires root object".to_string());
    };
    let mut out = String::new();
    write_table(map, &mut out, "")?;
    Ok(out.trim_end().to_string())
}

fn write_table(
    map: &std::collections::BTreeMap<String, Value>,
    out: &mut String,
    prefix: &str,
) -> Result<(), String> {
    let mut nested = Vec::new();
    for (k, v) in map {
        match v {
            Value::Object(_) => nested.push((k.clone(), v)),
            _ => out.push_str(&format!("{k} = {}\n", scalar_or_inline(v)?)),
        }
    }
    for (k, v) in nested {
        let Value::Object(child) = v else {
            continue;
        };
        let name = if prefix.is_empty() {
            k
        } else {
            format!("{prefix}.{k}")
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{name}]\n"));
        write_table(child, out, &name)?;
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
                parts.push(format!("{k} = {}", scalar_or_inline(v)?));
            }
            Ok(format!("{{ {} }}", parts.join(", ")))
        }
    }
}
