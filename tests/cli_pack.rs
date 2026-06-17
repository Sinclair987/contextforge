use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn pack_generates_bundle_manifest_and_report_in_current_directory() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/rust.md"),
        "# Ownership\nRust ownership and borrowing help explain safe memory management. This context also covers move semantics, references, lifetimes, aliasing rules, compiler checks, and stable-anchor-after-preview-limit.\n",
    )
    .expect("markdown file");
    fs::write(source.join(".env.sample"), "SERVICE_API_KEY=test-key\n").expect("sample env file");
    fs::write(source.join("notes.txt"), "unrelated grocery list\n").expect("text file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args(["--goal", "ownership borrowing", "--budget", "120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generated context-bundle.md"))
        .stdout(predicate::str::contains("Generated context-manifest.json"))
        .stdout(predicate::str::contains("Generated context-report.md"));

    let bundle = fs::read_to_string(root.join("context-bundle.md")).expect("bundle");
    let manifest = fs::read_to_string(root.join("context-manifest.json")).expect("manifest");
    let report = fs::read_to_string(root.join("context-report.md")).expect("report");

    assert!(bundle.contains("ownership borrowing"));
    assert!(bundle.contains("Rust ownership and borrowing"));
    assert!(bundle.contains("stable-anchor-after-preview-limit"));
    assert!(bundle.contains("Privacy findings"));
    assert!(manifest.contains("\"used_tokens\""));
    assert!(manifest.contains("rust.md"));
    assert!(manifest.contains("API key"));
    assert!(report.contains("ContextForge Report"));
    assert!(report.contains("Selected chunks: 1"));
}

#[test]
fn pack_writes_outputs_to_requested_directory() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");
    let output = root.join("dist");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/rust.md"),
        "# Ownership\nRust ownership and borrowing belong in the selected context.\n",
    )
    .expect("markdown file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args([
            "--goal",
            "ownership borrowing",
            "--budget",
            "120",
            "--output-dir",
        ])
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("dist"));

    assert!(output.join("context-bundle.md").exists());
    assert!(output.join("context-manifest.json").exists());
    assert!(output.join("context-report.md").exists());
    assert!(!root.join("context-bundle.md").exists());
}

#[test]
fn pack_report_includes_selection_statistics() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/rust.md"),
        "# Ownership\nRust ownership and borrowing belong in the selected context.\n",
    )
    .expect("markdown file");
    fs::write(source.join(".env.sample"), "SERVICE_API_KEY=test-key\n").expect("sample env file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args(["--goal", "ownership borrowing", "--budget", "120"])
        .assert()
        .success();

    let report = fs::read_to_string(root.join("context-report.md")).expect("report");
    let manifest = fs::read_to_string(root.join("context-manifest.json")).expect("manifest");

    assert!(report.contains("Selected chunk types"));
    assert!(report.contains("markdown section: 1"));
    assert!(report.contains("Privacy severity counts"));
    assert!(manifest.contains("\"selected_chunk_types\""));
    assert!(manifest.contains("\"privacy_severity_counts\""));
}

#[test]
fn pack_manifest_explains_ranking_and_budget_decisions() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/ownership.md"),
        "ownership borrowing ownership borrowing makes one strong paragraph for ranking\n\nownership borrowing ownership borrowing creates another strong paragraph from the same file\n\nownership borrowing ownership borrowing would exceed the per-file budget guard\n",
    )
    .expect("ownership markdown");
    fs::write(
        source.join("docs/lifetime.md"),
        "ownership borrowing lifetime notes give another file a chance inside the same context budget\n",
    )
    .expect("lifetime markdown");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args(["--goal", "ownership borrowing", "--budget", "90"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Selected chunks:"))
        .stdout(predicate::str::contains("Excluded chunks:"));

    let manifest = fs::read_to_string(root.join("context-manifest.json")).expect("manifest");
    let report = fs::read_to_string(root.join("context-report.md")).expect("report");

    assert!(manifest.contains("\"score_breakdown\""));
    assert!(manifest.contains("\"selection_reason\""));
    assert!(manifest.contains("\"excluded_chunks\""));
    assert!(manifest.contains("\"per_file_budget_limit\""));
    assert!(manifest.contains("per-file budget limit"));
    assert!(report.contains("Excluded chunks:"));
}

#[test]
fn pack_dry_run_previews_selection_without_writing_outputs() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/ownership.md"),
        "ownership borrowing ownership borrowing selected context paragraph\n\nownership borrowing ownership borrowing another paragraph that should hit budget limits\n",
    )
    .expect("ownership markdown");
    fs::write(
        source.join("docs/lifetime.md"),
        "ownership borrowing lifetime note from a second file\n",
    )
    .expect("lifetime markdown");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args([
            "--goal",
            "ownership borrowing",
            "--budget",
            "70",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run: no files written"))
        .stdout(predicate::str::contains("Selected preview:"))
        .stdout(predicate::str::contains("Excluded preview:"))
        .stdout(predicate::str::contains("score"))
        .stdout(predicate::str::contains("per-file budget limit"));

    assert!(!root.join("context-bundle.md").exists());
    assert!(!root.join("context-manifest.json").exists());
    assert!(!root.join("context-report.md").exists());
}

#[test]
fn pack_redacts_selected_sensitive_lines_when_requested() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(&source).expect("source directory");
    fs::write(
        source.join("notes.md"),
        "# Deploy\nownership borrowing release note\nSERVICE_API_KEY=test-key\n",
    )
    .expect("notes file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args([
            "--goal",
            "ownership borrowing",
            "--budget",
            "160",
            "--redact",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Redaction: enabled"));

    let bundle = fs::read_to_string(root.join("context-bundle.md")).expect("bundle");
    let manifest = fs::read_to_string(root.join("context-manifest.json")).expect("manifest");

    assert!(bundle.contains("[REDACTED: API key]"));
    assert!(!bundle.contains("test-key"));
    assert!(manifest.contains("\"redacted\": true"));
}

#[test]
fn pack_can_fail_on_configured_privacy_severity() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(&source).expect("source directory");
    fs::write(
        source.join("notes.md"),
        "# Deploy\nownership borrowing release note\nSERVICE_API_KEY=test-key\n",
    )
    .expect("notes file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args([
            "--goal",
            "ownership borrowing",
            "--budget",
            "160",
            "--fail-on",
            "high",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("privacy gate failed"));
}
