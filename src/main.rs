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
use std::io::{self, ErrorKind, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Write `content` to `dest` atomically via a sibling temp file then rename,
/// so a partial write is never visible to readers.
fn atomic_write(dest: &Path, content: &str) -> Result<(), String> {
    let tmp = dest.with_extension("tcon_tmp");
    fs::write(&tmp, content)
        .map_err(|e| format!("failed writing temp file {}: {e}", tmp.display()))?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("failed renaming temp file to {}: {e}", dest.display())
    })
}

/// Enable ANSI styling for a stream: TTY, no `NO_COLOR`, `CLICOLOR`≠0, `TERM`≠`dumb`.
fn color_on(stream: &impl IsTerminal) -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if env::var("CLICOLOR")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
    {
        return false;
    }
    if env::var("TERM")
        .map(|v| v.eq_ignore_ascii_case("dumb"))
        .unwrap_or(false)
    {
        return false;
    }
    stream.is_terminal()
}

/// ANSI styling; all codes are empty when color is disabled.
struct Theme {
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
    title: &'static str,
    cmd: &'static str,
    flag: &'static str,
    accent: &'static str,
    ok: &'static str,
    bad: &'static str,
    warn: &'static str,
    path: &'static str,
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut row: Vec<usize> = (0..=m).collect();
    for i in 1..=n {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=m {
            let tmp = row[j];
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + cost);
            prev = tmp;
        }
    }
    row[m]
}

/// If `input` is close to a known subcommand, return it (for "did you mean?" hints).
fn suggest_similar_command(input: &str) -> Option<&'static str> {
    const COMMANDS: &[&str] = &[
        "validate", "build", "generate", "check", "diff", "print", "watch", "init",
    ];
    let threshold = match input.chars().count() {
        0..3 => 0,
        3..6 => 1,
        _ => 2,
    };
    if threshold == 0 {
        return None;
    }
    let mut best: Option<&'static str> = None;
    let mut best_d = usize::MAX;
    for c in COMMANDS {
        let d = levenshtein(input, c);
        if d == 0 {
            return None;
        }
        if d <= threshold && d < best_d {
            best_d = d;
            best = Some(c);
        }
    }
    best
}

impl Theme {
    fn for_stderr() -> Self {
        Self::new(color_on(&io::stderr()))
    }

    fn for_stdout() -> Self {
        Self::new(color_on(&io::stdout()))
    }

