mod diff;
mod emit;
mod eval;
mod model;
mod tcon;
mod validate;
mod workspace;

use crate::diff::describe_drift;
use crate::emit::env::to_env;
use crate::emit::json::to_pretty_json;
use crate::emit::properties::to_properties;
use crate::emit::toml::to_toml;
use crate::emit::yaml::to_yaml;
use crate::eval::{evaluate_config, evaluate_schema, evaluate_spec};
use crate::tcon::loader::{
    LoadCache, collect_dependency_files, load_program_cached, load_unresolved_program,
};
use crate::validate::validator::validate;
use crate::workspace::Workspace;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorFormat {
    Text,
    Json,
}

fn usage() {
    eprintln!("tcon - typed configuration compiler");
    eprintln!("Usage:");
    eprintln!("  tcon [--error-format text|json] build [--entry <file.tcon>]");
    eprintln!("  tcon [--error-format text|json] check [--entry <file.tcon>]");
    eprintln!("  tcon [--error-format text|json] diff [--entry <file.tcon>]");
    eprintln!("  tcon [--error-format text|json] print --entry <file.tcon>");
    eprintln!("  tcon [--error-format text|json] watch [--entry <file.tcon>]");
    eprintln!("  tcon [--error-format text|json] init [--preset <name>] [--force]");
}

fn parse_optional_entry(args: &[String]) -> Result<Option<&str>, String> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() == 2 && args[0] == "--entry" {
        return Ok(Some(args[1].as_str()));
    }
    Err("expected optional --entry <file.tcon>".to_string())
}

fn parse_required_entry(args: &[String]) -> Result<&str, String> {
    if args.len() == 2 && args[0] == "--entry" {
        return Ok(args[1].as_str());
    }
    Err("print requires --entry <file.tcon>".to_string())
}

fn resolve_entries(ws: &Workspace, entry: Option<&str>) -> Result<Vec<PathBuf>, String> {
    match entry {
        Some(file) => Ok(vec![ws.resolve_entry(file)?]),
        None => ws.find_tcon_entries(),
    }
}

fn compile_entry(
    ws: &Workspace,
    entry_file: &Path,
    cache: &mut LoadCache,
) -> Result<(PathBuf, String), String> {
    let exports = load_program_cached(entry_file, cache)?;
    let file_name = entry_file.display().to_string();
    let spec = evaluate_spec(&exports, &file_name)?;
    if !matches!(
        spec.format.as_str(),
        "json" | "yaml" | "env" | "toml" | "properties"
    ) {
        return Err(format!(
            "{}: unsupported spec.format='{}' (supported: json, yaml, env, toml, properties)",
            file_name, spec.format
        ));
    }
    if spec.format == "env" {
        let lower = spec.path.to_ascii_lowercase();
        if !lower.ends_with(".env") {
            return Err(format!(
                "{}: env output path must end with '.env'",
                file_name
            ));
        }
    }
    if spec.format == "properties" {
        let lower = spec.path.to_ascii_lowercase();
        if !lower.ends_with(".properties") {
            return Err(format!(
                "{}: properties output path must end with '.properties'",
                file_name
            ));
        }
    }
    if spec.mode.is_none() {
        // Keep the mode field explicit in CLI semantics for future expansions.
    }
    if let Some(mode) = &spec.mode
        && mode != "replace"
    {
        return Err(format!(
            "{}: only spec.mode=\"replace\" is supported",
            file_name
        ));
    }
    let schema = evaluate_schema(&exports, &file_name)?;
    let cfg = evaluate_config(&exports, &file_name)?;
    let normalized = validate(&schema, &cfg, &file_name)?;
    let output_path = ws.root.join(&spec.path);
    let rendered = match spec.format.as_str() {
        "json" => format!("{}\n", to_pretty_json(&normalized)),
        "yaml" => format!("{}\n", to_yaml(&normalized)),
        "env" => format!("{}\n", to_env(&normalized)?),
        "toml" => format!("{}\n", to_toml(&normalized)?),
        "properties" => format!("{}\n", to_properties(&normalized)?),
        _ => unreachable!(),
    };
    Ok((output_path, rendered))
}

