use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn search_returns_relevant_file_path_line_score_and_preview() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::write(
        root.join("docs/rust.md"),
        "# Ownership\nRust ownership and borrowing prevent data races.\n",
    )
    .expect("markdown file");
    fs::write(root.join("notes.txt"), "unrelated grocery list\n").expect("text file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(root)
        .arg("ownership borrowing")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Search results for: ownership borrowing",
        ))
        .stdout(predicate::str::contains("rust.md"))
        .stdout(predicate::str::contains("lines 1-2"))
        .stdout(predicate::str::contains("markdown section"))
        .stdout(predicate::str::contains("title: Ownership"))
        .stdout(predicate::str::contains("score"))
        .stdout(predicate::str::contains("Rust ownership and borrowing"))
        .stdout(predicate::str::contains("notes.txt").not());
}
