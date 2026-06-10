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
