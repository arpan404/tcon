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
use crate::eval::{evaluate_config, evaluate_schema, evaluate_spec, raw_config_expr};
use crate::model::{Schema, Value};
use crate::tcon::loader::{
    LoadCache, collect_dependency_files, load_program_cached, load_unresolved_program,
};
use crate::validate::validator::{validate, validate_schema_defaults, validate_secret_fields};
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
        "validate", "build", "generate", "check", "diff", "status", "print", "watch", "init",
        "secrets",
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
        "  {flag}-q{sreset}, {flag}--quiet{sreset}{pad}Suppress stdout; only emit errors to stderr (CI-friendly).",
        flag = s.flag,
        sreset = s.reset,
        pad = " ".repeat(20)
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
        "status",
        "Per-entry health check: ok / drift / missing / error. Handles compile errors gracefully.",
    );
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
    cmd(
        "secrets",
        "Audit git-tracked files for exposed secrets (.env, *.key, *.pem, etc.) and .gitignore gaps.",
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
        "{dim}validate = sources only · check = sources vs on-disk outputs · docs/cli-reference.md · NO_COLOR=1 · CLICOLOR=0 · tcon secrets for git exposure audit{sreset}",
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
    let raw_cfg = raw_config_expr(&exports, &file_name)?;
    validate_secret_fields(&schema, raw_cfg, &file_name)?;
    let cfg = evaluate_config(&exports, &file_name)?;
    let normalized = validate(&schema, &cfg, &file_name)?;
    let output_path = ws.root.join(&spec.path);
    ensure_output_path_within_workspace(&ws.root, &output_path, &file_name)?;
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

fn ensure_output_path_within_workspace(
    workspace_root: &Path,
    output_path: &Path,
    file_name: &str,
) -> Result<(), String> {
    let root_canonical = fs::canonicalize(workspace_root).map_err(|e| {
        format!(
            "{file_name}: failed to resolve workspace root {}: {e}",
            workspace_root.display()
        )
    })?;

    let mut probe = output_path;
    while !probe.exists() {
        probe = probe.parent().ok_or_else(|| {
            format!(
                "{file_name}: spec.path '{}' resolves to an invalid filesystem location",
                output_path.display()
            )
        })?;
    }

    let probe_canonical = fs::canonicalize(probe).map_err(|e| {
        format!(
            "{file_name}: failed to resolve output ancestor '{}': {e}",
            probe.display()
        )
    })?;

    if !probe_canonical.starts_with(&root_canonical) {
        return Err(format!(
            "{file_name}: spec.path '{}' escapes workspace root after symlink resolution (resolved ancestor: '{}')",
            output_path.display(),
            probe_canonical.display()
        ));
    }

    Ok(())
}

fn run_status(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let s = Theme::for_stdout();
    let ansi = color_on(&io::stdout());
    let mut ok_count = 0usize;
    let total = entries.len();

    for entry_file in &entries {
        let entry_rel = entry_file
            .strip_prefix(&ws.tcon_dir)
            .unwrap_or(entry_file)
            .display();
        let mut cache = LoadCache::default();
        match compile_entry(ws, entry_file, &mut cache) {
            Err(e) => {
                let first = e.lines().next().unwrap_or("(error)");
                println!(
                    "{bad}error{rst}  {p}{entry_rel}{rst}",
                    bad = s.bad,
                    p = s.path,
                    rst = s.reset
                );
                println!("         {d}{first}{rst}", d = s.dim, rst = s.reset);
                let extra_count = e.lines().count().saturating_sub(1);
                if extra_count > 0 {
                    println!(
                        "         {d}… and {extra_count} more error(s){rst}",
                        d = s.dim,
                        rst = s.reset
                    );
                }
            }
            Ok((output, expected)) => {
                let rel = output.strip_prefix(&ws.root).unwrap_or(&output).display();
                match fs::read_to_string(&output) {
                    Ok(actual) if actual == expected => {
                        ok_count += 1;
                        println!(
                            "{ok}  ok{rst}  {p}{entry_rel}{rst}  {d}→ {rel}{rst}",
                            ok = s.ok,
                            p = s.path,
                            d = s.dim,
                            rst = s.reset
                        );
                    }
                    Ok(_) => {
                        let diff = describe_drift(
                            &fs::read_to_string(&output).unwrap_or_default(),
                            &expected,
                            ansi,
                        );
                        println!(
                            "{warn}drift{rst}  {p}{entry_rel}{rst}  {d}→ {rel}  (on disk ≠ compiled){rst}",
                            warn = s.warn,
                            p = s.path,
                            d = s.dim,
                            rst = s.reset
                        );
                        for line in diff.lines().take(6) {
                            println!("         {line}");
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        println!(
                            "{bad} miss{rst}  {p}{entry_rel}{rst}  {d}→ {rel}  (run `tcon build`){rst}",
                            bad = s.bad,
                            p = s.path,
                            d = s.dim,
                            rst = s.reset
                        );
                    }
                    Err(e) => {
                        println!(
                            "{bad}error{rst}  {p}{entry_rel}{rst}  {d}→ {rel}  ({e}){rst}",
                            bad = s.bad,
                            p = s.path,
                            d = s.dim,
                            rst = s.reset
                        );
                    }
                }
            }
        }
    }

    println!();
    let (summary_color, summary_icon) = if ok_count == total {
        (s.ok, "✓")
    } else {
        (s.warn, "!")
    };
    println!(
        "{c}{summary_icon} {ok_count}/{total} output(s) up to date{rst}",
        c = summary_color,
        rst = s.reset
    );

    if ok_count < total {
        Err(format!(
            "{}/{} output(s) need attention — run `tcon build` or `tcon check`",
            total - ok_count,
            total
        ))
    } else {
        Ok(())
    }
}

fn run_validate(ws: &Workspace, entry: Option<&str>, quiet: bool) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    let s = Theme::for_stdout();
    let compiled = compile_all(ws, &entries)?;
    if !quiet {
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
    }
    Ok(())
}

fn run_build(ws: &Workspace, entry: Option<&str>, quiet: bool) -> Result<(), String> {
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
        if !quiet {
            let rel = output.strip_prefix(&ws.root).unwrap_or(&output).display();
            println!(
                "{ok}ok{s}  wrote {p}{rel}{s}",
                ok = s.ok,
                p = s.path,
                s = s.reset,
            );
        }
    }
    Ok(())
}

