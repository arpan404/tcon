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
fn strict_object_rejects_unknown_keys() {
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
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_VALIDATE_STRICT_UNKNOWN_KEY");
    assert!(
        stderr.contains("unknown key(s) in strict object"),
        "{stderr}"
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
    let out = run(&root, &["--error-format", "xml", "build"]);
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

// --- Block comments ---

#[test]
fn lex_unterminated_block_comment_maps_to_stable_code() {
    let root = mk_workspace("block_comment");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"/* opens but never closes
export const spec = { path: "out.toml", format: "toml" };
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
    assert_stderr_contains_json_code(&stderr, "E_LEX_UNTERMINATED_BLOCK_COMMENT");
    assert!(stderr.contains("unterminated block comment"), "{stderr}");
}

#[test]
fn block_comment_closing_at_eof_is_accepted() {
    let root = mk_workspace("block_ok");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
/* trailing block comment */export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    assert!(run(&root, &["build"]).status.success());
}

// --- Deep imports ---

#[test]
fn deep_import_chain_fails_when_leaf_file_missing() {
    let root = mk_workspace("deep_import");
    write_file(
        &root.join(".tcon/level0.tcon"),
        r#"
import { v1 } from "./level1.tcon";
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ x: t.number().default(0) }).strict();
export const config = v1;
"#,
    );
    write_file(
        &root.join(".tcon/level1.tcon"),
        r#"
import { v2 } from "./level2.tcon";
export const v1 = v2;
"#,
    );
    write_file(
        &root.join(".tcon/level2.tcon"),
        r#"
import { v3 } from "./level3.tcon";
export const v2 = v3;
"#,
    );
    // level3.tcon intentionally absent

    let out = run(&root, &["build", "--entry", "level0.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to read") || stderr.contains("No such file"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("level3.tcon"),
        "expected missing path in stderr: {stderr}"
    );
}

#[test]
fn deep_import_chain_four_levels_resolves() {
    let root = mk_workspace("deep_ok");
    write_file(
        &root.join(".tcon/leaf.tcon"),
        "export const payload = { x: 42 };",
    );
    write_file(
        &root.join(".tcon/l2.tcon"),
        r#"
import { payload } from "./leaf.tcon";
export const v2 = payload;
"#,
    );
    write_file(
        &root.join(".tcon/l1.tcon"),
        r#"
import { v2 } from "./l2.tcon";
export const v1 = v2;
"#,
    );
    // Resolving `config = v1` follows v1 → v2 → payload; each hop must exist in this file's map.
    write_file(
        &root.join(".tcon/entry.tcon"),
        r#"
import { v1, v2 } from "./l1.tcon";
import { payload } from "./leaf.tcon";
export const spec = { path: "deep.json", format: "json" };
export const schema = t.object({ x: t.number().default(0) }).strict();
export const config = v1;
"#,
    );
    assert!(
        run(&root, &["build", "--entry", "entry.tcon"])
            .status
            .success()
    );
    let json = fs::read_to_string(root.join("deep.json")).expect("read");
    assert!(json.contains("\"x\": 42"));
}

// --- Emitter corner cases (TOML / properties) ---

#[test]
fn toml_rejects_null_in_normalized_value() {
    let root = mk_workspace("toml_null");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.toml", format: "toml" };
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
        stderr.contains("toml emitter cannot represent null"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn toml_emits_dotted_and_quoted_key_paths_in_structure() {
    let root = mk_workspace("toml_keys");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "app.toml", format: "toml" };
export const schema = t.object({
  "service.name": t.string().default("api"),
  meta: t.object({
    "build:no": t.number().int().default(1),
  }).strict(),
}).strict();
export const config = {
  "service.name": "worker",
  meta: { "build:no": 7 },
};
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let toml = fs::read_to_string(root.join("app.toml")).expect("read");
    assert!(
        toml.contains("service.name") && toml.contains("worker"),
        "unexpected toml:\n{toml}"
    );
    assert!(
        toml.contains("build:no") || toml.contains("[meta]"),
        "unexpected toml:\n{toml}"
    );
}

#[test]
fn properties_escapes_equals_newlines_and_backslash_in_values() {
    let root = mk_workspace("props_escape");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "app.properties", format: "properties" };
export const schema = t.object({
  "sec:tion": t.object({
    token: t.string(),
  }).strict(),
}).strict();
export const config = {
  "sec:tion": { token: "p=a\\ss\nword\t" },
};
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let props = fs::read_to_string(root.join("app.properties")).expect("read");
    assert!(
        props.contains("sec:tion.token="),
        "unexpected properties:\n{props}"
    );
    assert!(
        props.contains("\\=") || props.contains("\\\\"),
        "expected escapes in value:\n{props}"
    );
}

#[test]
fn properties_rejects_null_leaf() {
    let root = mk_workspace("props_null");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "app.properties", format: "properties" };
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
        stderr.contains("properties emitter cannot represent null"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn schema_default_must_match_field_type() {
    let root = mk_workspace("bad_default");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  n: t.number().default("not-a-number"),
}).strict();
export const config = { n: 1 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("expected number") && stderr.contains("<schema.default>"),
        "{stderr}"
    );
}

