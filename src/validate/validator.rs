use crate::model::{Schema, Value};
use std::collections::BTreeMap;

pub fn validate(schema: &Schema, value: &Value, file_name: &str) -> Result<Value, String> {
    validate_inner(schema, Some(value), "config", file_name)
}

/// Ensure every `.default(...)` on the schema tree satisfies that node's own rules (strict object keys, min/max, etc.).
pub fn validate_schema_defaults(schema: &Schema, file_name: &str) -> Result<(), String> {
    defaults_walk(schema, file_name, "<schema.default>")
}

fn schema_default(schema: &Schema) -> Option<&Value> {
    match schema {
        Schema::String { default, .. }
        | Schema::Number { default, .. }
        | Schema::Boolean { default, .. }
        | Schema::Object { default, .. }
        | Schema::Array { default, .. }
        | Schema::Record { default, .. }
        | Schema::Literal { default, .. }
        | Schema::Enum { default, .. }
        | Schema::Union { default, .. } => default.as_ref(),
    }
}

fn defaults_walk(schema: &Schema, file_name: &str, path: &str) -> Result<(), String> {
    if let Some(d) = schema_default(schema) {
        validate_inner(schema, Some(d), path, file_name).map(|_| ())?;
    }
    match schema {
        Schema::Object { fields, .. } => {
            for (name, fs) in fields {
                defaults_walk(fs, file_name, &format!("{path}.{name}"))?;
            }
        }
        Schema::Array { item, .. } => defaults_walk(item, file_name, &format!("{path}[]"))?,
        Schema::Record { value, .. } => {
            defaults_walk(value, file_name, &format!("{path}.<record>"))?;
        }
        Schema::Union { variants, .. } => {
            for (i, v) in variants.iter().enumerate() {
                defaults_walk(v, file_name, &format!("{path}|variant{i}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

fn validate_inner(
    schema: &Schema,
    provided: Option<&Value>,
    path: &str,
    file_name: &str,
) -> Result<Value, String> {
    match schema {
        Schema::String {
            default,
            optional,
            min,
            max,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required string field is missing"),
            };
            let Value::String(s) = &v else {
                return err(
                    file_name,
                    path,
                    &format!("expected string, got {}", type_of(&v)),
                );
            };
            if let Some(m) = min
                && (s.chars().count() as f64) < *m
            {
                return err(
                    file_name,
                    path,
                    &format!("string shorter than min ({} < {m})", s.chars().count()),
                );
            }
            if let Some(m) = max
                && (s.chars().count() as f64) > *m
            {
                return err(
                    file_name,
                    path,
                    &format!("string longer than max ({} > {m})", s.chars().count()),
                );
            }
            Ok(Value::String(s.clone()))
        }
        Schema::Number {
            default,
            optional,
            min,
            max,
            int,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required number field is missing"),
            };
            let Value::Number(n) = &v else {
                return err(
                    file_name,
                    path,
                    &format!("expected number, got {}", type_of(&v)),
                );
            };
            let parsed = n
                .parse::<f64>()
                .map_err(|_| format!("{file_name}: {path}: invalid number '{n}'"))?;
            if parsed.is_nan() || parsed.is_infinite() {
                return err(file_name, path, "number must be finite");
            }
            if let Some(m) = min
                && parsed < *m
            {
                return err(
                    file_name,
                    path,
                    &format!("number smaller than min ({parsed} < {m})"),
                );
            }
            if let Some(m) = max
                && parsed > *m
            {
                return err(
                    file_name,
                    path,
                    &format!("number larger than max ({parsed} > {m})"),
                );
            }
            if *int && parsed.fract() != 0.0 {
                return err(
                    file_name,
                    path,
                    &format!("expected integer number, got {parsed}"),
                );
            }
            Ok(Value::Number(n.clone()))
        }
        Schema::Boolean { default, optional } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => {
                    return err(file_name, path, "required boolean field is missing");
                }
            };
            let Value::Bool(b) = v else {
                return err(
                    file_name,
                    path,
                    &format!("expected boolean, got {}", type_of(&v)),
                );
            };
            Ok(Value::Bool(b))
        }
        Schema::Array {
            item,
            default,
            optional,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required array field is missing"),
            };
            let Value::Array(items) = v else {
                return err(
                    file_name,
                    path,
                    &format!("expected array, got {}", type_of(&v)),
                );
            };
            let mut out = Vec::with_capacity(items.len());
            for (idx, it) in items.iter().enumerate() {
                out.push(validate_inner(
                    item,
                    Some(it),
                    &format!("{path}[{idx}]"),
                    file_name,
                )?);
            }
            Ok(Value::Array(out))
        }
        Schema::Record {
            value,
            default,
            optional,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required record field is missing"),
            };
            let Value::Object(obj) = v else {
                return err(
                    file_name,
                    path,
                    &format!("expected object for record, got {}", type_of(&v)),
                );
            };
            let mut out = BTreeMap::new();
            for (k, v) in &obj {
                let child_path = format!("{path}.{k}");
                out.insert(
                    k.clone(),
                    validate_inner(value, Some(v), &child_path, file_name)?,
                );
            }
            Ok(Value::Object(out))
        }
        Schema::Object {
            fields,
            strict,
            default,
            optional,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required object field is missing"),
            };
            let Value::Object(obj) = v else {
                return err(
                    file_name,
                    path,
                    &format!("expected object, got {}", type_of(&v)),
                );
            };

            if *strict {
                let mut unknown: Vec<String> = obj
                    .keys()
                    .filter(|k| !fields.contains_key(k.as_str()))
                    .cloned()
                    .collect();
                if !unknown.is_empty() {
                    unknown.sort();
                    let known: Vec<&str> = fields.keys().map(String::as_str).collect();
                    return err(
                        file_name,
                        path,
                        &format!(
                            "unknown key(s) in strict object: {} (known: {})",
                            unknown.join(", "),
                            if known.is_empty() {
                                "<none>".to_string()
                            } else {
                                known.join(", ")
                            }
                        ),
                    );
                }
            }

            let mut out = BTreeMap::new();
            for (name, field_schema) in fields {
                let in_value = obj.get(name);
                let child_path = format!("{path}.{name}");
                let normalized = validate_inner(field_schema, in_value, &child_path, file_name)?;
                if normalized != Value::Null {
                    out.insert(name.clone(), normalized);
                }
            }

            if !*strict {
                for (k, v) in &obj {
                    if !fields.contains_key(k) {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }

            Ok(Value::Object(out))
        }
        Schema::Enum {
            variants,
            default,
            optional,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required enum field is missing"),
            };
            let Value::String(s) = &v else {
                return err(
                    file_name,
                    path,
                    &format!("expected enum string value, got {}", type_of(&v)),
                );
            };
            if !variants.iter().any(|v| v == s) {
                let allowed = variants
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                return err(
                    file_name,
                    path,
                    &format!(
                        "enum value not in allowed variants: got \"{s}\", allowed: [{allowed}]"
                    ),
                );
            }
            Ok(Value::String(s.clone()))
        }
        Schema::Literal {
            value: literal,
            default,
            optional,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Ok(Value::Null),
                (None, None) => return err(file_name, path, "required literal field is missing"),
            };
            if v != *literal {
                return err(
                    file_name,
                    path,
                    &format!(
                        "value does not match required literal (expected {}, got {})",
                        fmt_value(literal),
                        fmt_value(&v)
                    ),
                );
            }
            Ok(v)
        }
        Schema::Union {
            variants,
            default,
            optional,
        } => {
            let value = match (provided, default) {
                (Some(v), _) => Some(v),
                (None, Some(d)) => Some(d),
                (None, None) if *optional => None,
                (None, None) => return err(file_name, path, "required union field is missing"),
            };
            let Some(value) = value else {
                return Ok(Value::Null);
            };
            let mut errors = Vec::new();
            for (i, variant) in variants.iter().enumerate() {
                match validate_inner(variant, Some(value), path, file_name) {
                    Ok(v) => return Ok(v),
                    Err(e) => errors.push(format!("variant {i}: {e}")),
                }
            }
            Err(format!(
                "{file_name}: {path}: value did not match any union variant ({})",
                errors.join("; ")
            ))
        }
    }
}

fn fmt_value(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.clone(),
        Value::String(s) => format!("\"{s}\""),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

fn err<T>(file_name: &str, path: &str, msg: &str) -> Result<T, String> {
    Err(format!("{file_name}: {path}: {msg}"))
}