fn run_build(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let mut cache = LoadCache::default();
    for entry_file in entries {
        let (output, rendered) = compile_entry(ws, &entry_file, &mut cache)?;
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed creating output directory: {e}"))?;
        }
        fs::write(&output, rendered).map_err(|e| format!("failed writing output file: {e}"))?;
        println!(
            "built {}",
            output.strip_prefix(&ws.root).unwrap_or(&output).display()
        );
    }
    Ok(())
}

fn run_check(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let mut cache = LoadCache::default();
    let mut drift = 0usize;
    for entry_file in entries {
        let (output, expected) = compile_entry(ws, &entry_file, &mut cache)?;
        let actual = fs::read_to_string(&output).unwrap_or_default();
        if actual != expected {
            drift += 1;
            println!(
                "drift: {}",
                output.strip_prefix(&ws.root).unwrap_or(&output).display()
            );
            println!("{}", describe_drift(&actual, &expected));
        } else {
            println!(
                "ok: {}",
                output.strip_prefix(&ws.root).unwrap_or(&output).display()
            );
        }
    }

    if drift > 0 {
        return Err(format!("detected drift in {drift} file(s)"));
    }
    Ok(())
}

fn run_diff(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let mut cache = LoadCache::default();
    let mut drift = 0usize;
    for entry_file in entries {
        let (output, expected) = compile_entry(ws, &entry_file, &mut cache)?;
        let actual = fs::read_to_string(&output).unwrap_or_default();
        if actual != expected {
            drift += 1;
            println!(
                "diff: {}",
                output.strip_prefix(&ws.root).unwrap_or(&output).display()
            );
            println!("{}", describe_drift(&actual, &expected));
        }
    }

    if drift == 0 {
        println!("no differences");
        Ok(())
    } else {
        Err(format!("found differences in {drift} file(s)"))
    }
}

fn run_print(ws: &Workspace, entry: &str) -> Result<(), String> {
    let path = ws.resolve_entry(entry)?;
    let program = load_unresolved_program(&path)?;
    println!("{program:#?}");
    Ok(())
}

fn run_watch(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }
    run_build(ws, entry)?;
    println!("watching .tcon files for changes...");

    let mut watched = resolve_watch_files(&entries)?;
    let mut stamps = read_stamps(&watched);
    loop {
        std::thread::sleep(Duration::from_millis(800));
        watched = resolve_watch_files(&entries)?;
        let next = read_stamps(&watched);
        let mut changed = changed_files(&stamps, &next);
        if !changed.is_empty() {
            // Debounce bursts of edits into one rebuild cycle.
            let debounce_until = Instant::now() + Duration::from_millis(450);
            while Instant::now() < debounce_until {
                std::thread::sleep(Duration::from_millis(120));
                watched = resolve_watch_files(&entries)?;
                let later = read_stamps(&watched);
                for p in changed_files(&next, &later) {
                    changed.push(p);
                }
            }

            let mut uniq = BTreeSet::new();
            for p in changed {
                uniq.insert(p);
            }
            let list: Vec<String> = uniq
                .iter()
                .map(|p| p.strip_prefix(&ws.root).unwrap_or(p).display().to_string())
                .collect();

            println!("change detected, rebuilding...");
            println!("changed files: {}", list.join(", "));
            if let Err(e) = run_build(ws, entry) {
                eprintln!("error: {e}");
            }
            watched = resolve_watch_files(&entries)?;
            stamps = read_stamps(&watched);
        } else {
            stamps = next;
        }
    }
}

fn resolve_watch_files(entries: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut merged = std::collections::BTreeSet::new();
    for entry in entries {
        for dep in collect_dependency_files(entry)? {
            merged.insert(dep);
        }
    }
    Ok(merged.into_iter().collect())
}

fn read_stamps(files: &[PathBuf]) -> BTreeMap<PathBuf, Option<SystemTime>> {
    let mut map = BTreeMap::new();
    for p in files {
        map.insert(p.clone(), fs::metadata(p).and_then(|m| m.modified()).ok());
    }
    map
}

