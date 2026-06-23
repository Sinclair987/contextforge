use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

#[test]
fn pack_generates_bundle_manifest_and_report_in_source_output_directory() {
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
        .stdout(predicate::str::contains("context-bundle.md"))
        .stdout(predicate::str::contains("context-manifest.json"))
        .stdout(predicate::str::contains("context-report.md"));

    let output = source.join("contextforge-output");
    let bundle = fs::read_to_string(output.join("context-bundle.md")).expect("bundle");
    let manifest = fs::read_to_string(output.join("context-manifest.json")).expect("manifest");
    let report = fs::read_to_string(output.join("context-report.md")).expect("report");

    assert!(bundle.contains("ownership borrowing"));
    assert!(bundle.contains("Rust ownership and borrowing"));
    assert!(bundle.contains("stable-anchor-after-preview-limit"));
    assert!(!bundle.contains("Score:"));
    assert!(!bundle.contains("Selection reason:"));
    assert!(!bundle.contains("Privacy findings"));
    assert!(!bundle.contains("Budget:"));
    assert!(manifest.contains("\"used_tokens\""));
    assert!(manifest.contains("rust.md"));
    assert!(manifest.contains("API key"));
    assert!(report.contains("ContextForge Report"));
    assert!(report.contains("Selected spans: 1"));
}

#[test]
fn pack_bundle_groups_repeated_chunks_under_one_file_heading() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(source.join("docs")).expect("docs directory");
    fs::write(
        source.join("docs/source_alpha.md"),
        "ownership borrowing first selected block with enough context words\n\nownership borrowing second selected block with enough context words\n",
    )
    .expect("markdown file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(root)
        .args(["pack", "--source"])
        .arg(&source)
        .args(["--goal", "ownership borrowing", "--budget", "220"])
        .assert()
        .success();

    let bundle =
        fs::read_to_string(source.join("contextforge-output/context-bundle.md")).expect("bundle");
    let report =
        fs::read_to_string(source.join("contextforge-output/context-report.md")).expect("report");

    assert_eq!(bundle.matches("source_alpha.md").count(), 1);
    assert!(bundle.contains("Lines 1-3"));
    assert_eq!(bundle.matches("Lines ").count(), 1);
    assert!(!bundle.contains("paragraph"));
    assert!(report.contains("Selected spans: 1"));
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

    let output = source.join("contextforge-output");
    let report = fs::read_to_string(output.join("context-report.md")).expect("report");
    let manifest = fs::read_to_string(output.join("context-manifest.json")).expect("manifest");

    assert!(report.contains("Selected files: 1"));
    assert!(report.contains("Selected privacy findings:"));
    assert!(report.contains("Source privacy findings:"));
    assert!(report.contains("Extraction warnings:"));
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

    let output = source.join("contextforge-output");
    let manifest = fs::read_to_string(output.join("context-manifest.json")).expect("manifest");
    let report = fs::read_to_string(output.join("context-report.md")).expect("report");

    assert!(manifest.contains("\"score_breakdown\""));
    assert!(manifest.contains("\"excluded_chunks\""));
    assert!(manifest.contains("\"per_file_budget_limit\""));
    assert!(manifest.contains("automatic relevance floor") || manifest.contains("budget limit"));
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
        .stdout(predicate::str::contains("ownership.md"))
        .stdout(predicate::str::contains("reason:").not())
        .stdout(predicate::str::contains("score").not())
        .stdout(predicate::str::contains("tokens").not())
        .stdout(predicate::str::contains("| paragraph |").not())
        .stdout(predicate::str::contains("per-file budget limit").not());

    assert!(!root.join("context-bundle.md").exists());
    assert!(!root.join("context-manifest.json").exists());
    assert!(!root.join("context-report.md").exists());
}

#[test]
fn pack_dry_run_explain_restores_opt_in_diagnostics() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("notes.md"),
        "ownership borrowing ownership borrowing diagnostic context\n",
    )
    .expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args([
            "pack",
            "ownership borrowing",
            "--budget",
            "100",
            "--dry-run",
            "--explain",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("reason:"))
        .stdout(predicate::str::contains("score"))
        .stdout(predicate::str::contains("tokens"))
        .stdout(predicate::str::contains("per-file budget limit"));
}

#[test]
fn pack_redacts_selected_sensitive_lines_when_requested() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    let source = root.join("source");

    fs::create_dir_all(&source).expect("source directory");
    fs::write(
        source.join("notes.md"),
        "# Deploy\nownership borrowing release note\nSERVICE_API_KEY=real-secret-value\n",
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

    let output = source.join("contextforge-output");
    let bundle = fs::read_to_string(output.join("context-bundle.md")).expect("bundle");
    let manifest = fs::read_to_string(output.join("context-manifest.json")).expect("manifest");

    assert!(bundle.contains("[REDACTED: API key]"));
    assert!(!bundle.contains("real-secret-value"));
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
        "# Deploy\nownership borrowing release note\nSERVICE_API_KEY=real-secret-value\n",
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

#[test]
fn pack_skips_unreadable_documents_and_records_the_warning() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("source");
    fs::create_dir_all(&source).expect("source directory");
    fs::write(source.join("broken.pdf"), b"%PDF-1.4 broken document").expect("broken pdf");
    fs::write(
        source.join("requirements.md"),
        "course project requirements remain packable\n",
    )
    .expect("valid document");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["pack", "course project requirements", "--source"])
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains("Extraction warnings: 1"))
        .stderr(predicate::str::contains("broken.pdf"));

    let manifest = fs::read_to_string(source.join("contextforge-output/context-manifest.json"))
        .expect("manifest");
    assert!(manifest.contains("extraction_issues"));
    assert!(manifest.contains("broken.pdf"));
}