#[test]
fn spec_rejects_unknown_keys() {
    let root = mk_workspace("spec_unknown");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json", typoKey: "oops" };
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
    assert_stderr_contains_json_code(&stderr, "E_SPEC_UNKNOWN_KEY");
    assert!(stderr.contains("unknown key in spec object"), "{stderr}");
}

#[test]
fn negative_numeric_literals_in_config() {
    let root = mk_workspace("neg_num");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  offset: t.number().default(-1),
}).strict();
export const config = { offset: -42 };
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("-42"));
}

#[test]
fn lone_minus_is_lex_error() {
    let root = mk_workspace("lone_minus");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = { a: - };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unexpected character '-'") || stderr.contains("unexpected token"),
        "{stderr}"
    );
}

// --- Parser / workspace / YAML quality ---

#[test]
fn duplicate_object_key_is_rejected() {
    let root = mk_workspace("dup_key");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  a: t.number(),
  a: t.string(),
}).strict();
export const config = { a: 1 };
"#,
    );
    let out = run(
        &root,
        &["--error-format", "json", "build", "--entry", "x.tcon"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_contains_json_code(&stderr, "E_PARSE_OR_SCHEMA");
    assert!(
        stderr.contains("duplicate key in object literal"),
        "{stderr}"
    );
}

#[test]
fn duplicate_object_key_detects_ident_and_string_same_name() {
    let root = mk_workspace("dup_key_mixed");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  x: t.number(),
  "x": t.string(),
}).strict();
export const config = { x: 1 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("duplicate key in object literal"),
        "{stderr}"
    );
}

#[test]
fn import_list_must_not_be_empty() {
    let root = mk_workspace("empty_import");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
import { } from "./nope.tcon";
export const spec = { path: "out.json", format: "json" };
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
    assert_stderr_contains_json_code(&stderr, "E_PARSE_OR_SCHEMA");
    assert!(
        stderr.contains("import requires at least one binding"),
        "{stderr}"
    );
}

#[test]
fn spec_path_empty_string_rejected() {
    let root = mk_workspace("empty_spec_path");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("spec.path must not be empty"), "{stderr}");
}

