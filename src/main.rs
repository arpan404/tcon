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
use crate::validate::validator::{validate, validate_schema_defaults};
use crate::workspace::Workspace;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// ANSI styling for help text; empty when `NO_COLOR` is set or stderr isn’t a TTY.
struct Theme {
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
    title: &'static str,
    cmd: &'static str,
    flag: &'static str,
    accent: &'static str,
}

impl Theme {
    fn for_stderr() -> Self {
        Self::new(io::stderr().is_terminal())
    }

    fn for_stdout() -> Self {
        Self::new(io::stdout().is_terminal())
    }

    fn new(color: bool) -> Self {
        if env::var_os("NO_COLOR").is_some() || !color {
            return Self {
                bold: "",
                dim: "",
                reset: "",
                title: "",
                cmd: "",
                flag: "",
                accent: "",
            };
        }
        Self {
            bold: "\x1b[1m",
            dim: "\x1b[2m",
            reset: "\x1b[0m",
            title: "\x1b[1;36m",
            cmd: "\x1b[1;32m",
            flag: "\x1b[33m",
            accent: "\x1b[35m",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorFormat {
    Text,
    Json,
}

fn usage() {
    let s = Theme::for_stderr();
    let ver = env!("CARGO_PKG_VERSION");
    eprintln!(
        "{ti}tcon{sreset} {dim}v{ver}{sreset} — typed configuration compiler",
        ti = s.title,
        sreset = s.reset,
        dim = s.dim,
        ver = ver
    );
    eprintln!(
        "{dim}Compile `.tcon` schema + values into JSON, YAML, ENV, TOML, or Java properties.{sreset}",
        dim = s.dim,
        sreset = s.reset
    );
    eprintln!();
    eprintln!("{bold}USAGE{sreset}", bold = s.bold, sreset = s.reset);
    eprintln!(
        "  {cmd}tcon{sreset} [{flag}OPTIONS{sreset}] {dim}<COMMAND>{sreset} [{dim}…{sreset}]",
        cmd = s.cmd,
        flag = s.flag,
        dim = s.dim,
        sreset = s.reset
    );
    eprintln!();
    eprintln!(
        "{bold}GLOBAL OPTIONS{sreset}",
        bold = s.bold,
        sreset = s.reset
    );
    eprintln!(
        "  {flag}--error-format{sreset} {dim}<text|json>{sreset}   Diagnostics: human text (default) or one JSON object per error.",
        flag = s.flag,
        dim = s.dim,
        sreset = s.reset
    );
    eprintln!(
        "  {flag}-h{sreset}, {flag}--help{sreset}{pad}This screen.",
        flag = s.flag,
        sreset = s.reset,
        pad = " ".repeat(22)
    );
    eprintln!(
        "  {flag}-V{sreset}, {flag}--version{sreset}{pad}Show version.",
        flag = s.flag,
        sreset = s.reset,
        pad = " ".repeat(18)
    );
    eprintln!();
    eprintln!("{bold}COMMANDS{sreset}", bold = s.bold, sreset = s.reset);
    let cmd = |name: &str, desc: &str| {
        eprintln!(
            "  {c}{n:<13}{r}{d}{desc}{r}",
            c = s.cmd,
            n = name,
            r = s.reset,
            d = s.dim,
            desc = desc
        );
    };
    cmd(
        "validate",
        "Compile entries in memory only — no writes (great for CI).",
    );
    cmd(
        "build",
        "Emit outputs to each `spec.path` (relative to workspace root).",
    );
    cmd("generate", "Alias of `build`.");
    cmd(
        "check",
        "Recompile and fail if on-disk files differ from the result.",
    );
    cmd("diff", "Show unified-style hunks for files that differ.");
    cmd(
        "print",
        "Debug: print the parsed program for one `--entry`.",
    );
    cmd(
        "watch",
        "Poll sources & rebuild on change. Flags: `--entry`, `--interval-ms` (default 800, min 100).",
    );
    cmd(
        "init",
        "Scaffold samples under `.tcon/`. Flags: `--preset`, `--force`.",
    );
    eprintln!();
    eprintln!("{bold}COMMON ARGS{sreset}", bold = s.bold, sreset = s.reset);
    eprintln!(
        "  {flag}--entry{sreset} {dim}<file.tcon>{sreset}     Path under `.tcon/` (or absolute).",
        flag = s.flag,
        dim = s.dim,
        sreset = s.reset
    );
    eprintln!(
        "  {dim}             Applies to: validate, build, generate, check, diff, watch, print.{sreset}",
        dim = s.dim,
        sreset = s.reset
    );
    eprintln!();
    eprintln!("{bold}EXAMPLES{sreset}", bold = s.bold, sreset = s.reset);
    eprintln!(
        "  {acc}$ tcon validate && tcon check{sreset}",
        acc = s.accent,
        sreset = s.reset
    );
    eprintln!(
        "  {acc}$ tcon build --entry api/server.tcon{sreset}",
        acc = s.accent,
        sreset = s.reset
    );
    eprintln!(
        "  {acc}$ tcon --error-format json validate{sreset}",
        acc = s.accent,
        sreset = s.reset
    );
    eprintln!();
    eprintln!(
        "{dim}More: docs/cli-reference.md · Disable ANSI: NO_COLOR=1{sreset}",
        dim = s.dim,
        sreset = s.reset
    );
}

fn print_version() {
    let s = Theme::for_stdout();
    let ver = env!("CARGO_PKG_VERSION");
    println!(
        "{ti}tcon{sreset} {bold}{ver}{sreset}",
        ti = s.title,
        bold = s.bold,
        sreset = s.reset,
        ver = ver
    );
    println!(
        "{dim}typed configuration compiler{sreset}",
        dim = s.dim,
        sreset = s.reset
    );
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

fn parse_watch_args(args: &[String]) -> Result<(Option<&str>, Duration), String> {
    let mut entry: Option<&str> = None;
    let mut interval_ms: u64 = 800;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--entry" {
            let path = args
                .get(i + 1)
                .ok_or_else(|| "missing value for --entry".to_string())?;
            entry = Some(path.as_str());
            i += 2;
            continue;
        }
        if args[i] == "--interval-ms" {
            let raw = args
                .get(i + 1)
                .ok_or_else(|| "missing value for --interval-ms".to_string())?;
            let ms: u64 = raw
                .parse()
                .map_err(|_| format!("invalid --interval-ms value '{raw}'"))?;
            if ms < 100 {
                return Err("--interval-ms must be at least 100".to_string());
            }
            interval_ms = ms;
            i += 2;
            continue;
        }
        return Err(format!("unexpected watch argument: {}", args[i]));
    }
    Ok((entry, Duration::from_millis(interval_ms)))
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
    if let Some(mode) = &spec.mode
        && mode != "replace"
    {
        return Err(format!(
            "{}: only spec.mode=\"replace\" is supported",
            file_name
        ));
    }
    let schema = evaluate_schema(&exports, &file_name)?;
    validate_schema_defaults(&schema, &file_name)?;
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

fn run_validate(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let mut cache = LoadCache::default();
    for entry_file in entries {
        let (output, _) = compile_entry(ws, &entry_file, &mut cache)?;
        println!(
            "valid {}",
            output.strip_prefix(&ws.root).unwrap_or(&output).display()
        );
    }
    Ok(())
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

fn run_watch(ws: &Workspace, entry: Option<&str>, poll_interval: Duration) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }
    run_build(ws, entry)?;
    let mut watched = resolve_watch_files(&entries)?;
    println!(
        "watching {} source file(s) (poll every {}ms); Ctrl+C to stop",
        watched.len(),
        poll_interval.as_millis()
    );
    for p in &watched {
        println!("  - {}", p.strip_prefix(&ws.root).unwrap_or(p).display());
    }

    let mut stamps = read_stamps(&watched);
    let debounce_main =
        Duration::from_millis((poll_interval.as_millis() / 2).clamp(150, 800) as u64);
    let debounce_tick = Duration::from_millis(120);
    loop {
        std::thread::sleep(poll_interval);
        watched = resolve_watch_files(&entries)?;
        let next = read_stamps(&watched);
        let mut changed = changed_files(&stamps, &next);
        if !changed.is_empty() {
            // Debounce bursts of edits into one rebuild cycle.
            let debounce_until = Instant::now() + debounce_main;
            while Instant::now() < debounce_until {
                std::thread::sleep(debounce_tick);
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
    if message.contains("unterminated block comment") {
        return "E_LEX_UNTERMINATED_BLOCK_COMMENT";
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
    if message.contains("unknown key(s) in strict object") {
        return "E_VALIDATE_STRICT_UNKNOWN_KEY";
    }
    if message.contains("unknown key in spec object") {
        return "E_SPEC_UNKNOWN_KEY";
    }
    if message.contains("unsupported spec.format") {
        return "E_SPEC_FORMAT";
    }
    if message.contains("expected ")
        || message.contains("unsupported schema")
        || message.contains("t.union() requires")
        || message.contains("t.enum() requires")
        || message.contains(".strict() only valid")
        || message.contains("duplicate key in object literal")
        || message.contains("import requires at least one binding")
    {
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

    if cmd == "help" || cmd == "--help" || cmd == "-h" {
        usage();
        return;
    }
    if cmd == "version" || cmd == "--version" || cmd == "-V" {
        print_version();
        return;
    }

    let ws = match if cmd == "init" {
        Workspace::discover_or_create(None)
    } else {
        Workspace::discover(None)
    } {
        Ok(ws) => ws,
        Err(e) => {
            print_error(error_format, &e);
            std::process::exit(1);
        }
    };

    let result = match cmd.as_str() {
        "validate" => parse_optional_entry(&rest).and_then(|entry| run_validate(&ws, entry)),
        "build" | "generate" => parse_optional_entry(&rest).and_then(|entry| run_build(&ws, entry)),
        "check" => parse_optional_entry(&rest).and_then(|entry| run_check(&ws, entry)),
        "diff" => parse_optional_entry(&rest).and_then(|entry| run_diff(&ws, entry)),
        "print" => parse_required_entry(&rest).and_then(|entry| run_print(&ws, entry)),
        "watch" => {
            parse_watch_args(&rest).and_then(|(entry, interval)| run_watch(&ws, entry, interval))
        }
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