#[test]
fn pack_rejects_blank_goal_and_zero_budget() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "useful material\n").expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "   ", "--budget", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("goal must not be blank"));
}

#[test]
fn pack_does_not_write_an_empty_bundle_when_nothing_matches() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "unrelated grocery list\n").expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "ownership borrowing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no context matched"));

    assert!(!temp
        .path()
        .join("contextforge-output/context-bundle.md")
        .exists());
}

#[test]
fn pack_explains_why_no_readable_context_was_loaded() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("source");
    let config = temp.path().join("limits.toml");
    fs::create_dir_all(&source).expect("source directory");
    fs::write(source.join("oversized.md"), "123456789").expect("oversized text");
    fs::write(source.join("unsupported.mobi"), [0_u8, 1, 2, 3]).expect("binary file");
    fs::write(
        &config,
        "[scanner]\nmax_file_bytes = 4\nmax_document_bytes = 8\n",
    )
    .expect("test configuration");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .arg("--config")
        .arg(&config)
        .args(["pack", "missing context", "--source"])
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no readable context"))
        .stderr(predicate::str::contains("scanned: 0"))
        .stderr(predicate::str::contains("skipped: 2"))
        .stderr(predicate::str::contains("too large: 1"))
        .stderr(predicate::str::contains("binary: 1"));
}

#[test]
fn pack_rejects_budget_too_small_for_bundle_overhead() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "target\n").expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "target", "--budget", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("budget is too small"));
}

#[test]
fn pack_suggests_a_budget_that_can_fit_the_smallest_candidate() {
    let temp = tempdir().expect("temporary directory");
    fs::write(temp.path().join("notes.md"), "target context ".repeat(40)).expect("source file");

    let output = Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "target", "--budget", "20"])
        .output()
        .expect("pack output");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let minimum = stderr
        .split("use at least ")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<usize>().ok())
        .expect("suggested minimum budget");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "target", "--budget", &minimum.to_string()])
        .assert()
        .success();
}

#[test]
fn pack_excludes_weak_matches_below_the_automatic_relevance_floor() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("strong.md"),
        "alpha beta gamma delta alpha beta gamma delta\n",
    )
    .expect("strong source");
    fs::write(temp.path().join("weak.md"), "alpha appears once\n").expect("weak source");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "alpha beta gamma delta", "--budget", "600"])
        .assert()
        .success();

    let bundle = fs::read_to_string(temp.path().join("contextforge-output/context-bundle.md"))
        .expect("bundle");
    assert!(bundle.contains("strong.md"));
    assert!(!bundle.contains("weak.md"));
}

#[test]
fn pack_privacy_gate_only_checks_selected_context() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("requirements.md"),
        "course project requirements and implementation details\n",
    )
    .expect("selected source");
    fs::write(
        temp.path().join("unrelated.env"),
        "SERVICE_API_KEY=real-secret-value-123456\n",
    )
    .expect("unselected secret");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "course project requirements", "--fail-on", "high"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Selected privacy findings: 0"))
        .stdout(predicate::str::contains("Source privacy findings: 1"));
}

#[test]
fn pack_manifest_is_compact_and_uses_relative_paths() {
    let temp = tempdir().expect("temporary directory");
    fs::create_dir_all(temp.path().join("docs")).expect("docs directory");
    fs::write(
        temp.path().join("docs/requirements.md"),
        "course project requirements and implementation details\n",
    )
    .expect("source file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "course project requirements"])
        .assert()
        .success();

    let manifest_text = fs::read_to_string(
        temp.path()
            .join("contextforge-output/context-manifest.json"),
    )
    .expect("manifest");
    let manifest: Value = serde_json::from_str(&manifest_text).expect("manifest json");
    let selected = &manifest["selected_chunks"][0];

    assert_eq!(selected["path"], "docs/requirements.md");
    assert!(selected.get("text").is_none());
    assert!(selected.get("selection_reason").is_none());
    assert!(selected.get("preview").is_none());
    assert!(!manifest_text.contains(&temp.path().display().to_string()));
}

#[test]
fn pack_applies_include_and_exclude_paths_from_source_config() {
    let temp = tempdir().expect("temporary directory");
    fs::create_dir_all(temp.path().join("docs/private")).expect("private directory");
    fs::write(
        temp.path().join("contextforge.toml"),
        "[scanner]\ninclude_paths = [\"docs\"]\nexclude_paths = [\"docs/private\"]\n",
    )
    .expect("source config");
    fs::write(
        temp.path().join("docs/requirements.md"),
        "course project requirements public context\n",
    )
    .expect("public source");
    fs::write(
        temp.path().join("docs/private/secret.md"),
        "course project requirements private context\n",
    )
    .expect("private source");
    fs::write(
        temp.path().join("other.md"),
        "course project requirements unrelated root context\n",
    )
    .expect("root source");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["pack", "course project requirements"])
        .assert()
        .success();

    let bundle = fs::read_to_string(temp.path().join("contextforge-output/context-bundle.md"))
        .expect("bundle");
    assert!(bundle.contains("docs/requirements.md"));
    assert!(!bundle.contains("private context"));
    assert!(!bundle.contains("unrelated root context"));
}