#[test]
fn absolute_entry_path_must_exist_on_disk() {
    let root = mk_workspace("abs_miss");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let missing = std::env::temp_dir().join(format!(
        "tcon_abs_missing_{}_{}.tcon",
        std::process::id(),
        nanos
    ));
    assert!(!missing.exists(), "precondition: path should not exist");
    let entry = missing.to_str().expect("utf8 temp path");
    let out = run(&root, &["build", "--entry", entry]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("File not found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn yaml_emits_quoted_keys_for_reserved_and_special_characters() {
    let root = mk_workspace("yaml_keys");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.yaml", format: "yaml" };
export const schema = t.object({
  ok_plain: t.string(),
  "meta:kv": t.string(),
  "has space": t.string(),
  "true": t.string(),
}).strict();
export const config = {
  ok_plain: "a",
  "meta:kv": "b",
  "has space": "c",
  "true": "d",
};
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let yaml = fs::read_to_string(root.join("out.yaml")).expect("read");
    assert!(
        yaml.contains("\"meta:kv\""),
        "colon in key should force quotes:\n{yaml}"
    );
    assert!(
        yaml.contains("\"has space\""),
        "space in key should force quotes:\n{yaml}"
    );
    assert!(
        yaml.contains("\"true\""),
        "reserved word keys should be quoted:\n{yaml}"
    );
    assert!(
        yaml.contains("ok_plain:") && !yaml.contains("\"ok_plain\""),
        "simple keys stay unquoted:\n{yaml}"
    );
}

// ─── New robustness guards ────────────────────────────────────────────────────

// --- Lexer: multi-decimal numbers ---

#[test]
fn number_with_multiple_decimal_points_rejected() {
    let root = mk_workspace("multi_dot_num");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ v: t.number() }).strict();
export const config = { v: 1.2.3 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "build should fail on 1.2.3");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("multiple decimal") || stderr.contains("unexpected"),
        "expected decimal-point error: {stderr}"
    );
}

// --- spec.path safety ---

#[test]
fn spec_path_absolute_rejected() {
    let root = mk_workspace("abs_spec_path");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "/etc/tcon_output.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "absolute spec.path must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("relative path") || stderr.contains("absolute"),
        "expected absolute-path error: {stderr}"
    );
}

#[test]
fn spec_path_traversal_rejected() {
    let root = mk_workspace("traversal_spec_path");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "../../etc/secrets.json", format: "json" };
export const schema = t.object({}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "path traversal must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("..") || stderr.contains("traversal"),
        "expected traversal error: {stderr}"
    );
}

// --- Duplicate output path across entries ---

#[test]
fn duplicate_output_path_across_entries_fails() {
    let root = mk_workspace("dup_output");
    write_file(
        &root.join(".tcon/a.tcon"),
        r#"
export const spec = { path: "shared.json", format: "json" };
export const schema = t.object({ n: t.number().default(1) }).strict();
export const config = {};
"#,
    );
    write_file(
        &root.join(".tcon/b.tcon"),
        r#"
export const spec = { path: "shared.json", format: "json" };
export const schema = t.object({ n: t.number().default(2) }).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build"]);
    assert!(!out.status.success(), "two .tcon files writing same path must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("collision") || stderr.contains("shared.json"),
        "expected collision error: {stderr}"
    );
}

// --- Duplicate enum variants ---

#[test]
fn enum_duplicate_variant_rejected() {
    let root = mk_workspace("dup_enum");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  mode: t.enum(["dev", "prod", "dev"]),
}).strict();
export const config = { mode: "dev" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "duplicate enum variant must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("duplicate variant") || stderr.contains("duplicate"),
        "expected duplicate-variant error: {stderr}"
    );
}

// --- Inverted min/max bounds ---

#[test]
fn number_min_greater_than_max_rejected() {
    let root = mk_workspace("inv_num_bounds");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  port: t.number().min(100).max(10),
}).strict();
export const config = { port: 50 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "inverted number bounds must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("inverted") || stderr.contains("bounds"),
        "expected inverted-bounds error: {stderr}"
    );
}

#[test]
fn string_min_greater_than_max_rejected() {
    let root = mk_workspace("inv_str_bounds");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  name: t.string().min(10).max(3),
}).strict();
export const config = { name: "hi" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "inverted string bounds must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("inverted") || stderr.contains("bounds"),
        "expected inverted-bounds error: {stderr}"
    );
}