fn run_check(ws: &Workspace, entry: Option<&str>, quiet: bool) -> Result<(), String> {
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
            if !quiet {
                println!(
                    "{bad}drift{s}  {p}{rel}{s}  {d}({note}){s}",
                    bad = s.bad,
                    p = s.path,
                    d = s.dim,
                    s = s.reset,
                );
                println!("{}", describe_drift(&actual, &expected, ansi));
            }
        } else if !quiet {
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

fn redact_secrets(value: &Value, schema: &Schema) -> Value {
    if schema.is_secret() {
        return Value::String("[secret]".to_string());
    }

    match (schema, value) {
        (
            Schema::Object {
                fields,
                secret: false,
                ..
            },
            Value::Object(map),
        ) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in map {
                let redacted = if let Some(fs) = fields.get(k) {
                    redact_secrets(v, fs)
                } else {
                    v.clone()
                };
                out.insert(k.clone(), redacted);
            }
            Value::Object(out)
        }
        (Schema::Array { item, .. }, Value::Array(items)) => Value::Array(
            items
                .iter()
                .map(|item_value| redact_secrets(item_value, item))
                .collect(),
        ),
        (Schema::Record { value: item, .. }, Value::Object(map)) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in map {
                out.insert(k.clone(), redact_secrets(v, item));
            }
            Value::Object(out)
        }
        (Schema::Union { variants, .. }, v) => {
            for variant in variants {
                let candidate = redact_secrets(v, variant);
                if candidate != *v {
                    return candidate;
                }
            }
            v.clone()
        }
        (_, v) => v.clone(),
    }
}

