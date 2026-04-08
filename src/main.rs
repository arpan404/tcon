mod diff;
mod emit;
mod eval;
mod model;
mod tcon;
mod validate;
mod workspace;

use crate::diff::describe_drift;
use crate::emit::json::to_pretty_json;
use crate::eval::{evaluate_config, evaluate_schema, evaluate_spec};
use crate::tcon::loader::load_program;
use crate::validate::validator::validate;
use crate::workspace::Workspace;
use std::env;
use std::fs;
use std::path::PathBuf;

fn usage() {
    eprintln!("tcon - typed configuration compiler");
    eprintln!("Usage:");
    eprintln!("  tcon build [--entry <file.tcon>]");
    eprintln!("  tcon check [--entry <file.tcon>]");
    eprintln!("  tcon print --entry <file.tcon>");
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

fn compile_entry(ws: &Workspace, entry_file: &PathBuf) -> Result<(PathBuf, String), String> {
    let (exports, file_name) = load_program(entry_file)?;
    let spec = evaluate_spec(&exports, &file_name)?;
    if spec.format != "json" {
        return Err(format!(
            "{}: only spec.format=\"json\" is supported in MVP",
            file_name
        ));
    }
    if let Some(mode) = &spec.mode && mode != "replace" {
        return Err(format!(
            "{}: only spec.mode=\"replace\" is supported in MVP",
            file_name
        ));
    }
    let schema = evaluate_schema(&exports, &file_name)?;
    let cfg = evaluate_config(&exports, &file_name)?;
    let normalized = validate(&schema, &cfg, &file_name)?;
    let output_path = ws.root.join(&spec.path);
    let rendered = format!("{}\n", to_pretty_json(&normalized));
    Ok((output_path, rendered))
}

fn run_build(ws: &Workspace, entry: Option<&str>) -> Result<(), String> {
    let entries = resolve_entries(ws, entry)?;
    if entries.is_empty() {
        return Err("no .tcon files found under .tcon/".to_string());
    }

    for entry_file in entries {
        let (output, rendered) = compile_entry(ws, &entry_file)?;
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

    let mut drift = 0usize;
    for entry_file in entries {
        let (output, expected) = compile_entry(ws, &entry_file)?;
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

fn run_print(ws: &Workspace, entry: &str) -> Result<(), String> {
    let path = ws.resolve_entry(entry)?;
    let (exports, _) = load_program(&path)?;
    println!("{exports:#?}");
    Ok(())
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        usage();
        std::process::exit(2);
    };
    let rest: Vec<String> = args.collect();

    let ws = match Workspace::discover(None) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let result = match cmd.as_str() {
        "build" => parse_optional_entry(&rest).and_then(|entry| run_build(&ws, entry)),
        "check" => parse_optional_entry(&rest).and_then(|entry| run_check(&ws, entry)),
        "print" => parse_required_entry(&rest).and_then(|entry| run_print(&ws, entry)),
        _ => {
            usage();
            Err(format!("unknown command: {cmd}"))
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
