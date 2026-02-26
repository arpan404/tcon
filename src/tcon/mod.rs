/// Compiler front-end modules:
///
/// ast -> the parsed AST data structure
/// lexer -> turns raw code into tokens
/// parser -> tuens tokens into AST
/// loader -> reads a file and produces a parsed unit (exports map)

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod loader;

