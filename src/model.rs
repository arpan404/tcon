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
    pub span: Span,
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
    pub exports: Vec<ExportConst>,
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
        min: Option<f64>,
        max: Option<f64>,
    },
    Number {
        default: Option<Value>,
        optional: bool,
        min: Option<f64>,
        max: Option<f64>,
        int: bool,
    },
    Boolean {
        default: Option<Value>,
        optional: bool,
    },
    Object {
        fields: BTreeMap<String, Schema>,
        strict: bool,
        default: Option<Value>,
        optional: bool,
    },
    Array {
        item: Box<Schema>,
        default: Option<Value>,
        optional: bool,
    },
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
