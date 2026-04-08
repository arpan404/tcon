use crate::model::{Expr, Program};
use crate::tcon::lexer::lex;
use crate::tcon::parser::parse;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_program(path: &PathBuf) -> Result<(BTreeMap<String, Expr>, String), String> {
    let mut stack = BTreeSet::new();
    let mut cache = BTreeMap::new();
    let exports = load_program_inner(path, &mut stack, &mut cache)?;
    Ok((exports, path.display().to_string()))
}

fn load_program_inner(
    path: &Path,
    stack: &mut BTreeSet<PathBuf>,
    cache: &mut BTreeMap<PathBuf, BTreeMap<String, Expr>>,
) -> Result<BTreeMap<String, Expr>, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("{}: failed to resolve path: {}", path.display(), e))?;
    if let Some(existing) = cache.get(&canonical) {
        return Ok(existing.clone());
    }
    if stack.contains(&canonical) {
        return Err(format!("circular import detected at {}", canonical.display()));
    }
    stack.insert(canonical.clone());

    let file_name = canonical.display().to_string();
    let src = fs::read_to_string(&canonical)
        .map_err(|e| format!("{}: failed to read: {}", file_name, e))?;
    let tokens = lex(&src, &file_name)?;
    let program = parse(&tokens, &file_name)?;
    let exports = exports_map(program, &canonical, stack, cache)?;
    stack.remove(&canonical);
    cache.insert(canonical, exports.clone());
    Ok(exports)
}

fn exports_map(
    program: Program,
    current_file: &Path,
    stack: &mut BTreeSet<PathBuf>,
    cache: &mut BTreeMap<PathBuf, BTreeMap<String, Expr>>,
) -> Result<BTreeMap<String, Expr>, String> {
    let file_name = current_file.display().to_string();
    let mut out = BTreeMap::new();
    for import in program.imports {
        let parent = current_file.parent().unwrap_or_else(|| Path::new("."));
        let import_path = parent.join(import.from);
        let imported = load_program_inner(&import_path, stack, cache)?;
        for name in import.names {
            let expr = imported.get(&name).ok_or_else(|| {
                format!(
                    "{}: imported symbol '{}' not found in {}",
                    file_name,
                    name,
                    import_path.display()
                )
            })?;
            if out.contains_key(&name) {
                return Err(format!("{file_name}: duplicate symbol '{}'", name));
            }
            out.insert(name, expr.clone());
        }
        let _span = import.span;
    }

    for ex in program.exports {
        if out.contains_key(&ex.name) {
            return Err(format!("{file_name}: duplicate export '{}'", ex.name));
        }
        out.insert(ex.name, ex.expr);
    }
    Ok(out)
}

pub fn load_unresolved_program(path: &PathBuf) -> Result<Program, String> {
    let file_name = path.display().to_string();
    let src = fs::read_to_string(path).map_err(|e| format!("{}: failed to read: {}", file_name, e))?;
    let tokens = lex(&src, &file_name)?;
    parse(&tokens, &file_name)
}