    fn new(color: bool) -> Self {
        if !color {
            return Self {
                bold: "",
                dim: "",
                reset: "",
                title: "",
                cmd: "",
                flag: "",
                accent: "",
                ok: "",
                bad: "",
                warn: "",
                path: "",
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
            ok: "\x1b[1;32m",
            bad: "\x1b[1;31m",
            warn: "\x1b[1;33m",
            path: "\x1b[36m",
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
        "Compile `.tcon` only; does not read or write `spec.path` files on disk.",
    );
    cmd(
        "build",
        "Emit outputs to each `spec.path` (relative to workspace root).",
    );
    cmd("generate", "Alias of `build`.");
    cmd(
        "check",
        "Recompile and fail if on-disk outputs differ (drift / stale artifacts).",
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
        "{dim}validate = sources only · check = sources vs on-disk outputs · docs/cli-reference.md · NO_COLOR=1 · CLICOLOR=0{sreset}",
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

/// Compile every entry in `entries`, check for duplicate output paths, and return
/// `(entry_file, output_path, rendered_content)` triples ready for use.
fn compile_all(
    ws: &Workspace,
    entries: &[PathBuf],
) -> Result<Vec<(PathBuf, PathBuf, String)>, String> {
    let mut cache = LoadCache::default();
    let mut seen: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    let mut results = Vec::with_capacity(entries.len());
    for entry_file in entries {
        let (output, rendered) = compile_entry(ws, entry_file, &mut cache)?;
        if !seen.insert(output.clone()) {
            return Err(format!(
                "output collision: multiple .tcon sources emit to '{}' — each spec.path must be unique",
                output.strip_prefix(&ws.root).unwrap_or(&output).display()
            ));
        }
        results.push((entry_file.clone(), output, rendered));
    }
    Ok(results)
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

    let s = Theme::for_stdout();
    let compiled = compile_all(ws, &entries)?;
    for (entry_file, output, _) in &compiled {
        let out_rel = output.strip_prefix(&ws.root).unwrap_or(output).display();
        let entry_rel = entry_file
            .strip_prefix(&ws.tcon_dir)
            .unwrap_or(entry_file)
            .display();
        println!(
            "{ok}ok{s}  {p}{entry_rel}{s} {d}→{s} {p}{out_rel}{s}  {d}(dry-run; disk not touched){s}",
            ok = s.ok,
            p = s.path,
            d = s.dim,
            s = s.reset,
        );
    }
    println!();
    println!(
        "{d}validate{s}  compiled `.tcon` only — did not read or write `{p}spec.path{s}` outputs.",
        d = s.dim,
        p = s.path,
        s = s.reset,
    );
    println!(
        "{d}next{s}     {a}tcon check{s} before commit, or {a}tcon build{s} to refresh artifacts.",
        d = s.dim,
        a = s.accent,
        s = s.reset,
    );
    Ok(())
}

fn run_build(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let s = Theme::for_stdout();
    let compiled = compile_all(ws, &entries)?;
    for (_, output, rendered) in compiled {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed creating output directory: {e}"))?;
        }
        atomic_write(&output, &rendered)?;
        let rel = output.strip_prefix(&ws.root).unwrap_or(&output).display();
        println!(
            "{ok}ok{s}  wrote {p}{rel}{s}",
            ok = s.ok,
            p = s.path,
            s = s.reset,
        );
    }
    Ok(())
}

fn run_check(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let s = Theme::for_stdout();
    let ansi = color_on(&io::stdout());
    let compiled = compile_all(ws, &entries)?;
    let mut drift = 0usize;
    for (_, output, expected) in compiled {
        let rel = output.strip_prefix(&ws.root).unwrap_or(&output).display();
        let (actual, missing) = match fs::read_to_string(&output) {
            Ok(got) => (got, false),
            Err(e) if e.kind() == ErrorKind::NotFound => (String::new(), true),
            Err(e) => {
                return Err(format!(
                    "failed reading on-disk output {}: {e}",
                    output.display()
                ));
            }
        };
        if actual != expected {
            drift += 1;
            let note = if missing {
                "file missing on disk"
            } else {
                "on disk ≠ compiled output"
            };
            println!(
                "{bad}drift{s}  {p}{rel}{s}  {d}({note}){s}",
                bad = s.bad,
                p = s.path,
                d = s.dim,
                s = s.reset,
            );
            println!("{}", describe_drift(&actual, &expected, ansi));
        } else {
            println!(
                "{ok}ok{s}  {p}{rel}{s}  {d}(matches compiled output){s}",
                ok = s.ok,
                p = s.path,
                d = s.dim,
                s = s.reset,
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

    let s = Theme::for_stdout();
    let ansi = color_on(&io::stdout());
    let compiled = compile_all(ws, &entries)?;
    let mut drift = 0usize;
    for (_, output, expected) in compiled {
        let rel = output.strip_prefix(&ws.root).unwrap_or(&output).display();
        let (actual, missing) = match fs::read_to_string(&output) {
            Ok(got) => (got, false),
            Err(e) if e.kind() == ErrorKind::NotFound => (String::new(), true),
            Err(e) => {
                return Err(format!(
                    "failed reading on-disk output {}: {e}",
                    output.display()
                ));
            }
        };
        if actual != expected {
            drift += 1;
            let note = if missing {
                "file missing on disk"
            } else {
                "differs from compiled output"
            };
            println!(
                "{warn}diff{s}  {p}{rel}{s}  {d}({note}){s}",
                warn = s.warn,
                p = s.path,
                d = s.dim,
                s = s.reset,
            );
            println!("{}", describe_drift(&actual, &expected, ansi));
        }
    }

    if drift == 0 {
        println!(
            "{ok}ok{s}  {d}no differences: on-disk outputs match compiled results.{s}",
            ok = s.ok,
            d = s.dim,
            s = s.reset,
        );
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
                print_error(ErrorFormat::Text, &e);
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
            let s = Theme::for_stderr();
            let body = message
                .strip_prefix("error: ")
                .map(str::trim_start)
                .unwrap_or(message);
            eprintln!("{bad}error:{rst} {body}", bad = s.bad, rst = s.reset);
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
            let mut msg = format!("unknown command: {cmd}");
            if let Some(s) = suggest_similar_command(cmd.as_str()) {
                msg.push_str(&format!(" (did you mean `{s}`?)"));
            }
            Err(msg)
        }
    };

    if let Err(e) = result {
        print_error(error_format, &e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod command_hint_tests {
    use super::{levenshtein, suggest_similar_command};

    #[test]
    fn levenshtein_examples() {
        assert_eq!(levenshtein("checl", "check"), 1);
        assert_eq!(levenshtein("check", "check"), 0);
    }

    #[test]
    fn suggest_typo() {
        assert_eq!(suggest_similar_command("checl"), Some("check"));
        assert_eq!(suggest_similar_command("valiate"), Some("validate"));
        assert_eq!(suggest_similar_command("xyzunknown"), None);
    }
}