fn changed_files(
    before: &BTreeMap<PathBuf, Option<SystemTime>>,
    after: &BTreeMap<PathBuf, Option<SystemTime>>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut keys = BTreeSet::new();
    for k in before.keys() {
        keys.insert(k.clone());
    }
    for k in after.keys() {
        keys.insert(k.clone());
    }
    for k in keys {
        let b = before.get(&k).copied().flatten();
        let a = after.get(&k).copied().flatten();
        if b != a {
            out.push(k);
        }
    }
    out
}

fn parse_global_args(args: &[String]) -> Result<(ErrorFormat, String, Vec<String>), String> {
    let mut format = ErrorFormat::Text;
    let mut positional = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--error-format" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| "missing value for --error-format".to_string())?;
            format = match value.as_str() {
                "text" => ErrorFormat::Text,
                "json" => ErrorFormat::Json,
                _ => {
                    return Err(format!(
                        "unsupported --error-format '{}' (expected text|json)",
                        value
                    ));
                }
            };
            i += 2;
            continue;
        }
        positional.push(args[i].clone());
        i += 1;
    }

    let Some(cmd) = positional.first() else {
        return Err("missing command".to_string());
    };
    Ok((format, cmd.clone(), positional[1..].to_vec()))
}

#[derive(Debug, Clone)]
struct DiagnosticJson {
    code: &'static str,
    message: String,
    file: Option<String>,
    line: Option<usize>,
    col: Option<usize>,
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn classify_error_code(message: &str) -> &'static str {
    if message.contains("unexpected character") {
        return "E_LEX_UNEXPECTED_CHAR";
    }
    if message.contains("unterminated string literal") {
        return "E_LEX_UNTERMINATED_STRING";
    }
    if message.contains("missing required export") {
        return "E_EVAL_MISSING_EXPORT";
    }
    if message.contains("circular import detected") {
        return "E_IMPORT_CYCLE";
    }
    if message.contains("enum value not in allowed variants") {
        return "E_VALIDATE_ENUM";
    }
    if message.contains("unsupported spec.format") {
        return "E_SPEC_FORMAT";
    }
    if message.contains("expected ") || message.contains("unsupported schema") {
        return "E_PARSE_OR_SCHEMA";
    }
    "E_RUNTIME"
}

fn parse_diagnostic(message: &str) -> DiagnosticJson {
    let mut file = None;
    let mut line = None;
    let mut col = None;
    for l in message.lines() {
        let trimmed = l.trim_start();
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            let mut parts = rest.rsplitn(3, ':');
            let c = parts.next();
            let ln = parts.next();
            let f = parts.next();
            if let (Some(c), Some(ln), Some(f)) = (c, ln, f) {
                file = Some(f.to_string());
                line = ln.parse::<usize>().ok();
                col = c.parse::<usize>().ok();
            }
        }
    }
    let first_line = message.lines().next().unwrap_or(message);
    let msg = if let Some(rest) = first_line.strip_prefix("error: ") {
        rest.to_string()
    } else {
        first_line.to_string()
    };
    DiagnosticJson {
        code: classify_error_code(message),
        message: msg,
        file,
        line,
        col,
    }
}

fn print_error(format: ErrorFormat, message: &str) {
    match format {
        ErrorFormat::Text => {
            if message.starts_with("error: ") {
                eprintln!("{message}");
            } else {
                eprintln!("error: {message}");
            }
        }
        ErrorFormat::Json => {
            let d = parse_diagnostic(message);
            let file = d.file.unwrap_or_default();
            let line = d
                .line
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());
            let col = d
                .col
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());
            let file_json = if file.is_empty() {
                "null".to_string()
            } else {
                format!("\"{}\"", json_escape(&file))
            };
            eprintln!(
                "{{\"code\":\"{}\",\"message\":\"{}\",\"file\":{},\"line\":{},\"col\":{}}}",
                d.code,
                json_escape(&d.message),
                file_json,
                line,
                col
            );
        }
    }
}

