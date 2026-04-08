use crate::model::Program;
use crate::tcon::lexer::lex;
use crate::tcon::parser::parse;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

pub fn load_program(path: &PathBuf) -> Result<(BTreeMap<String, crate::model::Expr>, String), String> {
    let file_name = path.display().to_string();
    let src = fs::read_to_string(path).map_err(|e| format!("{}: failed to read: {}", file_name, e))?;
    let tokens = lex(&src, &file_name)?;
    let program = parse(&tokens, &file_name)?;
    exports_map(program, &file_name)
}

fn exports_map(program: Program, file_name: &str) -> Result<(BTreeMap<String, crate::model::Expr>, String), String> {
    let mut out = BTreeMap::new();
    for ex in program.exports {
        if out.contains_key(&ex.name) {
            return Err(format!("{file_name}: duplicate export '{}'", ex.name));
        }
        let _span = ex.span;
        out.insert(ex.name, ex.expr);
    }
    Ok((out, file_name.to_string()))
}
