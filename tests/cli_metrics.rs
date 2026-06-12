use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn metrics_reports_rust_project_summary() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::create_dir_all(root.join("tests")).expect("tests directory");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod parser;

pub trait Parse {
    fn parse(&self) -> Result<(), String>;
}

pub struct Parser;

pub enum Mode {
    Fast,
}

impl Parse for Parser {
    fn parse(&self) -> Result<(), String> {
        Ok(())
    }
}
"#,
    )
    .expect("library file");
    fs::write(
        root.join("tests/cli.rs"),
        "#[test]\nfn metrics_cli_test() {\n    assert!(true);\n}\n",
    )
    .expect("test file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["metrics", "--source"])
        .arg(root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust project metrics"))
        .stdout(predicate::str::contains("Rust files: 2"))
        .stdout(predicate::str::contains("Structs: 1"))
        .stdout(predicate::str::contains("Enums: 1"))
        .stdout(predicate::str::contains("Traits: 1"))
        .stdout(predicate::str::contains("Test functions: 1"))
        .stdout(predicate::str::contains("Requirement signals:"));
}

#[test]
fn metrics_can_emit_json() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("main file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["metrics", "--source"])
        .arg(root)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rust_files\": 1"))
        .stdout(predicate::str::contains("\"assessment\""))
        .stdout(predicate::str::contains("\"files\""));
}
