use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

#[test]
fn version_flag_prints_package_version() {
    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("contextforge 0.1.0"));
}

#[test]
fn scan_uses_current_directory_when_source_is_omitted() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "# useful notes\n").expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanned files: 1"));
}

#[test]
fn pack_accepts_positional_goal_and_uses_practical_defaults() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("requirements.md"),
        "# Course project\ncourse project requirements and implementation details\n",
    )
    .expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "course project requirements"])
        .assert()
        .success()
        .stdout(predicate::str::contains("contextforge-output"));

    let output = temp.path().join("contextforge-output");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(output.join("context-manifest.json")).expect("manifest"),
    )
    .expect("manifest json");

    assert_eq!(manifest["budget"], 6000);
    assert!(output.join("context-bundle.md").exists());
}

#[test]
fn source_directory_config_is_loaded_when_running_elsewhere() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("source");
    let runner = temp.path().join("runner");
    fs::create_dir_all(&source).expect("source directory");
    fs::create_dir_all(&runner).expect("runner directory");
    fs::write(source.join("large.txt"), "123456789").expect("large file");
    fs::write(
        source.join("contextforge.toml"),
        "[scanner]\nmax_file_bytes = 4\n",
    )
    .expect("source config");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(&runner)
        .args(["scan", "--source"])
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanned files: 0"))
        .stdout(predicate::str::contains("Too large: 1"));
}