// --- ENV key normalization collision ---

#[test]
fn env_key_collision_rejected() {
    let root = mk_workspace("env_key_coll");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.env", format: "env" };
export const schema = t.object({
  a_b: t.string().default("one"),
  "a-b": t.string().default("two"),
}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "env key collision must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("collision") || stderr.contains("A_B"),
        "expected collision error mentioning key: {stderr}"
    );
}

// --- Enum error message shows allowed variants ---

#[test]
fn enum_error_shows_allowed_variants() {
    let root = mk_workspace("enum_msg");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  env: t.enum(["development", "staging", "production"]),
}).strict();
export const config = { env: "local" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("enum value not in allowed variants"),
        "should still contain base message: {stderr}"
    );
    assert!(
        stderr.contains("development") && stderr.contains("production"),
        "error should list allowed variants: {stderr}"
    );
    assert!(
        stderr.contains("local"),
        "error should echo the rejected value: {stderr}"
    );
}

// --- TOML: keys with special characters are properly quoted ---

#[test]
fn toml_keys_with_special_chars_are_quoted() {
    let root = mk_workspace("toml_quote_keys");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.toml", format: "toml" };
export const schema = t.object({
  "has.dot": t.string().default("dotval"),
  "has space": t.string().default("spaceval"),
  simple: t.string().default("ok"),
}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let toml = fs::read_to_string(root.join("out.toml")).expect("read toml");
    // Keys with dots or spaces must be quoted in TOML
    assert!(
        toml.contains("\"has.dot\"") || toml.contains("'has.dot'"),
        "dot key should be quoted:\n{toml}"
    );
    assert!(
        toml.contains("\"has space\"") || toml.contains("'has space'"),
        "space key should be quoted:\n{toml}"
    );
    // Simple bare key stays unquoted
    assert!(
        toml.contains("simple ="),
        "bare key should stay unquoted:\n{toml}"
    );
}

// --- Richer type mismatch messages ---

#[test]
fn type_mismatch_error_shows_actual_type() {
    let root = mk_workspace("type_mismatch");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  name: t.string(),
  count: t.number(),
}).strict();
export const config = { name: 42, count: "hello" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Fields are visited alphabetically: count (number, got string) comes first
    assert!(
        stderr.contains("expected number, got string"),
        "error should show expected type and actual type: {stderr}"
    );
}

// ─── Enterprise features ──────────────────────────────────────────────────────

// --- Multi-error collection ---

#[test]
fn multi_error_validation_reports_all_fields() {
    let root = mk_workspace("multi_err");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  host: t.string(),
  port: t.number().int(),
  mode: t.enum(["dev", "prod"]),
}).strict();
export const config = { host: 123, port: "not-a-number", mode: "staging" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // All three field errors should appear in the same output
    assert!(
        stderr.contains("expected string"),
        "missing host error: {stderr}"
    );
    assert!(
        stderr.contains("expected number"),
        "missing port error: {stderr}"
    );
    assert!(
        stderr.contains("enum value not in allowed variants"),
        "missing mode error: {stderr}"
    );
}

#[test]
fn multi_error_summary_line_shows_count() {
    let root = mk_workspace("multi_err_count");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  a: t.string(),
  b: t.number(),
  c: t.boolean(),
}).strict();
export const config = { a: 1, b: "two", c: "three" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Summary line should mention the count
    assert!(
        stderr.contains("3 error") || stderr.contains("errors"),
        "expected error count in summary: {stderr}"
    );
}

// --- ENV variable interpolation ---

