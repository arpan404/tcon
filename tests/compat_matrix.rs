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
    let snapshot = Path::new(env!("CARGO_MANIFEST_DIR")).join("compat/v1/success");
    for case in fs::read_dir(&snapshot).expect("list success cases") {
        let case = case.expect("dir entry");
        let case_path = case.path();
        if !case_path.is_dir() {
            continue;
        }
        let name = case_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let root = mk_workspace(name);
        let entry_src =
            fs::read_to_string(case_path.join("entry.tcon")).expect("read entry fixture");
        write_file(&root.join(".tcon/entry.tcon"), &entry_src);

        let out = run(&root, &["build", "--entry", "entry.tcon"]);
        assert!(out.status.success(), "case {} failed: {:?}", name, out);

        let expected_path = fs::read_to_string(case_path.join("expected_path.txt"))
            .expect("read expected path")
            .trim()
            .to_string();
        let expected_output = fs::read_to_string(case_path.join("expected_output.txt"))
            .expect("read expected output");
        let actual = fs::read_to_string(root.join(expected_path)).expect("read generated output");
        assert_eq!(
            actual.trim_end(),
            expected_output.trim_end(),
            "case {}",
            name
        );
    }
}

#[test]
fn compatibility_matrix_failure_cases() {
    let snapshot = Path::new(env!("CARGO_MANIFEST_DIR")).join("compat/v1/failure");
    for case in fs::read_dir(&snapshot).expect("list failure cases") {
        let case = case.expect("dir entry");
        let case_path = case.path();
        if !case_path.is_dir() {
            continue;
        }
        let name = case_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let root = mk_workspace(name);
        let entry_src =
            fs::read_to_string(case_path.join("entry.tcon")).expect("read entry fixture");
        write_file(&root.join(".tcon/entry.tcon"), &entry_src);

        let expected_code = fs::read_to_string(case_path.join("expected_code.txt"))
            .expect("read expected code")
            .trim()
            .to_string();
        let expected_message = fs::read_to_string(case_path.join("expected_message.txt"))
            .expect("read expected message")
            .trim()
            .to_string();

        let out = run(
            &root,
            &["--error-format", "json", "build", "--entry", "entry.tcon"],
        );
        assert!(!out.status.success(), "case {} unexpectedly passed", name);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("\"code\":\"{}\"", expected_code)),
            "case {} missing code '{}': {}",
            name,
            expected_code,
            stderr
        );
        assert!(
            stderr.contains(&expected_message),
            "case {} missing message '{}': {}",
            name,
            expected_message,
            stderr
        );
    }
}
