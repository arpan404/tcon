//! schema.rs
//!
//! Zod-like types

use crate::span::Span;
#[derive(Debug, Clone)]
pub enum Schema {
    // primitives
    String {
        span: Span,
    },
    Number {
        span: Span,
    },
    Bool {
        span: Span,
    },
    Null {
        span: Span,
    },
    Unknown {
        span: Span,
    },

    // structured
    Object {
        field: Vec<Field>,
        span: Span,
    },
    Array {
        item: Box<Schema>,
        span: Span,
    },
    Tuple {
        item: Box<Schema>,
        span: Span,
    },
    Record {
        key: Box<Schema>,
        value: Box<Schema>,
        span: Span,
    },

    // literals / enums
    Literal {
        value: LiteralValue,
        span: Span,
    },
    Enum {
        value: Vec<String>,
        span: Span,
    },
    // combinators
    Union {
        variants: Vec<Schema>,
        span: Span,
    },
    Intersection {
        parts: Vec<Schema>,
        span: Span,
    },

    // modifiers
    Options {
        inner: Box<Schema>,
        span: Span,
    },
    Nullable {
        inner: Box<Schema>,
        span: Span,
    },
    Default {
        inner: Box<Schema>,
        value: ConstValue,
        span: Span,
    },

    // constraints/effects
    Refine {
        inner: Box<Schema>,
        pred: Predicate,
        span: Span,
    },
}

/// An object field
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub schema: Schema,
    pub optional: bool,
    pub span: Span,
}

/// Literal values `t.literal(...)`
#[derive(Debug, Clone)]
pub enum LiteralValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
}

/// JSON-ish const values for `.default(...)`
#[derive(Debug, Clone)]
pub enum ConstValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
    Array(Vec<ConstValue>),
    Object(Vec<(String, ConstValue)>),
}

/// Safe predicates so no arbitary code execution in `.tcon`
#[derive(Debug, Clone)]
pub enum Predicate {
    MinNumber { n: String },
    MaxNumber { n: String },

    MinLength { n: String },
    MaxLength { n: String },

    Regex { pattern: String },
    Email,
}

impl Schema {
    /// Span for any schema node
    pub fn span(&self) -> Span {
        match self {
            Schema::String { span }
            | Schema::Number { span }
            | Schema::Unknown { span }
            | Schema::Bool { span }
            | Schema::Null { span }
            | Schema::Objct { span, .. }
            | Schema::Array { span, .. }
            | Schema::Tuple { span, .. }
            | Schema::Record { span, .. }
            | Schema::Literal { span, .. }
            | Schema::Enum { span, .. }
            | Schema::Union { span, .. }
            | Schema::Intersection { span, .. }
            | Schema::Optional { span, .. }
            | Schema::Nullable { span, .. }
            | Schema::Default { span, .. }
            | Schema::Refine { span, .. } => *span,
        }
    }
}
