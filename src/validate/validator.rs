use crate::model::{Schema, Value};
use std::collections::BTreeMap;

/// Validate `value` against `schema`, collecting ALL errors in one pass.
/// Returns `Err` with every problem joined by newlines so the caller can
/// display them all at once.
pub fn validate(schema: &Schema, value: &Value, file_name: &str) -> Result<Value, String> {
    let mut errors = Vec::new();
    let result = validate_node(schema, Some(value), "config", file_name, &mut errors);
    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors.join("\n"))
    }
}

/// Verify every `.default(...)` on the schema tree satisfies that node's own
/// rules; collects ALL violations in one pass.
pub fn validate_schema_defaults(schema: &Schema, file_name: &str) -> Result<(), String> {
    let mut errors = Vec::new();
    defaults_walk(schema, file_name, "<schema.default>", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

// ─── internals ───────────────────────────────────────────────────────────────

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

fn defaults_walk(schema: &Schema, file_name: &str, path: &str, errors: &mut Vec<String>) {
    if let Some(d) = schema_default(schema) {
        validate_node(schema, Some(d), path, file_name, errors);
    }
    match schema {
        Schema::Object { fields, .. } => {
            for (name, fs) in fields {
                defaults_walk(fs, file_name, &format!("{path}.{name}"), errors);
            }
        }
        Schema::Array { item, .. } => defaults_walk(item, file_name, &format!("{path}[]"), errors),
        Schema::Record { value, .. } => {
            defaults_walk(value, file_name, &format!("{path}.<record>"), errors);
        }
        Schema::Union { variants, .. } => {
            for (i, v) in variants.iter().enumerate() {
                defaults_walk(v, file_name, &format!("{path}|variant{i}"), errors);
            }
        }
        _ => {}
    }
}

/// Core validator.  On error, pushes to `errors` and returns `Value::Null` as
/// a harmless placeholder so sibling fields in the same object can still be
/// validated (multi-error accumulation).
fn validate_node(
    schema: &Schema,
    provided: Option<&Value>,
    path: &str,
    file_name: &str,
    errors: &mut Vec<String>,
) -> Value {
    match schema {
        Schema::String {
            default,
            optional,
            secret: _,
            min,
            max,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required string field is missing");
                    return Value::Null;
                }
            };
            let Value::String(s) = &v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected string, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            if let Some(m) = min
                && (s.chars().count() as f64) < *m
            {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("string shorter than min ({} < {m})", s.chars().count()),
                );
            }
            if let Some(m) = max
                && (s.chars().count() as f64) > *m
            {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("string longer than max ({} > {m})", s.chars().count()),
                );
            }
            Value::String(s.clone())
        }

        Schema::Number {
            default,
            optional,
            secret: _,
            min,
            max,
            int,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required number field is missing");
                    return Value::Null;
                }
            };
            let Value::Number(n) = &v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected number, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            let parsed = match n.parse::<f64>() {
                Ok(f) => f,
                Err(_) => {
                    push_err(
                        errors,
                        file_name,
                        path,
                        &format!("invalid number literal '{n}'"),
                    );
                    return Value::Null;
                }
            };
            if parsed.is_nan() || parsed.is_infinite() {
                push_err(errors, file_name, path, "number must be finite");
                return Value::Null;
            }
            if let Some(m) = min
                && parsed < *m
            {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("number smaller than min ({parsed} < {m})"),
                );
            }
            if let Some(m) = max
                && parsed > *m
            {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("number larger than max ({parsed} > {m})"),
                );
            }
            if *int && parsed.fract() != 0.0 {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected integer number, got {parsed}"),
                );
            }
            Value::Number(n.clone())
        }

        Schema::Boolean { default, optional, secret: _ } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required boolean field is missing");
                    return Value::Null;
                }
            };
            let Value::Bool(b) = v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected boolean, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            Value::Bool(b)
        }

        Schema::Array {
            item,
            default,
            optional,
            secret: _,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required array field is missing");
                    return Value::Null;
                }
            };
            let Value::Array(items) = v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected array, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            // Validate every element — collect ALL element errors.
            let mut out = Vec::with_capacity(items.len());
            for (idx, it) in items.iter().enumerate() {
                out.push(validate_node(
                    item,
                    Some(it),
                    &format!("{path}[{idx}]"),
                    file_name,
                    errors,
                ));
            }
            Value::Array(out)
        }

        Schema::Record {
            value,
            default,
            optional,
            secret: _,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required record field is missing");
                    return Value::Null;
                }
            };
            let Value::Object(obj) = v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected object for record, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            let mut out = BTreeMap::new();
            for (k, v) in &obj {
                let child_path = format!("{path}.{k}");
                out.insert(
                    k.clone(),
                    validate_node(value, Some(v), &child_path, file_name, errors),
                );
            }
            Value::Object(out)
        }

        Schema::Object {
            fields,
            strict,
            default,
            optional,
            secret: _,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required object field is missing");
                    return Value::Null;
                }
            };
            let Value::Object(obj) = v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected object, got {}", type_of(&v)),
                );
                return Value::Null;
            };

            // Unknown-key check (non-fatal: still validate known fields).
            if *strict {
                let mut unknown: Vec<String> = obj
                    .keys()
                    .filter(|k| !fields.contains_key(k.as_str()))
                    .cloned()
                    .collect();
                if !unknown.is_empty() {
                    unknown.sort();
                    let known: Vec<&str> = fields.keys().map(String::as_str).collect();
                    push_err(
                        errors,
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

            // Validate ALL declared fields — do NOT stop on first error.
            let mut out = BTreeMap::new();
            for (name, field_schema) in fields {
                let in_value = obj.get(name);
                let child_path = format!("{path}.{name}");
                let normalized =
                    validate_node(field_schema, in_value, &child_path, file_name, errors);
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

            Value::Object(out)
        }

        Schema::Enum {
            variants,
            default,
            optional,
            secret: _,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required enum field is missing");
                    return Value::Null;
                }
            };
            let Value::String(s) = &v else {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!("expected enum string value, got {}", type_of(&v)),
                );
                return Value::Null;
            };
            if !variants.iter().any(|v| v == s) {
                let allowed = variants
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!(
                        "enum value not in allowed variants: got \"{s}\", allowed: [{allowed}]"
                    ),
                );
            }
            Value::String(s.clone())
        }

        Schema::Literal {
            value: literal,
            default,
            optional,
            secret: _,
        } => {
            let v = match (provided, default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => d.clone(),
                (None, None) if *optional => return Value::Null,
                (None, None) => {
                    push_err(errors, file_name, path, "required literal field is missing");
                    return Value::Null;
                }
            };
            if v != *literal {
                push_err(
                    errors,
                    file_name,
                    path,
                    &format!(
                        "value does not match required literal (expected {}, got {})",
                        fmt_value(literal),
                        fmt_value(&v)
                    ),
                );
            }
            v
        }

        Schema::Union {
            variants,
            default,
            optional,
            secret: _,
        } => {
            let value = match (provided, default) {
                (Some(v), _) => Some(v),
                (None, Some(d)) => Some(d),
                (None, None) if *optional => None,
                (None, None) => {
                    push_err(errors, file_name, path, "required union field is missing");
                    return Value::Null;
                }
            };
            let Some(value) = value else {
                return Value::Null;
            };
            // Try each variant independently; take the first success.
            let mut variant_errors = Vec::new();
            for (i, variant) in variants.iter().enumerate() {
                let mut probe = Vec::new();
                let result = validate_node(variant, Some(value), path, file_name, &mut probe);
                if probe.is_empty() {
                    return result;
                }
                variant_errors.push(format!("variant {i}: {}", probe.join("; ")));
            }
            push_err(
                errors,
                file_name,
                path,
                &format!(
                    "value did not match any union variant ({})",
                    variant_errors.join("; ")
                ),
            );
            Value::Null
        }
    }
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

