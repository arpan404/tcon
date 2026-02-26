//! ast.rs
//!
//! This is the TS-like AST for tcon language.
//! This layer must be kept purely syntactic.
//!
//! The parser should ONLY produce these nodes.
//! Zod-like meaning are handled in `schema.rs` and `lower.rs`

use crate::span::Span;
use std::fmt;

/// One `export const <name> = <expr>;` statement
#[derive(Debug, Clone)]
pub struct ExportConst {
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

/// Expression nodes for our TS subset
#[derive(Debug, Clone)]
pub enum Expr {
    /// Keep spans per property as well for better error messages

    /// Object literal: {key: value, ...}
    Object(Vec<(Key, Expr, Span)>, Span),

    /// Array literal: [a,b, c]
    Array(Vec<(Expr, Span)>, Span),

    /// Literal values
    String(String, Span),

    /// Keep the original numeric lexeme as a string for determinstic printing.
    Number(String, Span),

    Bool(bool, Span),
    Null(Span),

    /// Identifiers, e.g. t, object, schema
    Ident(String, Span),

    /// Member access, e.g. t.object
    Member(Box<Expr>, String, Span),

    /// Call expression, e.g. t.object({}) or schema.min(1)
    Call(Box<Expr>, Vec<Expr>, Span),

    /// Parentheses grouping. Optional but useful for spans and tooling.
    Paren(Box<Expr>, Span),

    /// `undefined` literal.
    Undefined(Span),
}

/// Object keys can be identifiers or string
#[derive(Debug, Clone)]
pub enum Key {
    Ident(String),
    String(String),
}

impl Expr {
    /// Get span for any expression
    pub fn span(&self) -> Span {
        match self {
            Expr::Object(_, s)
            | Expr::Array(_, s)
            | Expr::Null(s)
            | Expr::Member(_, _, s)
            | Expr::Call(_, _, s)
            | Expr::Paren(_, s)
            | Expr::Undefined(s) => *s,
            Expr::String(_, s) | Expr::Number(_, s) | Expr::Bool(_, s) | Expr::Ident(_, s) => *s,
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
