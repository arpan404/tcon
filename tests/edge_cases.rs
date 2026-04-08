//! Integration tests focused on boundary conditions and failure modes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn mk_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("tcon_edge_{name}_{nanos}_{}", std::process::id()));
    fs::create_dir_all(root.join(".tcon")).expect("create .tcon");
    root
}

fn mk_plain_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "tcon_edge_plain_{name}_{nanos}_{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create root");
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

fn assert_stderr_contains_json_code(stderr: &str, code: &str) {
    let needle = format!("\"code\":\"{code}\"");
    assert!(
        stderr.contains(&needle),
        "expected JSON diagnostic {needle} in:\n{stderr}"
    );
}

// --- Lexer ---

#[test]
fn lex_unterminated_string_maps_to_stable_code() {
    let root = mk_workspace("lex_unterm");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
export const bad = "no closing quote
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_LEX_UNTERMINATED_STRING");
}

#[test]
fn lex_unexpected_character_maps_to_stable_code() {
    let root = mk_workspace("lex_bad_char");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
export const bad = `nope`;
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_LEX_UNEXPECTED_CHAR");
}

// --- Loader / imports ---

#[test]
fn circular_import_detected() {
    let root = mk_workspace("import_cycle");
    write_file(
        &root.join(".tcon/a.tcon"),
        r#"
import { b } from "./b.tcon";
export const x = { n: 1 };
"#,
    );
    write_file(
        &root.join(".tcon/b.tcon"),
        r#"
import { x } from "./a.tcon";
export const y = x;
"#,
    );
    write_file(
        &root.join(".tcon/entry.tcon"),
        r#"
import { y } from "./b.tcon";
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ n: t.number().default(0) }).strict();
export const config = y;
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "entry.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_IMPORT_CYCLE");
}

#[test]
fn duplicate_export_in_same_file_fails() {
    let root = mk_workspace("dup_export");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const spec = { path: "other.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("duplicate export"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn imported_symbol_missing_in_dependency_fails() {
    let root = mk_workspace("import_missing");
    write_file(
        &root.join(".tcon/dep.tcon"),
        r#"export const onlyThis = 1;"#,
    );
    write_file(
        &root.join(".tcon/entry.tcon"),
        r#"
import { notThere } from "./dep.tcon";
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "entry.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found") && stderr.contains("notThere"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn duplicate_imported_name_from_two_files_fails() {
    let root = mk_workspace("dup_import_name");
    write_file(
        &root.join(".tcon/one.tcon"),
        r#"export const shared = t.object({ a: t.number().default(1) }).strict();"#,
    );
    write_file(
        &root.join(".tcon/two.tcon"),
        r#"export const shared = t.object({ b: t.number().default(2) }).strict();"#,
    );
    write_file(
        &root.join(".tcon/entry.tcon"),
        r#"
import { shared } from "./one.tcon";
import { shared } from "./two.tcon";
export const spec = { path: "out.json", format: "json" };
export const schema = shared;
export const config = { a: 1 };
"#,
    );
    let out = run(&root, &["build", "--entry", "entry.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("duplicate symbol"),
        "unexpected stderr: {stderr}"
    );
}

// --- Required exports & identifier resolution ---

#[test]
fn missing_schema_export_is_diagnosed() {
    let root = mk_workspace("no_schema");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const config = {};
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_EVAL_MISSING_EXPORT");
    assert!(stderr.contains("schema"), "unexpected stderr: {stderr}");
}

#[test]
fn circular_alias_while_resolving_export_fails() {
    let root = mk_workspace("circ_alias");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = s1;
export const s1 = s2;
export const s2 = s1;
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("circular identifier reference"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn unresolved_alias_in_export_chain_fails() {
    let root = mk_workspace("missing_alias");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = ghost;
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unresolved identifier") && stderr.contains("ghost"),
        "unexpected stderr: {stderr}"
    );
}

// --- Spec ---

#[test]
fn unsupported_spec_format_code() {
    let root = mk_workspace("bad_format");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.xml", format: "xml" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_SPEC_FORMAT");
}

#[test]
fn spec_path_required() {
    let root = mk_workspace("no_spec_path");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("spec.path is required"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn spec_mode_only_replace_supported() {
    let root = mk_workspace("bad_mode");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json", mode: "merge" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("only spec.mode=\"replace\""),
        "unexpected stderr: {stderr}"
    );
}

// --- Schema construction ---

#[test]
fn union_requires_two_variants() {
    let root = mk_workspace("union_one");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  x: t.union([t.string()]),
}).strict();
export const config = { x: "hi" };
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_PARSE_OR_SCHEMA");
}

#[test]
fn enum_requires_at_least_one_variant() {
    let root = mk_workspace("enum_empty");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  x: t.enum([]),
}).strict();
export const config = { x: "a" };
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_PARSE_OR_SCHEMA");
}

#[test]
fn strict_only_on_object_schema() {
    let root = mk_workspace("strict_on_string");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  x: t.string().strict(),
}).strict();
export const config = { x: "a" };
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_PARSE_OR_SCHEMA");
}