fn run_print(ws: &Workspace, entry: &str) -> Result<(), String> {
    let path = ws.resolve_entry(entry)?;
    let s = Theme::for_stdout();
    let file_name = path.display().to_string();

    // Raw AST (existing debug behavior)
    let program = load_unresolved_program(&path)?;
    println!("{bold}── raw AST ──{r}", bold = s.bold, r = s.reset);
    println!("{program:#?}");

    // Evaluated output with secrets redacted
    let mut cache = LoadCache::default();
    let exports = load_program_cached(&path, &mut cache)?;
    match evaluate_schema(&exports, &file_name) {
        Ok(schema) => match evaluate_config(&exports, &file_name) {
            Ok(cfg) => match validate(&schema, &cfg, &file_name) {
                Ok(normalized) => {
                    let display = redact_secrets(&normalized, &schema);
                    println!();
                    println!(
                        "{bold}── evaluated config (secrets redacted) ──{r}",
                        bold = s.bold,
                        r = s.reset
                    );
                    println!("{display:#?}");
                }
                Err(e) => {
                    println!();
                    println!(
                        "{warn}── evaluated config (validation failed) ──{r}",
                        warn = s.warn,
                        r = s.reset
                    );
                    println!("{e}");
                }
            },
            Err(e) => {
                println!();
                println!(
                    "{d}(config evaluation skipped: {e}){r}",
                    d = s.dim,
                    r = s.reset
                );
            }
        },
        Err(e) => {
            println!();
            println!(
                "{d}(schema evaluation skipped: {e}){r}",
                d = s.dim,
                r = s.reset
            );
        }
    }
    Ok(())
}

fn run_watch(ws: &Workspace, entry: Option<&str>, poll_interval: Duration) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }
    run_build(ws, entry, false)?;
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
            if let Err(e) = run_build(ws, entry, false) {
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

fn parse_global_args(args: &[String]) -> Result<(ErrorFormat, bool, String, Vec<String>), String> {
    let mut format = ErrorFormat::Text;
    let mut quiet = false;
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
        if args[i] == "--quiet" || args[i] == "-q" {
            quiet = true;
            i += 1;
            continue;
        }
        positional.push(args[i].clone());
        i += 1;
    }

    let Some(cmd) = positional.first() else {
        return Err("missing command".to_string());
    };
    Ok((format, quiet, cmd.clone(), positional[1..].to_vec()))
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

#[derive(Debug, Clone, Copy)]
enum DiagnosticCode {
    LexUnexpectedChar,
    LexUnterminatedString,
    LexUnterminatedBlockComment,
    EvalMissingExport,
    ImportCycle,
    ValidateEnum,
    ValidateStrictUnknownKey,
    ValidateSecret,
    SpecUnknownKey,
    SpecFormat,
    SpecPathEscape,
    EmitNullUnsupported,
    OutputCollision,
    EnvInterpolation,
    ParseOrSchema,
    Runtime,
}

impl DiagnosticCode {
    fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::LexUnexpectedChar => "E_LEX_UNEXPECTED_CHAR",
            DiagnosticCode::LexUnterminatedString => "E_LEX_UNTERMINATED_STRING",
            DiagnosticCode::LexUnterminatedBlockComment => "E_LEX_UNTERMINATED_BLOCK_COMMENT",
            DiagnosticCode::EvalMissingExport => "E_EVAL_MISSING_EXPORT",
            DiagnosticCode::ImportCycle => "E_IMPORT_CYCLE",
            DiagnosticCode::ValidateEnum => "E_VALIDATE_ENUM",
            DiagnosticCode::ValidateStrictUnknownKey => "E_VALIDATE_STRICT_UNKNOWN_KEY",
            DiagnosticCode::ValidateSecret => "E_VALIDATE_SECRET",
            DiagnosticCode::SpecUnknownKey => "E_SPEC_UNKNOWN_KEY",
            DiagnosticCode::SpecFormat => "E_SPEC_FORMAT",
            DiagnosticCode::SpecPathEscape => "E_SPEC_PATH_ESCAPE",
            DiagnosticCode::EmitNullUnsupported => "E_EMIT_NULL_UNSUPPORTED",
            DiagnosticCode::OutputCollision => "E_OUTPUT_COLLISION",
            DiagnosticCode::EnvInterpolation => "E_ENV_INTERPOLATION",
            DiagnosticCode::ParseOrSchema => "E_PARSE_OR_SCHEMA",
            DiagnosticCode::Runtime => "E_RUNTIME",
        }
    }
}

