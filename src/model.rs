use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone)]
pub struct ExportConst {
    pub name: String,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Object(Vec<(Key, Expr, Span)>, Span),
    Array(Vec<(Expr, Span)>, Span),
    String(String, Span),
    Number(String, Span),
    Bool(bool, Span),
    Null(Span),
    Ident(String, Span),
    Member(Box<Expr>, String, Span),
    Call(Box<Expr>, Vec<Expr>, Span),
}

#[derive(Debug, Clone)]
pub enum Key {
    Ident(String),
    String(String),
}

#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<ImportStmt>,
    pub exports: Vec<ExportConst>,
}

#[derive(Debug, Clone)]
pub struct ImportStmt {
    pub names: Vec<String>,
    pub from: String,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub path: String,
    pub format: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Schema {
    String {
        default: Option<Value>,
        optional: bool,
        secret: bool,
        min: Option<f64>,
        max: Option<f64>,
    },
    Number {
        default: Option<Value>,
        optional: bool,
        secret: bool,
        min: Option<f64>,
        max: Option<f64>,
        int: bool,
    },
    Boolean {
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Object {
        fields: BTreeMap<String, Schema>,
        strict: bool,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Array {
        item: Box<Schema>,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Record {
        value: Box<Schema>,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Literal {
        value: Value,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Enum {
        variants: Vec<String>,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
    Union {
        variants: Vec<Schema>,
        default: Option<Value>,
        optional: bool,
        secret: bool,
    },
}

impl Schema {
    pub fn is_secret(&self) -> bool {
        match self {
            Schema::String { secret, .. }
            | Schema::Number { secret, .. }
            | Schema::Boolean { secret, .. }
            | Schema::Object { secret, .. }
            | Schema::Array { secret, .. }
            | Schema::Record { secret, .. }
            | Schema::Literal { secret, .. }
            | Schema::Enum { secret, .. }
            | Schema::Union { secret, .. } => *secret,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(String),
    Bool(bool),
    Null,
}
