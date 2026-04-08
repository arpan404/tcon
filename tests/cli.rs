use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn mk_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("tcon_{name}_{nanos}_{}", std::process::id()));
    fs::create_dir_all(root.join(".tcon")).expect("create .tcon");
    root
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write file");
}

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tcon"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn tcon")
}

#[test]
fn build_and_check_json_drift() {
    let root = mk_workspace("json");
    write_file(
        &root.join(".tcon/server.tcon"),
        r#"
export const spec = { path: "server.json", format: "json", mode: "replace" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().min(1).max(65535).default(8080),
}).strict();
export const config = { port: 3000 };
"#,
    );

    let out = run(&root, &["build"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("server.json")).expect("read output");
    assert!(json.contains("\"port\": 3000"));

    let ok = run(&root, &["check"]);
    assert!(ok.status.success(), "check should pass");

    write_file(
        &root.join("server.json"),
        "{\n  \"host\": \"0.0.0.0\",\n  \"port\": 4000\n}\n",
    );
    let drift = run(&root, &["check"]);
    assert!(!drift.status.success(), "check should fail on drift");
}

#[test]
fn diff_reports_difference() {
    let root = mk_workspace("diff");
    write_file(
        &root.join(".tcon/server.tcon"),
        r#"
export const spec = { path: "server.json", format: "json" };
export const schema = t.object({ port: t.number().default(1) }).strict();
export const config = { port: 1 };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    write_file(&root.join("server.json"), "{\n  \"port\": 2\n}\n");

    let out = run(&root, &["diff"]);
    assert!(!out.status.success(), "diff should exit non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--- actual"), "{stdout}");
    assert!(stdout.contains("+++ expected"), "{stdout}");
}

#[test]
fn yaml_and_env_outputs_build() {
    let root = mk_workspace("formats");
    write_file(
        &root.join(".tcon/yaml.tcon"),
        r#"
export const spec = { path: "server.yaml", format: "yaml" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().default(8080),
}).strict();
export const config = { port: 3000 };
"#,
    );
    write_file(
        &root.join(".tcon/env.tcon"),
        r#"
export const spec = { path: "service.env", format: "env" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().default(8080),
}).strict();
export const config = { port: 3000 };
"#,
    );

    let out = run(&root, &["build"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let yaml = fs::read_to_string(root.join("server.yaml")).expect("read yaml");
    let env = fs::read_to_string(root.join("service.env")).expect("read env");
    assert!(yaml.contains("host:"));
    assert!(env.contains("HOST=0.0.0.0"));
}

#[test]
fn import_between_files_works() {
    let root = mk_workspace("import");
    write_file(
        &root.join(".tcon/base.tcon"),
        r#"
export const sharedSchema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().default(8080),
}).strict();
export const sharedConfig = { port: 3000 };
"#,
    );
    write_file(
        &root.join(".tcon/server.tcon"),
        r#"
import { sharedSchema, sharedConfig } from "./base.tcon";
export const spec = { path: "server.json", format: "json" };
export const schema = sharedSchema;
export const config = sharedConfig;
"#,
    );
    let out = run(&root, &["build", "--entry", "server.tcon"]);
    assert!(out.status.success(), "import build failed: {:?}", out);
    let json = fs::read_to_string(root.join("server.json")).expect("read json");
    assert!(json.contains("\"port\": 3000"));
}

#[test]
fn diagnostics_include_line_column_snippet() {
    let root = mk_workspace("diag");
    write_file(
        &root.join(".tcon/bad.tcon"),
        r#"
export const spec = { path: "x.json", format: "json" };
export const schema = t.object({ port: t.number().default(1) }).strict();
export const config = { port: @ };
"#,
    );

    let out = run(&root, &["build", "--entry", "bad.tcon"]);
    assert!(!out.status.success(), "build should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("-->"), "{stderr}");
    assert!(stderr.contains("^"), "{stderr}");
}