fn classify_error_code(message: &str) -> &'static str {
    let code = if message.contains("unexpected character") {
        DiagnosticCode::LexUnexpectedChar
    } else if message.contains("unterminated string literal") {
        DiagnosticCode::LexUnterminatedString
    } else if message.contains("unterminated block comment") {
        DiagnosticCode::LexUnterminatedBlockComment
    } else if message.contains("missing required export") {
        DiagnosticCode::EvalMissingExport
    } else if message.contains("circular import detected") {
        DiagnosticCode::ImportCycle
    } else if message.contains("enum value not in allowed variants") {
        DiagnosticCode::ValidateEnum
    } else if message.contains("unknown key(s) in strict object") {
        DiagnosticCode::ValidateStrictUnknownKey
    } else if message.contains("secret field must") || message.contains(".secret()") {
        DiagnosticCode::ValidateSecret
    } else if message.contains("unknown key in spec object") {
        DiagnosticCode::SpecUnknownKey
    } else if message.contains("unsupported spec.format") {
        DiagnosticCode::SpecFormat
    } else if message.contains("escapes workspace root after symlink resolution")
        || message.contains("path traversal outside workspace")
    {
        DiagnosticCode::SpecPathEscape
    } else if message.contains("output collision: multiple .tcon sources emit to") {
        DiagnosticCode::OutputCollision
    } else if message.contains("env variable")
        && (message.contains("not set") || message.contains("interpolation"))
    {
        DiagnosticCode::EnvInterpolation
    } else if message.contains("cannot emit null")
        || message.contains("cannot represent null")
        || message.contains("toml emitter cannot represent null")
    {
        DiagnosticCode::EmitNullUnsupported
    } else if message.contains("expected ")
        || message.contains("unsupported schema")
        || message.contains("t.union() requires")
        || message.contains("t.enum() requires")
        || message.contains(".strict() only valid")
        || message.contains("duplicate key in object literal")
        || message.contains("import requires at least one binding")
    {
        DiagnosticCode::ParseOrSchema
    } else {
        DiagnosticCode::Runtime
    };
    code.as_str()
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