#[test]
fn secret_modifier_only_valid_on_string_schema() {
    let root = mk_workspace("secret_non_string_schema");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  password: t.number().secret(),
}).strict();
export const config = { password: 1234 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "non-string .secret() must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(".secret() only valid on string schema"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn secret_string_rejects_hardcoded_literal() {
    let root = mk_workspace("secret_hardcoded");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  password: t.string().secret(),
}).strict();
export const config = { password: "hunter2" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "hardcoded secret must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("secret field must be sourced from an environment variable"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn secret_string_accepts_env_interpolation() {
    let root = mk_workspace("secret_env_ok");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  password: t.string().secret(),
}).strict();
export const config = { password: "${APP_PASSWORD}" };
"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_tcon"))
        .args(["build", "--entry", "x.tcon"])
        .current_dir(&root)
        .env("APP_PASSWORD", "super-secret")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "secret interpolation should pass: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(json.contains("super-secret"), "{json}");
}

#[test]
fn env_interpolation_resolves_set_variable() {
    let root = mk_workspace("env_interp_ok");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ host: t.string() }).strict();
export const config = { host: "${TCON_TEST_HOST}" };
"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_tcon"))
        .args(["build", "--entry", "x.tcon"])
        .current_dir(&root)
        .env("TCON_TEST_HOST", "prod.example.com")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(
        json.contains("prod.example.com"),
        "interpolated value missing: {json}"
    );
}

#[test]
fn env_interpolation_uses_default_when_var_unset() {
    let root = mk_workspace("env_interp_default");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ host: t.string() }).strict();
export const config = { host: "${TCON_ABSENT_VAR_XYZ:fallback.local}" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(
        json.contains("fallback.local"),
        "default value missing: {json}"
    );
}

#[test]
fn env_interpolation_fails_when_var_unset_and_no_default() {
    let root = mk_workspace("env_interp_missing");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ secret: t.string() }).strict();
export const config = { secret: "${DEFINITELY_NOT_SET_VAR_12345}" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "should fail when env var is missing");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DEFINITELY_NOT_SET_VAR_12345"),
        "missing var name in error: {stderr}"
    );
    assert!(
        stderr.contains("not set") || stderr.contains("unset"),
        "should say variable is not set: {stderr}"
    );
}

#[test]
fn env_interpolation_in_schema_default() {
    let root = mk_workspace("env_interp_schema_default");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  region: t.string().default("${TCON_REGION:us-east-1}"),
}).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(
        json.contains("us-east-1"),
        "interpolated default missing: {json}"
    );
}

#[test]
fn env_interpolation_escaped_dollar_brace() {
    let root = mk_workspace("env_interp_escape");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ template: t.string() }).strict();
export const config = { template: "$${PLACEHOLDER}" };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    assert!(
        json.contains("${PLACEHOLDER}"),
        "escaped placeholder should be literal: {json}"
    );
}

// --- t.extend() schema composition ---

#[test]
fn schema_extend_merges_base_fields() {
    let root = mk_workspace("schema_extend");
    write_file(
        &root.join(".tcon/base.tcon"),
        r#"
export const baseSchema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();
"#,
    );
    write_file(
        &root.join(".tcon/server.tcon"),
        r#"
import { baseSchema } from "./base.tcon";
export const spec = { path: "server.json", format: "json" };
export const schema = t.object({
  name: t.string().default("my-service"),
}).strict().extend(baseSchema);
export const config = { port: 3000 };
"#,
    );
    let out = run(&root, &["build", "--entry", "server.tcon"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("server.json")).expect("read");
    assert!(json.contains("\"port\": 3000"), "port missing: {json}");
    assert!(json.contains("\"host\": \"0.0.0.0\""), "host missing: {json}");
    assert!(json.contains("\"name\": \"my-service\""), "name missing: {json}");
}

#[test]
fn schema_extend_base_fields_do_not_override_child() {
    let root = mk_workspace("schema_extend_precedence");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const baseSchema = t.object({
  timeout: t.number().int().default(30),
  retries: t.number().int().default(3),
}).strict();
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({
  timeout: t.number().int().default(60),
}).strict().extend(baseSchema);
export const config = {};
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(out.status.success(), "build failed: {:?}", out);
    let json = fs::read_to_string(root.join("out.json")).expect("read");
    // Child's timeout (60) must win over base's timeout (30)
    assert!(json.contains("\"timeout\": 60"), "child timeout should win: {json}");
    assert!(json.contains("\"retries\": 3"), "retries from base missing: {json}");
}

#[test]
fn schema_extend_requires_object_schema() {
    let root = mk_workspace("schema_extend_bad_arg");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ n: t.number() }).strict().extend(t.string());
