use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn audit_reports_privacy_findings_with_severity_and_location() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::write(
        root.join(".env.sample"),
        "SERVICE_API_KEY=test-key\nCONTACT_EMAIL=support@example.invalid\n",
    )
    .expect("sample env file");
    fs::write(
        root.join("database.txt"),
        "DATABASE_URL=postgres://demo:demo-pass@example.invalid/app\n",
    )
    .expect("database note");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["audit", "--source"])
        .arg(root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Privacy findings: 3"))
        .stdout(predicate::str::contains("API key"))
        .stdout(predicate::str::contains("Database URL"))
        .stdout(predicate::str::contains("Email address"))
        .stdout(predicate::str::contains("High"))
        .stdout(predicate::str::contains("Low"))
        .stdout(predicate::str::contains(".env.sample"))
        .stdout(predicate::str::contains("line 1"));
}

#[test]
fn audit_can_emit_json_findings() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::write(root.join(".env.sample"), "SERVICE_API_KEY=test-key\n").expect("sample env file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["audit", "--source"])
        .arg(root)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"findings\""))
        .stdout(predicate::str::contains("\"kind\": \"API key\""))
        .stdout(predicate::str::contains("\"severity\": \"High\""));
}
