use crate::eval::config_eval::evaluate_config_expr;
use crate::model::{Expr, Key, Schema, Value};
use std::collections::BTreeMap;

pub fn evaluate_schema_expr(
    expr: &Expr,
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Schema, String> {
    eval_chain(expr, exports, file_name)
}

/// Recursively evaluate a schema expression.
///
/// The `exports` map is threaded through so that bare identifiers like
/// `baseSchema` (imported from another file or defined locally) can be
/// resolved as schema values — enabling patterns like `.extend(baseSchema)`.
fn eval_chain(
    expr: &Expr,
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Schema, String> {
    match expr {
        // Identifier → resolve from exports (enables reuse / extend)
        Expr::Ident(name, _) => {
            let resolved = exports
                .get(name.as_str())
                .ok_or_else(|| format!("{file_name}: undefined schema identifier '{name}'"))?;
            eval_chain(resolved, exports, file_name)
        }
        Expr::Call(callee, args, _) => match callee.as_ref() {
            Expr::Member(base, method, _) => {
                if let Expr::Ident(root, _) = base.as_ref()
                    && root == "t"
                {
                    build_base(method, args, exports, file_name)
                } else {
                    let mut schema = eval_chain(base, exports, file_name)?;
                    apply_method(&mut schema, method, args, exports, file_name)?;
                    Ok(schema)
                }
            }
            _ => Err(format!(
                "{file_name}: unsupported schema expression; expected method call"
            )),
        },
        _ => Err(format!(
            "{file_name}: schema must be built from t.*() call chains"
        )),
    }
}

fn build_base(
    name: &str,
    args: &[Expr],
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<Schema, String> {
    match name {
        "string" => Ok(Schema::String {
            default: None,
            optional: false,
            secret: false,
            min: None,
            max: None,
        }),
        "number" => Ok(Schema::Number {
            default: None,
            optional: false,
            secret: false,
            min: None,
            max: None,
            int: false,
        }),
        "boolean" | "bool" => Ok(Schema::Boolean {
            default: None,
            optional: false,
            secret: false,
        }),
        "object" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.object() expects one argument"));
            }
            let Expr::Object(fields, _) = &args[0] else {
                return Err(format!(
                    "{file_name}: t.object() argument must be an object"
                ));
            };
            let mut out = BTreeMap::new();
            for (k, v, _) in fields {
                let name = match k {
                    Key::Ident(s) | Key::String(s) => s.clone(),
                };
                out.insert(name, eval_chain(v, exports, file_name)?);
            }
            Ok(Schema::Object {
                fields: out,
                strict: false,
                default: None,
                optional: false,
                secret: false,
            })
        }
        "array" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.array() expects one argument"));
            }
            Ok(Schema::Array {
                item: Box::new(eval_chain(&args[0], exports, file_name)?),
                default: None,
                optional: false,
                secret: false,
            })
        }
        "record" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.record() expects one argument"));
            }
            Ok(Schema::Record {
                value: Box::new(eval_chain(&args[0], exports, file_name)?),
                default: None,
                optional: false,
                secret: false,
            })
        }
        "literal" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.literal() expects one argument"));
            }
            let value = evaluate_config_expr(&args[0], file_name)?;
            match value {
                Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {
                    Ok(Schema::Literal {
                        value,
                        default: None,
                        optional: false,
                        secret: false,
                    })
                }
                _ => Err(format!(
                    "{file_name}: t.literal() only supports primitive literal values"
                )),
            }
        }
        "enum" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.enum() expects one argument"));
            }
            let Expr::Array(items, _) = &args[0] else {
                return Err(format!("{file_name}: t.enum() expects an array of strings"));
            };
            let mut variants = Vec::with_capacity(items.len());
            for (item, _) in items {
                let Expr::String(s, _) = item else {
                    return Err(format!(
                        "{file_name}: t.enum() only supports string variants"
                    ));
                };
                variants.push(s.clone());
            }
            if variants.is_empty() {
                return Err(format!(
                    "{file_name}: t.enum() requires at least one variant"
                ));
            }
            let mut seen_v = std::collections::BTreeSet::new();
            for s in &variants {
                if !seen_v.insert(s.as_str()) {
                    return Err(format!("{file_name}: t.enum() has duplicate variant '{s}'"));
                }
            }
            Ok(Schema::Enum {
                variants,
                default: None,
                optional: false,
                secret: false,
            })
        }
        "union" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: t.union() expects one argument"));
            }
            let Expr::Array(items, _) = &args[0] else {
                return Err(format!(
                    "{file_name}: t.union() expects an array of schemas"
                ));
            };
            let mut variants = Vec::with_capacity(items.len());
            for (item, _) in items {
                variants.push(eval_chain(item, exports, file_name)?);
            }
            if variants.len() < 2 {
                return Err(format!(
                    "{file_name}: t.union() requires at least two schema variants"
                ));
            }
            Ok(Schema::Union {
                variants,
                default: None,
                optional: false,
                secret: false,
            })
        }
        _ => Err(format!(
            "{file_name}: unsupported schema root constructor t.{name}()"
        )),
    }
}

