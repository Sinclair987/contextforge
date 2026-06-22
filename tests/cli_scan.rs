use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn scan_reports_file_types_and_skip_reasons() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::create_dir_all(root.join("target")).expect("target directory");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules directory");

    fs::write(root.join("README.md"), "# Notes\n").expect("markdown file");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("rust file");
    fs::write(root.join("notes.txt"), "plain notes\n").expect("text file");
    fs::write(root.join("target/generated.rs"), "fn generated() {}\n").expect("ignored file");
    fs::write(
        root.join("node_modules/pkg/index.js"),
        "console.log('skip');\n",
    )
    .expect("ignored js");
    fs::write(root.join("image.bin"), [0_u8, 159, 146, 150]).expect("binary file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["scan", "--source"])
        .arg(root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanned files: 3"))
        .stdout(predicate::str::contains("Skipped files: 3"))
        .stdout(predicate::str::contains("Markdown: 1"))
        .stdout(predicate::str::contains("Rust: 1"))
        .stdout(predicate::str::contains("Text: 1"))
        .stdout(predicate::str::contains("Binary: 1"))
        .stdout(predicate::str::contains("Ignored directory: 2"));
}

#[test]
fn scan_uses_contextforge_toml_when_present() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(&source).expect("source directory");
    fs::write(source.join("large.txt"), "123456789").expect("large text file");
    fs::write(
        root.join("contextforge.toml"),
        "[scanner]\nmax_file_bytes = 4\nignore_patterns = [\"target\", \".git\", \"node_modules\"]\n",
    )
    .expect("config file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["scan", "--source"])
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanned files: 0"))
        .stdout(predicate::str::contains("Too large: 1"));
}

#[test]
fn scan_supports_repeatable_include_and_exclude_paths() {
    let temp = tempdir().expect("temporary directory");
    fs::create_dir_all(temp.path().join("keep/private")).expect("private directory");
    fs::create_dir_all(temp.path().join("drop")).expect("drop directory");
    fs::write(temp.path().join("keep/public.md"), "public context\n").expect("public file");
    fs::write(
        temp.path().join("keep/private/secret.md"),
        "private context\n",
    )
    .expect("private file");
    fs::write(temp.path().join("drop/other.md"), "other context\n").expect("other file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["scan", "--include", "keep", "--exclude", "keep/private"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanned files: 1"))
        .stdout(predicate::str::contains("Filtered path:"));
}