fn emit_single_error_json(message: &str) {
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

/// Print one or more errors.  When the validator returns a multi-line error
/// (one per `\n`), each line is printed as a separate diagnostic.
fn print_error(format: ErrorFormat, message: &str) {
    let lines: Vec<&str> = message.lines().collect();
    match format {
        ErrorFormat::Text => {
            let s = Theme::for_stderr();
            for line in &lines {
                let body = line
                    .strip_prefix("error: ")
                    .map(str::trim_start)
                    .unwrap_or(line);
                eprintln!("{bad}error:{rst} {body}", bad = s.bad, rst = s.reset);
            }
            if lines.len() > 1 {
                eprintln!(
                    "{bad}  └─ {n} error(s) above{rst}",
                    bad = s.bad,
                    n = lines.len(),
                    rst = s.reset
                );
            }
        }
        ErrorFormat::Json => {
            for line in &lines {
                emit_single_error_json(line);
            }
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

// ─── secrets audit ───────────────────────────────────────────────────────────

/// Patterns for files that commonly carry secrets and should NOT be git-tracked.
const SECRET_PATTERNS: &[(&str, &str)] = &[
    (".env", "environment variable file"),
    (".env.local", "local environment overrides"),
    (".env.dev", "dev environment file"),
    (".env.prod", "production environment file"),
    (".env.staging", "staging environment file"),
    (".env.test", "test environment file"),
    (".env.example", "example env (may contain real values)"),
    (".envrc", "direnv file"),
    ("*.pem", "PEM certificate/key"),
    ("*.key", "private key"),
    ("*.p12", "PKCS#12 keystore"),
    ("*.pfx", "PFX certificate"),
    ("*.jks", "Java KeyStore"),
    ("*.keystore", "keystore file"),
    ("id_rsa", "RSA private key"),
    ("id_ed25519", "Ed25519 private key"),
    ("id_ecdsa", "ECDSA private key"),
    ("id_dsa", "DSA private key"),
    ("secrets.yaml", "secrets manifest"),
    ("secrets.yml", "secrets manifest"),
    ("secrets.json", "secrets manifest"),
    ("secrets.toml", "secrets manifest"),
    ("credentials", "credentials file"),
    ("credentials.json", "credentials file"),
    ("service-account.json", "GCP service account key"),
    ("*.secret", "generic secret file"),
    (".npmrc", "npm registry config (may contain tokens)"),
    (".pypirc", "PyPI credentials"),
    ("htpasswd", "Apache password file"),
    (".htpasswd", "Apache password file"),
    ("vault.hcl", "HashiCorp Vault config"),
    ("vault.yml", "HashiCorp Vault config"),
];

/// Check whether a file path matches any of the secret patterns (glob-style
/// suffix/basename matching only — no recursive glob expansion needed here
/// because `git ls-files` already returns full relative paths).
fn matches_secret_pattern(path: &str) -> Option<&'static str> {
    let basename = path.rsplit('/').next().unwrap_or(path);
    let lower_base = basename.to_ascii_lowercase();
    let lower_path = path.to_ascii_lowercase();

    for (pattern, label) in SECRET_PATTERNS {
        if pattern.starts_with("*.") {
            // Suffix glob: match any file with this extension.
            let ext = &pattern[1..]; // includes the dot
            if lower_base.ends_with(ext) {
                return Some(label);
            }
        } else {
            // Exact basename or known file name match.
            let lower_pat = pattern.to_ascii_lowercase();
            if lower_base == lower_pat || lower_path.ends_with(&format!("/{lower_pat}")) {
                return Some(label);
            }
            // Also match .env.* variants
            if lower_pat == ".env" && lower_base.starts_with(".env") {
                return Some(label);
            }
        }
    }
    None
}

fn looks_like_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0)
}

/// Lightweight content heuristics for likely leaked secrets.
fn detect_secret_content(content: &str) -> Option<&'static str> {
    if content.contains("-----BEGIN PRIVATE KEY-----")
        || content.contains("-----BEGIN RSA PRIVATE KEY-----")
        || content.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
        || content.contains("-----BEGIN EC PRIVATE KEY-----")
    {
        return Some("private key material in file content");
    }

    let lower = content.to_ascii_lowercase();
    if lower.contains("aws_secret_access_key")
        || lower.contains("aws_access_key_id")
        || lower.contains("x-api-key")
        || lower.contains("authorization: bearer ")
    {
        return Some("secret-like token in file content");
    }

    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("//") || t.starts_with(';') {
            continue;
        }
        let mut parts = if let Some((k, v)) = t.split_once('=') {
            Some((k.trim(), v.trim()))
        } else if let Some((k, v)) = t.split_once(':') {
            Some((k.trim(), v.trim()))
        } else {
            None
        };
        let Some((key, value)) = parts.take() else {
            continue;
        };
        if value.is_empty() || value == "\"\"" || value == "''" {
            continue;
        }
        let key_l = key.to_ascii_lowercase();
        if key_l.contains("password")
            || key_l.contains("passwd")
            || key_l.contains("token")
            || key_l.contains("api_key")
            || key_l.contains("apikey")
            || key_l.contains("secret")
            || key_l.contains("private_key")
            || key_l.contains("client_secret")
            || key_l.contains("access_key")
        {
            return Some("secret-like key/value in file content");
        }
    }

    None
}

