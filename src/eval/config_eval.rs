use crate::model::{Expr, Key, Value};
use std::collections::BTreeMap;

pub fn evaluate_config_expr(expr: &Expr, file_name: &str) -> Result<Value, String> {
    eval_value(expr, file_name)
}

fn eval_value(expr: &Expr, file_name: &str) -> Result<Value, String> {
    match expr {
        Expr::Object(fields, _) => {
            let mut out = BTreeMap::new();
            for (key, value, _) in fields {
                let name = match key {
                    Key::Ident(s) | Key::String(s) => s.clone(),
                };
                out.insert(name, eval_value(value, file_name)?);
            }
            Ok(Value::Object(out))
        }
        Expr::Array(items, _) => {
            let mut out = Vec::with_capacity(items.len());
            for (item, _) in items {
                out.push(eval_value(item, file_name)?);
            }
            Ok(Value::Array(out))
        }
        Expr::String(s, _) => {
            if s.contains("${") {
                Ok(Value::String(interpolate_env(s, file_name)?))
            } else {
                Ok(Value::String(s.clone()))
            }
        }
        Expr::Number(s, _) => Ok(Value::Number(s.clone())),
        Expr::Bool(b, _) => Ok(Value::Bool(*b)),
        Expr::Null(_) => Ok(Value::Null),
        _ => Err(format!(
            "{file_name}: config contains unsupported expression; only literal/object/array values are allowed"
        )),
    }
}

/// Expand `${VAR_NAME}` and `${VAR_NAME:default}` placeholders in `s` using
/// the process environment.
///
/// - `${DB_HOST}` — substituted with the value of `DB_HOST`; fails if unset.
/// - `${DB_HOST:localhost}` — substituted with `DB_HOST`, or `"localhost"` if
///   the variable is unset or empty.
/// - `${DB_HOST:}` — substituted with `DB_HOST`, or `""` if unset.
///
/// Nested `${...}` is not supported.  A literal `${` can be escaped as `$${`.
pub fn interpolate_env(s: &str, file_name: &str) -> Result<String, String> {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Escaped: $${ → literal ${
        if i + 2 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'{'
        {
            result.push_str("${");
            i += 3;
            continue;
        }
        // Interpolation start
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let start = i;
            i += 2;
            let inner_start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i >= bytes.len() {
                return Err(format!(
                    "{file_name}: unterminated env interpolation starting at byte {start}"
                ));
            }
            let inner = &s[inner_start..i];
            i += 1; // consume '}'

            let (var_name, default_val) = if let Some(colon) = inner.find(':') {
                (&inner[..colon], Some(&inner[colon + 1..]))
            } else {
                (inner, None)
            };

            if var_name.is_empty() {
                return Err(format!(
                    "{file_name}: env interpolation contains empty variable name in \"${{{inner}}}\""
                ));
            }
            if !var_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(format!(
                    "{file_name}: env variable name contains invalid characters: \"{var_name}\" \
                     (only A-Z, a-z, 0-9, _ are allowed)"
                ));
            }

            match std::env::var(var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => match default_val {
                    Some(d) => result.push_str(d),
                    None => {
                        return Err(format!(
                            "{file_name}: env variable '{var_name}' is not set and has no default \
                             (use ${{{}:fallback}} to provide one)",
                            var_name
                        ));
                    }
                },
            }
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    Ok(result)
}