fn run_init(ws: &Workspace, args: &[String]) -> Result<(), String> {
    let mut preset: Option<&str> = None;
    let mut force = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--preset" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --preset".to_string())?;
                preset = Some(v.as_str());
                i += 2;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            other => {
                return Err(format!("unknown init argument: {other}"));
            }
        }
    }

    let presets = match preset {
        Some("json") => vec![("sample_json.tcon", init_json())],
        Some("yaml") => vec![("sample_yaml.tcon", init_yaml())],
        Some("env") => vec![("sample_env.tcon", init_env())],
        Some("toml") => vec![("sample_toml.tcon", init_toml())],
        Some("properties") => vec![("sample_properties.tcon", init_properties())],
        Some("all") | None => vec![
            ("sample_json.tcon", init_json()),
            ("sample_yaml.tcon", init_yaml()),
            ("sample_env.tcon", init_env()),
            ("sample_toml.tcon", init_toml()),
            ("sample_properties.tcon", init_properties()),
        ],
        Some(other) => return Err(format!("unknown preset '{other}'")),
    };

    for (name, content) in presets {
        let path = ws.tcon_dir.join(name);
        if path.exists() && !force {
            println!(
                "skip {} (already exists, use --force)",
                path.strip_prefix(&ws.root).unwrap_or(&path).display()
            );
            continue;
        }
        fs::write(&path, content).map_err(|e| format!("failed writing {}: {e}", path.display()))?;
        println!(
            "init {}",
            path.strip_prefix(&ws.root).unwrap_or(&path).display()
        );
    }
    Ok(())
}

fn init_json() -> &'static str {
    r#"export const spec = {
  path: "sample.json",
  format: "json",
  mode: "replace",
};

export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();

export const config = {
  port: 3000,
};
"#
}

fn init_yaml() -> &'static str {
    r#"export const spec = {
  path: "sample.yaml",
  format: "yaml",
  mode: "replace",
};

export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();

export const config = {
  port: 3000,
};
"#
}

fn init_env() -> &'static str {
    r#"export const spec = {
  path: "sample.env",
  format: "env",
  mode: "replace",
};

export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();

export const config = {
  port: 3000,
};
"#
}

fn init_toml() -> &'static str {
    r#"export const spec = {
  path: "sample.toml",
  format: "toml",
  mode: "replace",
};

export const schema = t.object({
  app: t.object({
    name: t.string().default("tcon-app"),
    port: t.number().int().default(8080),
  }).strict(),
}).strict();

export const config = {
  app: {
    port: 3000,
  },
};
"#
}

fn init_properties() -> &'static str {
    r#"export const spec = {
  path: "sample.properties",
  format: "properties",
  mode: "replace",
};

export const schema = t.object({
  app: t.object({
    host: t.string().default("0.0.0.0"),
    port: t.number().int().default(8080),
  }).strict(),
}).strict();

export const config = {
  app: {
    port: 3000,
  },
};
"#
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (error_format, cmd, rest) = match parse_global_args(&args) {
        Ok(v) => v,
        Err(e) => {
            usage();
            print_error(ErrorFormat::Text, &e);
            std::process::exit(2);
        }
    };

    let ws = match Workspace::discover(None) {
        Ok(ws) => ws,
        Err(e) => {
            print_error(error_format, &e);
            std::process::exit(1);
        }
    };

    let result = match cmd.as_str() {
        "build" => parse_optional_entry(&rest).and_then(|entry| run_build(&ws, entry)),
        "check" => parse_optional_entry(&rest).and_then(|entry| run_check(&ws, entry)),
        "diff" => parse_optional_entry(&rest).and_then(|entry| run_diff(&ws, entry)),
        "print" => parse_required_entry(&rest).and_then(|entry| run_print(&ws, entry)),
        "watch" => parse_optional_entry(&rest).and_then(|entry| run_watch(&ws, entry)),
        "init" => run_init(&ws, &rest),
        _ => {
            usage();
            Err(format!("unknown command: {cmd}"))
        }
    };

    if let Err(e) = result {
        print_error(error_format, &e);
        std::process::exit(1);
    }
}
