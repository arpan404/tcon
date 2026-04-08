pub mod config_eval;
pub mod schema_eval;
pub mod spec_eval;

use crate::model::{Expr, Schema, Spec, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub fn evaluate_spec(exports: &BTreeMap<String, Expr>, file_name: &str) -> Result<Spec, String> {
    let expr = resolve_named_export(exports, "spec", file_name)?;
    spec_eval::evaluate_spec_expr(expr, file_name)
}

pub fn evaluate_schema(
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Schema, String> {
    let expr = resolve_named_export(exports, "schema", file_name)?;
    schema_eval::evaluate_schema_expr(expr, exports, file_name)
}

pub fn evaluate_config(exports: &BTreeMap<String, Expr>, file_name: &str) -> Result<Value, String> {
    let expr = resolve_named_export(exports, "config", file_name)?;
    config_eval::evaluate_config_expr(expr, file_name)
}

fn resolve_named_export<'a>(
    exports: &'a BTreeMap<String, Expr>,
    name: &str,
    file_name: &str,
) -> Result<&'a Expr, String> {
    let mut seen = BTreeSet::new();
    let mut cur = exports
        .get(name)
        .ok_or_else(|| format!("{file_name}: missing required export '{name}'"))?;
    loop {
        match cur {
            Expr::Ident(next, _) => {
                if !seen.insert(next.clone()) {
                    return Err(format!(
                        "{file_name}: circular identifier reference while resolving '{}'",
                        name
                    ));
                }
                cur = exports.get(next).ok_or_else(|| {
                    format!(
                        "{file_name}: unresolved identifier '{}' while resolving '{}'",
                        next, name
                    )
                })?;
            }
            _ => return Ok(cur),
        }
    }
}

/// Return the resolved (identifier-followed) raw config `Expr` without
/// performing any env-var interpolation.  Used by the secret-field pre-check.
pub fn raw_config_expr<'a>(
    exports: &'a BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<&'a Expr, String> {
    resolve_named_export(exports, "config", file_name)
}
