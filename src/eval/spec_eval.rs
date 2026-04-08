use crate::model::{Expr, Key, Spec};

pub fn evaluate_spec_expr(expr: &Expr, file_name: &str) -> Result<Spec, String> {
    let Expr::Object(fields, _) = expr else {
        return Err(format!("{file_name}: spec must be an object literal"));
    };

    let mut path: Option<String> = None;
    let mut format: Option<String> = None;
    let mut mode: Option<String> = None;

    for (key, value, _) in fields {
        let key = key_name(key);
        match key.as_str() {
            "path" => path = Some(expect_string(value, file_name, "spec.path")?),
            "format" => format = Some(expect_string(value, file_name, "spec.format")?),
            "mode" => mode = Some(expect_string(value, file_name, "spec.mode")?),
            _ => {}
        }
    }

    let path = path.ok_or_else(|| format!("{file_name}: spec.path is required"))?;
    let format = format.unwrap_or_else(|| "json".to_string());
    Ok(Spec { path, format, mode })
}

fn key_name(k: &Key) -> String {
    match k {
        Key::Ident(s) | Key::String(s) => s.clone(),
    }
}

fn expect_string(expr: &Expr, file_name: &str, field: &str) -> Result<String, String> {
    match expr {
        Expr::String(s, _) => Ok(s.clone()),
        _ => Err(format!("{file_name}: {field} must be a string")),
    }
}
