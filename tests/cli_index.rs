use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn index_build_creates_local_index_and_reports_counts() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "local indexed knowledge\n").expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed files: 1"))
        .stdout(predicate::str::contains("Updated files: 1"));

    assert!(temp.path().join(".contextforge/index-v1.json").is_file());
}

#[test]
fn index_status_reports_fresh_index_without_rebuilding() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "local indexed knowledge\n").expect("source file");
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: fresh"))
        .stdout(predicate::str::contains("Version: 1"))
        .stdout(predicate::str::contains("Indexed files: 1"));
}

#[test]
fn index_status_reports_stale_after_a_source_file_changes() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("notes.md");
    fs::write(&source, "local indexed knowledge\n").expect("source file");
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success();
    fs::write(
        &source,
        "changed local indexed knowledge with a different size\n",
    )
    .expect("changed source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: stale"));
}

#[test]
fn index_clear_is_idempotent_and_preserves_source_files() {
    let temp = tempdir().expect("temporary directory");
    let source_file = temp.path().join("notes.md");
    fs::write(&source_file, "local indexed knowledge\n").expect("source file");
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success();

    for _ in 0..2 {
        Command::cargo_bin("contextforge")
            .expect("contextforge binary")
            .args(["index", "--source"])
            .arg(temp.path())
            .arg("clear")
            .assert()
            .success();
    }

    assert!(source_file.is_file());
    assert!(!temp.path().join(".contextforge/index-v1.json").exists());
}

#[test]
fn index_build_force_reextracts_unchanged_files() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "forced indexed knowledge\n").expect("source file");
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .args(["build", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated files: 1"))
        .stdout(predicate::str::contains("Reused files: 0"));
}

#[test]
fn index_status_verbose_prints_cached_extraction_warnings() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("broken.epub"), "not a zip archive").expect("broken EPUB");
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["index", "--source"])
        .arg(temp.path())
        .args(["status", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Extraction warnings: 1"))
        .stdout(predicate::str::contains("broken.epub"));
}
