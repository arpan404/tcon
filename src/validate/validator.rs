use crate::model::{Schema, Value};
use std::collections::BTreeMap;

pub fn validate(schema: &Schema, value: &Value, file_name: &str) -> Result<Value, String> {
    validate_inner(schema, Some(value), "config", file_name)
}

/// Ensure every `.default(...)` on the schema tree satisfies that node’s own rules (strict object keys, min/max, etc.).
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
            let Value::String(s) = v else {
                return err(file_name, path, "expected string");
            };
            if let Some(m) = min
                && (s.chars().count() as f64) < *m
            {
                return err(file_name, path, "string shorter than min");
            }
            if let Some(m) = max
                && (s.chars().count() as f64) > *m
            {
                return err(file_name, path, "string longer than max");
            }
            Ok(Value::String(s))
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
            let Value::Number(n) = v else {
                return err(file_name, path, "expected number");
            };
            let parsed = n
                .parse::<f64>()
                .map_err(|_| format!("{file_name}: {path}: invalid number"))?;
            if let Some(m) = min
                && parsed < *m
            {
                return err(file_name, path, "number smaller than min");
            }
            if let Some(m) = max
                && parsed > *m
            {
                return err(file_name, path, "number larger than max");
            }
            if *int && parsed.fract() != 0.0 {
                return err(file_name, path, "expected integer number");
            }
            Ok(Value::Number(n))
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
                return err(file_name, path, "expected boolean");
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
                return err(file_name, path, "expected array");
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
                return err(file_name, path, "expected object for record");
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
                return err(file_name, path, "expected object");
            };

            if *strict {
                let mut unknown: Vec<String> = obj
                    .keys()
                    .filter(|k| !fields.contains_key(k.as_str()))
                    .cloned()
                    .collect();
                if !unknown.is_empty() {
                    unknown.sort();
                    return err(
                        file_name,
                        path,
                        &format!("unknown key(s) in strict object: {}", unknown.join(", ")),
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
            let Value::String(s) = v else {
                return err(file_name, path, "expected enum string value");
            };
            if !variants.iter().any(|v| v == &s) {
                return err(file_name, path, "enum value not in allowed variants");
            }
            Ok(Value::String(s))
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
                return err(file_name, path, "value does not match required literal");
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

fn err<T>(file_name: &str, path: &str, msg: &str) -> Result<T, String> {
    Err(format!("{file_name}: {path}: {msg}"))
}