#[test]
fn literal_rejects_non_primitive_argument() {
    let root = mk_workspace("lit_obj");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  x: t.literal({ k: 1 }),
}).strict();
export const config = { x: { k: 1 } };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("t.literal() only supports primitive"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn config_rejects_call_expressions() {
    let root = mk_workspace("cfg_call");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = foo();
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported expression"),
        "unexpected stderr: {stderr}"
    );
}

// --- Validation ---

#[test]
fn strict_object_drops_unknown_keys_from_output() {
    let root = mk_workspace("strict_strip");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  keep: t.number().default(1),
}).strict();
export const config = { keep: 2, extra: "gone" };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("\"keep\""));
    assert!(
        !json.contains("extra"),
        "strict mode should omit unknown keys, got: {json}"
    );
}

#[test]
fn non_strict_object_retains_unknown_keys() {
    let root = mk_workspace("non_strict");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  keep: t.number().default(1),
});
export const config = { keep: 2, extra: "stay" };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("\"extra\""));
}

#[test]
fn optional_missing_field_omitted_from_json() {
    let root = mk_workspace("opt_miss");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  a: t.string().optional(),
  b: t.number().default(3),
}).strict();
export const config = { b: 4 };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(!json.contains("\"a\""));
    assert!(json.contains("\"b\": 4"));
}

#[test]
fn string_min_length_enforced_by_unicode_char_count() {
    let root = mk_workspace("str_min");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  s: t.string().min(3),
}).strict();
export const config = { s: "α" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("shorter than min"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn number_max_constraint_rejected() {
    let root = mk_workspace("num_max");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  n: t.number().max(10),
}).strict();
export const config = { n: 11 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("larger than max"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn required_object_field_missing_fails() {
    let root = mk_workspace("req_miss");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  need_me: t.string(),
}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("required string field is missing"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn invalid_global_error_format_exits_before_workspace() {
    let root = mk_plain_workspace("bad_fmt_flag");
    let out = run(
        &root,
        &["--error-format", "xml", "build"],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported --error-format"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn number_int_rejects_fractional() {
    let root = mk_workspace("num_int");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  n: t.number().int(),
}).strict();
export const config = { n: 3.5 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("expected integer"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn record_requires_object_value() {
    let root = mk_workspace("rec_arr");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  m: t.record(t.string()),
}).strict();
export const config = { m: [] };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("expected object for record"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn literal_accepts_true_and_number_in_json() {
    let root = mk_workspace("lit_prims");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  flag: t.literal(true),
  zero: t.literal(0),
}).strict();
export const config = { flag: true, zero: 0 };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("\"flag\": true"));
    assert!(json.contains("\"zero\": 0"));
}

#[test]
fn literal_null_validates_but_is_omitted_from_object_json_output() {
    let root = mk_workspace("lit_null_omit");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  empty: t.literal(null),
}).strict();
export const config = { empty: null };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert_eq!(
        json.trim(),
        "{}",
        "null-valued object fields are not emitted"
    );
}

#[test]
fn literal_null_preserved_inside_array_for_json() {
    let root = mk_workspace("lit_null_arr");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  items: t.array(t.literal(null)),
}).strict();
export const config = { items: [null, null] };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("null"));
}

// --- Emit ---

#[test]
fn env_format_rejects_null_leaf_value() {
    let root = mk_workspace("env_null");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.env", format: "env" };
export const schema = t.object({
  items: t.array(t.literal(null)),
}).strict();
export const config = { items: [null] };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot emit null"),
        "unexpected stderr: {stderr}"
    );
}

// --- CLI ---

#[test]
fn workspace_without_tcon_dir_fails() {
    let root = mk_plain_workspace("no_dot_tcon");
    let out = run(&root, &["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Missing .tcon directory"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn build_with_empty_tcon_tree_fails() {
    let root = mk_workspace("empty_tree");
    let out = run(&root, &["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no .tcon files found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn unknown_command_exits_nonzero() {
    let root = mk_workspace("unk");
    let out = run(&root, &["frobnicate"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown command"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn print_requires_entry_flag() {
    let root = mk_workspace("print_args");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"export const spec = { path: "a.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};"#,
    );
    let out = run(&root, &["print"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("print requires --entry"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn entry_file_not_found_under_tcon() {
    let root = mk_workspace("missing_entry");
    let out = run(&root, &["build", "--entry", "nope.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("File not found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn optional_entry_malformed_args_rejected() {
    let root = mk_workspace("bad_entry_args");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"export const spec = { path: "a.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};"#,
    );
    let out = run(&root, &["build", "--entry"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("expected optional --entry"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn init_unknown_preset_fails() {
    let root = mk_plain_workspace("bad_preset");
    fs::create_dir_all(root.join(".tcon")).unwrap();
    let out = run(&root, &["init", "--preset", "xml"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown preset"),
        "unexpected stderr: {stderr}"
    );
}

// --- Parser resilience ---

#[test]
fn trailing_commas_in_object_and_array() {
    let root = mk_workspace("trail_comma");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json", };
export const schema = t.object({
  items: t.array(t.number()),
}).strict();
export const config = { items: [1, 2, 3,], };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
}

#[test]
fn string_escape_sequences_roundtrip_in_config() {
    let root = mk_workspace("escapes");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  s: t.string(),
}).strict();
export const config = { s: "line1\nline2\t\"quote\"" };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("line1"));
    assert!(json.contains("line2"));
}
