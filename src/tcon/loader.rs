use crate::model::{Expr, Program};
use crate::tcon::lexer::lex;
use crate::tcon::parser::parse;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct LoadCache {
    exports: BTreeMap<PathBuf, BTreeMap<String, Expr>>,
}

pub fn load_program_cached(
    path: &Path,
    cache: &mut LoadCache,
) -> Result<BTreeMap<String, Expr>, String> {
    let mut stack = BTreeSet::new();
    load_program_inner(path, &mut stack, cache)
}

fn load_program_inner(
    path: &Path,
    stack: &mut BTreeSet<PathBuf>,
    cache: &mut LoadCache,
) -> Result<BTreeMap<String, Expr>, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("{}: failed to resolve path: {}", path.display(), e))?;
    if let Some(existing) = cache.exports.get(&canonical) {
        return Ok(existing.clone());
    }
    if stack.contains(&canonical) {
        return Err(format!(
            "circular import detected at {}",
            canonical.display()
        ));
    }
    stack.insert(canonical.clone());

    let file_name = canonical.display().to_string();
    let src = fs::read_to_string(&canonical)
        .map_err(|e| format!("{}: failed to read: {}", file_name, e))?;
    let tokens = lex(&src, &file_name)?;
    let program = parse(&tokens, &file_name, &src)?;
    let exports = exports_map(program, &canonical, stack, cache)?;
    stack.remove(&canonical);
    cache.exports.insert(canonical, exports.clone());
    Ok(exports)
}

fn exports_map(
    program: Program,
    current_file: &Path,
    stack: &mut BTreeSet<PathBuf>,
    cache: &mut LoadCache,
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
    }

    for ex in program.exports {
        if out.contains_key(&ex.name) {
            return Err(format!("{file_name}: duplicate export '{}'", ex.name));
        }
        out.insert(ex.name, ex.expr);
    }
    Ok(out)
}

pub fn load_unresolved_program(path: &Path) -> Result<Program, String> {
    let file_name = path.display().to_string();
    let src =
        fs::read_to_string(path).map_err(|e| format!("{}: failed to read: {}", file_name, e))?;
    let tokens = lex(&src, &file_name)?;
    parse(&tokens, &file_name, &src)
}

pub fn collect_dependency_files(entry: &Path) -> Result<Vec<PathBuf>, String> {
    let mut visiting = BTreeSet::new();
    let mut seen = BTreeSet::new();
    dependency_dfs(entry, &mut visiting, &mut seen)?;
    Ok(seen.into_iter().collect())
}

fn dependency_dfs(
    path: &Path,
    visiting: &mut BTreeSet<PathBuf>,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("{}: failed to resolve path: {}", path.display(), e))?;
    if seen.contains(&canonical) {
        return Ok(());
    }
    if visiting.contains(&canonical) {
        return Err(format!(
            "circular import detected at {}",
            canonical.display()
        ));
    }
    visiting.insert(canonical.clone());
    seen.insert(canonical.clone());

    let program = load_unresolved_program(&canonical)?;
    let parent = canonical.parent().unwrap_or_else(|| Path::new("."));
    for import in program.imports {
        let child = parent.join(import.from);
        dependency_dfs(&child, visiting, seen)?;
    }
    visiting.remove(&canonical);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::collect_dependency_files;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn mk_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("tcon_loader_{name}_{nanos}_{}", std::process::id()));
        fs::create_dir_all(root.join(".tcon")).expect("create .tcon");
        root
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }

    #[test]
    fn collects_transitive_dependencies() {
        let root = mk_workspace("deps");
        write_file(
            &root.join(".tcon/base.tcon"),
            r#"
export const leaf = { port: 3000 };
"#,
        );
        write_file(
            &root.join(".tcon/mid.tcon"),
            r#"
import { leaf } from "./base.tcon";
export const shared = leaf;
"#,
        );
        write_file(
            &root.join(".tcon/top.tcon"),
            r#"
import { shared } from "./mid.tcon";
export const spec = { path: "server.json", format: "json" };
export const schema = t.object({ port: t.number().default(1) }).strict();
export const config = shared;
"#,
        );

        let deps =
            collect_dependency_files(&root.join(".tcon/top.tcon")).expect("collect dependencies");
        assert_eq!(deps.len(), 3, "expected top+mid+base");
    }
}