export const config = { n: 1 };
"#,
    );
    let out = run(&root, &["build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "extend with non-object should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("object schema"),
        "expected object-schema error: {stderr}"
    );
}

// --- tcon status command ---

#[test]
fn status_shows_ok_when_all_up_to_date() {
    let root = mk_workspace("status_ok");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "status.json", format: "json" };
export const schema = t.object({ v: t.number().default(1) }).strict();
export const config = {};
"#,
    );
    assert!(run(&root, &["build"]).status.success());
    let out = run(&root, &["status"]);
    assert!(out.status.success(), "status should succeed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok"), "should show ok: {stdout}");
    assert!(
        stdout.contains("1/1"),
        "should show 1/1 summary: {stdout}"
    );
}

#[test]
fn status_shows_missing_when_output_absent() {
    let root = mk_workspace("status_miss");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "missing.json", format: "json" };
export const schema = t.object({ v: t.number().default(1) }).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["status"]);
    assert!(!out.status.success(), "status should fail when output missing");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("miss") || stdout.contains("missing"),
        "should show missing: {stdout}"
    );
}

#[test]
fn status_handles_compile_error_gracefully() {
    let root = mk_workspace("status_err");
    write_file(
        &root.join(".tcon/bad.tcon"),
        r#"
export const spec = { path: "bad.json", format: "json" };
export const schema = t.object({ n: t.number() }).strict();
export const config = { n: @ };
"#,
    );
    write_file(
        &root.join(".tcon/good.tcon"),
        r#"
export const spec = { path: "good.json", format: "json" };
export const schema = t.object({ n: t.number().default(1) }).strict();
export const config = {};
"#,
    );
    assert!(
        run(&root, &["build", "--entry", "good.tcon"])
            .status
            .success()
    );
    // status should report the error for bad.tcon but not crash
    let out = run(&root, &["status"]);
    assert!(!out.status.success(), "status exits non-zero when any entry has an error");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("error") || stdout.contains("bad.tcon"),
        "should mention the failing entry: {stdout}"
    );
    assert!(
        stdout.contains("ok") || stdout.contains("good.tcon"),
        "should still report good.tcon as ok: {stdout}"
    );
}

// --- --quiet / -q flag ---

#[test]
fn quiet_flag_suppresses_stdout_on_build() {
    let root = mk_workspace("quiet_build");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ v: t.number().default(1) }).strict();
export const config = {};
"#,
    );
    let out = run(&root, &["--quiet", "build"]);
    assert!(out.status.success(), "quiet build failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.is_empty(),
        "stdout should be empty with --quiet: '{stdout}'"
    );
    assert!(root.join("out.json").exists(), "file should still be written");
}

#[test]
fn quiet_flag_still_emits_errors_to_stderr() {
    let root = mk_workspace("quiet_err");
    write_file(
        &root.join(".tcon/x.tcon"),
        r#"
export const spec = { path: "out.json", format: "json" };
export const schema = t.object({ n: t.number() }).strict();
export const config = { n: "wrong" };
"#,
    );
    let out = run(&root, &["-q", "build", "--entry", "x.tcon"]);
    assert!(!out.status.success(), "should fail on type mismatch");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.is_empty(), "stdout should be silent: '{stdout}'");
    assert!(
        stderr.contains("expected number"),
        "error should still appear on stderr: {stderr}"
    );
}
