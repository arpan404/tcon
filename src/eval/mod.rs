pub mod config_eval;
pub mod schema_eval;
pub mod spec_eval;

use crate::model::{Expr, Schema, Spec, Value};
use std::collections::BTreeMap;

pub fn evaluate_spec(exports: &BTreeMap<String, Expr>, file_name: &str) -> Result<Spec, String> {
    let expr = exports
        .get("spec")
        .ok_or_else(|| format!("{file_name}: missing required export 'spec'"))?;
    spec_eval::evaluate_spec_expr(expr, file_name)
}

pub fn evaluate_schema(
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Schema, String> {
    let expr = exports
        .get("schema")
        .ok_or_else(|| format!("{file_name}: missing required export 'schema'"))?;
    schema_eval::evaluate_schema_expr(expr, file_name)
}

pub fn evaluate_config(
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Value, String> {
    let expr = exports
        .get("config")
        .ok_or_else(|| format!("{file_name}: missing required export 'config'"))?;
    config_eval::evaluate_config_expr(expr, file_name)
}
