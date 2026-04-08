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
        Expr::String(s, _) => Ok(Value::String(s.clone())),
        Expr::Number(s, _) => Ok(Value::Number(s.clone())),
        Expr::Bool(b, _) => Ok(Value::Bool(*b)),
        Expr::Null(_) => Ok(Value::Null),
        _ => Err(format!(
            "{file_name}: config contains unsupported expression; only literal/object/array values are allowed"
        )),
    }
}