fn push_err(errors: &mut Vec<String>, file_name: &str, path: &str, msg: &str) {
    errors.push(format!("{file_name}: {path}: {msg}"));
}

/// Check that any schema fields marked `.secret()` are sourced from env-var
/// interpolation (`${VAR_NAME}`) rather than hardcoded literals.
///
/// Called before evaluation with the raw config `Expr` tree so that we can
/// inspect the unresolved source text.
pub fn validate_secret_fields(
    schema: &Schema,
    expr: &crate::model::Expr,
    file_name: &str,
) -> Result<(), String> {
    let mut errors = Vec::new();
    check_secret_expr(schema, expr, "config", file_name, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn contains_unescaped_interpolation(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Escaped interpolation marker: $${ -> literal ${
        if i + 2 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'{'
        {
            i += 3;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            return true;
        }
        i += 1;
    }
    false
}

fn check_secret_expr(
    schema: &Schema,
    expr: &crate::model::Expr,
    path: &str,
    file_name: &str,
    errors: &mut Vec<String>,
) {
    use crate::model::Expr;
    use crate::model::Key;

    if schema.is_secret() {
        match expr {
            Expr::String(s, _) if contains_unescaped_interpolation(s) => {}
            Expr::String(_, _) => errors.push(format!(
                "{file_name}: {path}: secret field must be sourced from an environment variable using ${{VAR_NAME}} interpolation, not a hardcoded literal"
            )),
            _ => errors.push(format!(
                "{file_name}: {path}: secret field must be a string using ${{VAR_NAME}} interpolation"
            )),
        }
        return;
    }

    // Recurse into object fields.
    if let Schema::Object { fields, .. } = schema
        && let Expr::Object(kv_pairs, _) = expr
    {
        for (k, v, _) in kv_pairs {
            let key_name = match k {
                Key::Ident(s) | Key::String(s) => s.as_str(),
            };
            if let Some(field_schema) = fields.get(key_name) {
                check_secret_expr(
                    field_schema,
                    v,
                    &format!("{path}.{key_name}"),
                    file_name,
                    errors,
                );
            }
        }
    }
}
