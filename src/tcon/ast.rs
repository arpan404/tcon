use std::fmt;

/// Span is a byte-range into the original source string.
/// This is thefoundation for the later error messages like:
/// "error at line/col..." (will map byte offesets to line/col leater).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span{
    pub start: usize,
    pub end: usize, 
}


/// `export const <name> = <expr>`
#[derive(Debug, Clone)]
pub struct ExportConst{
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

/// Expresseion nodes for the TS + Zod like subset tcon supports
#[derive(Debug, Clone)]
pub enum Expr{

}