fn apply_method(
    schema: &mut Schema,
    method: &str,
    args: &[Expr],
    exports: &BTreeMap<String, Expr>,
    file_name: &str,
) -> Result<(), String> {
    match method {
        "optional" => {
            if !args.is_empty() {
                return Err(format!("{file_name}: .optional() does not take arguments"));
            }
            set_optional(schema, true);
            Ok(())
        }
        "default" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: .default() expects one argument"));
            }
            let value = evaluate_config_expr(&args[0], file_name)?;
            set_default(schema, value);
            Ok(())
        }
        "min" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: .min() expects one argument"));
            }
            let n = parse_number_literal(&args[0], file_name, ".min()")?;
            match schema {
                Schema::String { min, max, .. } => {
                    if let Some(mx) = *max
                        && n > mx
                    {
                        return Err(format!(
                            "{file_name}: .min({n}) > .max({mx}) — bounds are inverted"
                        ));
                    }
                    *min = Some(n);
                    Ok(())
                }
                Schema::Number { min, max, .. } => {
                    if let Some(mx) = *max
                        && n > mx
                    {
                        return Err(format!(
                            "{file_name}: .min({n}) > .max({mx}) — bounds are inverted"
                        ));
                    }
                    *min = Some(n);
                    Ok(())
                }
                _ => Err(format!(
                    "{file_name}: .min() not supported for this schema type"
                )),
            }
        }
        "max" => {
            if args.len() != 1 {
                return Err(format!("{file_name}: .max() expects one argument"));
            }
            let n = parse_number_literal(&args[0], file_name, ".max()")?;
            match schema {
                Schema::String { max, min, .. } => {
                    if let Some(mn) = *min
                        && n < mn
                    {
                        return Err(format!(
                            "{file_name}: .max({n}) < .min({mn}) — bounds are inverted"
                        ));
                    }
                    *max = Some(n);
                    Ok(())
                }
                Schema::Number { max, min, .. } => {
                    if let Some(mn) = *min
                        && n < mn
                    {
                        return Err(format!(
                            "{file_name}: .max({n}) < .min({mn}) — bounds are inverted"
                        ));
                    }
                    *max = Some(n);
                    Ok(())
                }
                _ => Err(format!(
                    "{file_name}: .max() not supported for this schema type"
                )),
            }
        }
        "int" => match schema {
            Schema::Number { int, .. } => {
                *int = true;
                Ok(())
            }
            _ => Err(format!("{file_name}: .int() only valid on number schema")),
        },
        "strict" => match schema {
            Schema::Object { strict, .. } => {
                *strict = true;
                Ok(())
            }
            _ => Err(format!(
                "{file_name}: .strict() only valid on object schema"
            )),
        },
        "extend" => {
            if args.len() != 1 {
                return Err(format!(
                    "{file_name}: .extend() expects exactly one object-schema argument"
                ));
            }
            let ext = eval_chain(&args[0], exports, file_name)?;
            let Schema::Object {
                fields: base_fields,
                ..
            } = schema
            else {
                return Err(format!(
                    "{file_name}: .extend() is only valid on an object schema"
                ));
            };
            let Schema::Object {
                fields: ext_fields, ..
            } = ext
            else {
                return Err(format!(
                    "{file_name}: .extend() argument must be an object schema"
                ));
            };
            // Fields already declared in the child take precedence; the base
            // fills in any keys that are absent.
            for (k, v) in ext_fields {
                base_fields.entry(k).or_insert(v);
            }
            Ok(())
        }
        "secret" => {
            if !args.is_empty() {
                return Err(format!("{file_name}: .secret() does not take arguments"));
            }
            match schema {
                Schema::String { .. } => {
                    set_secret(schema, true);
                    Ok(())
                }
                _ => Err(format!(
                    "{file_name}: .secret() only valid on string schema"
                )),
            }
        }
        _ => Err(format!(
            "{file_name}: unsupported schema method .{method}()"
        )),
    }
}

fn set_secret(schema: &mut Schema, secret: bool) {
    match schema {
        Schema::String { secret: s, .. }
        | Schema::Number { secret: s, .. }
        | Schema::Boolean { secret: s, .. }
        | Schema::Object { secret: s, .. }
        | Schema::Array { secret: s, .. }
        | Schema::Record { secret: s, .. }
        | Schema::Literal { secret: s, .. }
        | Schema::Enum { secret: s, .. }
        | Schema::Union { secret: s, .. } => *s = secret,
    }
}

fn parse_number_literal(expr: &Expr, file_name: &str, name: &str) -> Result<f64, String> {
    match expr {
        Expr::Number(s, _) => s
            .parse::<f64>()
            .map_err(|_| format!("{file_name}: {name} expects a numeric literal")),
        _ => Err(format!("{file_name}: {name} expects a numeric literal")),
    }
}

fn set_optional(schema: &mut Schema, optional: bool) {
    match schema {
        Schema::String { optional: o, .. }
        | Schema::Number { optional: o, .. }
        | Schema::Boolean { optional: o, .. }
        | Schema::Object { optional: o, .. }
        | Schema::Array { optional: o, .. }
        | Schema::Record { optional: o, .. }
        | Schema::Literal { optional: o, .. }
        | Schema::Enum { optional: o, .. }
        | Schema::Union { optional: o, .. } => *o = optional,
    }
}

fn set_default(schema: &mut Schema, value: Value) {
    match schema {
        Schema::String { default, .. }
        | Schema::Number { default, .. }
        | Schema::Boolean { default, .. }
        | Schema::Object { default, .. }
        | Schema::Array { default, .. }
        | Schema::Record { default, .. }
        | Schema::Literal { default, .. }
        | Schema::Enum { default, .. }
        | Schema::Union { default, .. } => *default = Some(value),
    }
}