fn read_secret_reason_for_file(git_root: &Path, rel_path: &str) -> Option<&'static str> {
    let full = git_root.join(rel_path);
    let bytes = fs::read(&full).ok()?;
    if !looks_like_text(&bytes) {
        return None;
    }
    let content = String::from_utf8_lossy(&bytes);
    detect_secret_content(&content)
}

fn git_ignores_path(git_root: &Path, rel_path: &str) -> Result<bool, String> {
    let out = std::process::Command::new("git")
        .args(["check-ignore", "-q", rel_path])
        .current_dir(git_root)
        .output()
        .map_err(|e| format!("git check-ignore failed for '{rel_path}': {e}"))?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(code) => Err(format!(
            "git check-ignore returned exit code {code} for '{rel_path}'"
        )),
        None => Err(format!(
            "git check-ignore terminated by signal for '{rel_path}'"
        )),
    }
}

fn run_secrets_audit() -> Result<(), String> {
    let s = Theme::for_stdout();
    let se = Theme::for_stderr();

    // ── 1. Check git is available and we're in a repo ──────────────────────
    let cwd =
        std::env::current_dir().map_err(|e| format!("failed to read current directory: {e}"))?;
    let git_root_out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output()
        .map_err(|e| format!("failed to run git: {e} (is git installed?)"))?;

    if !git_root_out.status.success() {
        return Err("not inside a git repository — secrets audit requires a git repo".to_string());
    }
    let git_root = PathBuf::from(
        String::from_utf8_lossy(&git_root_out.stdout)
            .trim()
            .to_string(),
    );

    // ── 2. Get all git-tracked files ────────────────────────────────────────
    let ls_files_out = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(&git_root)
        .output()
        .map_err(|e| format!("git ls-files failed: {e}"))?;

    let tracked_files: Vec<String> = String::from_utf8_lossy(&ls_files_out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();

    // ── 3. Check for staged (but not yet committed) secret files ────────────
    let staged_out = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&git_root)
        .output()
        .map_err(|e| format!("git diff --cached failed: {e}"))?;

    let staged_files: Vec<String> = String::from_utf8_lossy(&staged_out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();

    // ── 4. Collect findings ─────────────────────────────────────────────────
    let mut tracked_secrets: Vec<(String, &'static str)> = Vec::new();
    let mut staged_secrets: Vec<(String, &'static str)> = Vec::new();
    let mut tracked_content_secrets: Vec<(String, &'static str)> = Vec::new();
    let mut staged_content_secrets: Vec<(String, &'static str)> = Vec::new();
    let mut missing_gitignore: Vec<(&'static str, &'static str)> = Vec::new();

    for file in &tracked_files {
        if let Some(label) = matches_secret_pattern(file) {
            tracked_secrets.push((file.clone(), label));
        }
        if let Some(reason) = read_secret_reason_for_file(&git_root, file) {
            tracked_content_secrets.push((file.clone(), reason));
        }
    }
    for file in &staged_files {
        if let Some(label) = matches_secret_pattern(file) {
            staged_secrets.push((file.clone(), label));
        }
        if let Some(reason) = read_secret_reason_for_file(&git_root, file) {
            staged_content_secrets.push((file.clone(), reason));
        }
    }

    // Check whether common secret paths are ignored according to git ignore rules.
    let common_missing_checks: &[(&str, &str)] = &[
        (".env", ".env"),
        (".env.*", ".env.local"),
        (".env.local", ".env.local"),
        ("*.pem", "secrets/server.pem"),
        ("*.key", "secrets/server.key"),
        ("*.p12", "secrets/cert.p12"),
        ("*.pfx", "secrets/cert.pfx"),
        ("*.jks", "secrets/keystore.jks"),
        ("*.keystore", "secrets/app.keystore"),
        ("*.secret", "secrets/app.secret"),
        ("secrets.yaml", "secrets.yaml"),
        ("secrets.yml", "secrets.yml"),
        ("secrets.json", "secrets.json"),
        ("credentials.json", "credentials.json"),
        ("service-account.json", "service-account.json"),
        ("id_rsa", "id_rsa"),
        ("id_ed25519", "id_ed25519"),
        (".npmrc", ".npmrc"),
    ];
    for (pat, sample_path) in common_missing_checks {
        if !git_ignores_path(&git_root, sample_path)? {
            let label = SECRET_PATTERNS
                .iter()
                .find(|(p, _)| *p == *pat)
                .map(|(_, l)| *l)
                .unwrap_or("secret file pattern");
            missing_gitignore.push((pat, label));
        }
    }

    // ── 6. Print report ──────────────────────────────────────────────────────
    println!("{ti}tcon secrets audit{r}", ti = s.title, r = s.reset);
    println!(
        "{d}Scans git-tracked files and .gitignore for exposed secrets.{r}",
        d = s.dim,
        r = s.reset
    );
    println!();

    let has_issues = !tracked_secrets.is_empty()
        || !staged_secrets.is_empty()
        || !tracked_content_secrets.is_empty()
        || !staged_content_secrets.is_empty();

    // Tracked secret files (committed — most severe)
    if tracked_secrets.is_empty() {
        println!(
            "{ok}✓{r}  No secret files found in git-tracked files.",
            ok = s.ok,
            r = s.reset
        );
    } else {
        println!(
            "{bad}✗  {n} secret file(s) are tracked by git (CRITICAL):{r}",
            bad = se.bad,
            n = tracked_secrets.len(),
            r = s.reset
        );
        for (file, label) in &tracked_secrets {
            println!(
                "   {bad}{file}{r}  {d}({label}){r}",
                bad = se.bad,
                file = file,
                label = label,
                d = s.dim,
                r = s.reset
            );
        }
        println!();
        println!(
            "  {warn}Fix:{r} Remove from tracking with:",
            warn = se.warn,
            r = s.reset
        );
        for (file, _) in &tracked_secrets {
            println!("    git rm --cached {file}");
        }
        println!("  Then add to .gitignore and commit the removal.");
    }

    println!();

    // Tracked files with secret-like content (committed — most severe)
    if tracked_content_secrets.is_empty() {
        println!(
            "{ok}✓{r}  No secret-like content found in git-tracked text files.",
            ok = s.ok,
            r = s.reset
        );
    } else {
        println!(
            "{bad}✗  {n} git-tracked file(s) contain secret-like content (CRITICAL):{r}",
            bad = se.bad,
            n = tracked_content_secrets.len(),
            r = s.reset
        );
        for (file, reason) in &tracked_content_secrets {
            println!(
                "   {bad}{file}{r}  {d}({reason}){r}",
                bad = se.bad,
                d = s.dim,
                r = s.reset
            );
        }
        println!();
        println!(
            "  {warn}Fix:{r} Rotate leaked secrets, then remove from tracking if needed:",
            warn = se.warn,
            r = s.reset
        );
        for (file, _) in &tracked_content_secrets {
            println!("    git rm --cached {file}");
        }
        println!("  And replace values with env interpolation (e.g. ${{VAR}}).");
    }

    println!();

    // Staged secret files (about to be committed — severe)
    if !staged_secrets.is_empty() {
        println!(
            "{bad}✗  {n} secret file(s) are staged for commit (CRITICAL):{r}",
            bad = se.bad,
            n = staged_secrets.len(),
            r = s.reset
        );
        for (file, label) in &staged_secrets {
            println!(
                "   {bad}{file}{r}  {d}({label}){r}",
                bad = se.bad,
                file = file,
                label = label,
                d = s.dim,
                r = s.reset
            );
        }
        println!();
        println!(
            "  {warn}Fix:{r} Unstage and add to .gitignore:",
            warn = se.warn,
            r = s.reset
        );
        for (file, _) in &staged_secrets {
            println!("    git reset HEAD {file}");
        }
        println!();
    }

    // Staged files with secret-like content (about to be committed — severe)
    if !staged_content_secrets.is_empty() {
        println!(
            "{bad}✗  {n} staged file(s) contain secret-like content (CRITICAL):{r}",
            bad = se.bad,
            n = staged_content_secrets.len(),
            r = s.reset
        );
        for (file, reason) in &staged_content_secrets {
            println!(
                "   {bad}{file}{r}  {d}({reason}){r}",
                bad = se.bad,
                d = s.dim,
                r = s.reset
            );
        }
        println!();
        println!(
            "  {warn}Fix:{r} Unstage, scrub file content, and rotate any exposed credentials:",
            warn = se.warn,
            r = s.reset
        );
        for (file, _) in &staged_content_secrets {
            println!("    git reset HEAD {file}");
        }
        println!();
    }

    // Missing ignore coverage (advisory)
    if missing_gitignore.is_empty() {
        println!(
            "{ok}✓{r}  Git ignore rules cover all common secret file patterns.",
            ok = s.ok,
            r = s.reset
        );
    } else {
        println!(
            "{warn}⚠  {n} common secret pattern(s) are not ignored by git:{r}",
            warn = se.warn,
            n = missing_gitignore.len(),
            r = s.reset
        );
        println!(
            "  {d}Add rules to .gitignore (or your global excludes) to prevent accidental commits:{r}",
            d = s.dim,
            r = s.reset
        );
        for (pat, label) in &missing_gitignore {
            println!(
                "   {warn}{pat}{r}  {d}# {label}{r}",
                warn = se.warn,
                pat = pat,
                label = label,
                d = s.dim,
                r = s.reset
            );
        }
    }

    println!();

    // ── 7. Summary & exit code ───────────────────────────────────────────────
    if has_issues {
        Err(format!(
            "{} secret file(s) exposed in git tracking — see above to remediate",
            tracked_secrets.len()
                + staged_secrets.len()
                + tracked_content_secrets.len()
                + staged_content_secrets.len()
        ))
    } else {
        println!(
            "{ok}Secrets audit passed.{r}  No exposed secret files detected.",
            ok = s.ok,
            r = s.reset
        );
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (error_format, quiet, cmd, rest) = match parse_global_args(&args) {
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

    if cmd == "secrets" {
        let result = run_secrets_audit();
        if let Err(e) = result {
            print_error(error_format, &e);
            std::process::exit(1);
        }
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
        "validate" => parse_optional_entry(&rest).and_then(|entry| run_validate(&ws, entry, quiet)),
        "build" | "generate" => {
            parse_optional_entry(&rest).and_then(|entry| run_build(&ws, entry, quiet))
        }
        "check" => parse_optional_entry(&rest).and_then(|entry| run_check(&ws, entry, quiet)),
        "diff" => parse_optional_entry(&rest).and_then(|entry| run_diff(&ws, entry)),
        "status" => parse_optional_entry(&rest).and_then(|entry| run_status(&ws, entry)),
        "print" => parse_required_entry(&rest).and_then(|entry| run_print(&ws, entry)),
        "watch" => {
            parse_watch_args(&rest).and_then(|(entry, interval)| run_watch(&ws, entry, interval))
        }
        "init" => run_init(&ws, &rest),
        "secrets" => run_secrets_audit(),
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
