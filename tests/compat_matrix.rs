use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct SuccessCase {
    name: &'static str,
    entry: &'static str,
    source: &'static str,
    output: &'static str,
    expected: &'static str,
}

struct FailureCase {
    name: &'static str,
    entry: &'static str,
    source: &'static str,
    expected_error: &'static str,
}

fn mk_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("tcon_compat_{name}_{nanos}_{}", std::process::id()));
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
fn compatibility_matrix_success_cases() {
    let cases = [
        SuccessCase {
            name: "json_defaults",
            entry: "json_case.tcon",
            source: r#"
export const spec = { path: "json_case.json", format: "json" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();
export const config = { port: 3000 };
"#,
            output: "json_case.json",
            expected: "{\n  \"host\": \"0.0.0.0\",\n  \"port\": 3000\n}\n",
        },
        SuccessCase {
            name: "yaml_format",
            entry: "yaml_case.tcon",
            source: r#"
export const spec = { path: "yaml_case.yaml", format: "yaml" };
export const schema = t.object({ port: t.number().default(8080) }).strict();
export const config = { port: 3000 };
"#,
            output: "yaml_case.yaml",
            expected: "port: 3000\n",
        },
        SuccessCase {
            name: "env_format",
            entry: "env_case.tcon",
            source: r#"
export const spec = { path: "env_case.env", format: "env" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().default(8080),
}).strict();
export const config = { port: 3000 };
"#,
            output: "env_case.env",
            expected: "HOST=0.0.0.0\nPORT=3000\n",
        },
        SuccessCase {
            name: "toml_format",
            entry: "toml_case.tcon",
            source: r#"
export const spec = { path: "toml_case.toml", format: "toml" };
export const schema = t.object({
  app: t.object({
    host: t.string().default("0.0.0.0"),
    port: t.number().default(8080),
  }).strict(),
}).strict();
export const config = { app: { port: 3000 } };
"#,
            output: "toml_case.toml",
            expected: "\n[app]\nhost = \"0.0.0.0\"\nport = 3000\n",
        },
        SuccessCase {
            name: "properties_format",
            entry: "properties_case.tcon",
            source: r#"
export const spec = { path: "properties_case.properties", format: "properties" };
export const schema = t.object({
  app: t.object({
    host: t.string().default("0.0.0.0"),
    port: t.number().default(8080),
  }).strict(),
}).strict();
export const config = { app: { port: 3000 } };
"#,
            output: "properties_case.properties",
            expected: "app.host=0.0.0.0\napp.port=3000\n",
        },
    ];

    for case in cases {
        let root = mk_workspace(case.name);
        write_file(&root.join(".tcon").join(case.entry), case.source);
        let out = run(&root, &["build", "--entry", case.entry]);
        assert!(out.status.success(), "case {} failed: {:?}", case.name, out);
        let actual = fs::read_to_string(root.join(case.output)).expect("read output");
        assert_eq!(actual, case.expected, "case {}", case.name);
    }
}

#[test]
fn compatibility_matrix_failure_cases() {
    let cases = [
        FailureCase {
            name: "enum_invalid",
            entry: "enum_invalid.tcon",
            source: r#"
export const spec = { path: "enum_invalid.json", format: "json" };
export const schema = t.object({ mode: t.enum(["dev", "prod"]) }).strict();
export const config = { mode: "staging" };
"#,
            expected_error: "enum value not in allowed variants",
        },
        FailureCase {
            name: "bad_env_extension",
            entry: "bad_env_ext.tcon",
            source: r#"
export const spec = { path: "bad_env.txt", format: "env" };
export const schema = t.object({ k: t.string().default("v") }).strict();
export const config = {};
"#,
            expected_error: "env output path must end with '.env'",
        },
        FailureCase {
            name: "bad_properties_extension",
            entry: "bad_props_ext.tcon",
            source: r#"
export const spec = { path: "bad_props.txt", format: "properties" };
export const schema = t.object({ k: t.string().default("v") }).strict();
export const config = {};
"#,
            expected_error: "properties output path must end with '.properties'",
        },
    ];

    for case in cases {
        let root = mk_workspace(case.name);
        write_file(&root.join(".tcon").join(case.entry), case.source);
        let out = run(&root, &["build", "--entry", case.entry]);
        assert!(
            !out.status.success(),
            "case {} unexpectedly passed",
            case.name
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(case.expected_error),
            "case {} missing error '{}': {}",
            case.name,
            case.expected_error,
            stderr
        );
    }
}
