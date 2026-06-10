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
    fs::write(
        source.join(".env.sample"),
        "SERVICE_API_KEY=demo-sensitive-value-123456\n",
    )
    .expect("sample env file");
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
