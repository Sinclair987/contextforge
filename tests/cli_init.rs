use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn init_creates_contextforge_toml_in_current_directory() {
    let temp = tempdir().expect("temporary directory");
    let config_path = temp.path().join("contextforge.toml");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created contextforge.toml"));

    let content = fs::read_to_string(config_path).expect("generated config file");
    assert!(content.contains("[scanner]"));
    assert!(content.contains("max_file_bytes"));
}

#[test]
fn init_refuses_to_overwrite_existing_config() {
    let temp = tempdir().expect("temporary directory");
    let config_path = temp.path().join("contextforge.toml");
    fs::write(&config_path, "existing = true\n").expect("seed config");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "configuration file already exists",
        ));

    let content = fs::read_to_string(config_path).expect("existing config file");
    assert_eq!(content, "existing = true\n");
}

#[test]
fn init_can_create_config_in_another_source_directory() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("source");
    fs::create_dir_all(&source).expect("source directory");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["init", "--source"])
        .arg(&source)
        .assert()
        .success();

    assert!(source.join("contextforge.toml").exists());
}
