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
