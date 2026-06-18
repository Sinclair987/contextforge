use assert_cmd::Command;
use predicates::prelude::*;
use std::{fs, io::Write, path::Path};
use tempfile::tempdir;
use zip::{write::SimpleFileOptions, ZipWriter};

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

#[test]
fn search_can_read_pdf_and_docx_documents() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    write_simple_pdf(&root.join("guide.pdf"), "Ownership borrowing PDF notes");
    write_simple_docx(&root.join("brief.docx"), "Ownership borrowing DOCX notes");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(root)
        .arg("ownership borrowing")
        .assert()
        .success()
        .stdout(predicate::str::contains("guide.pdf"))
        .stdout(predicate::str::contains("PDF notes"))
        .stdout(predicate::str::contains("brief.docx"))
        .stdout(predicate::str::contains("DOCX notes"));
}

#[test]
fn search_can_read_common_data_markup_and_code_formats() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("scripts")).expect("scripts directory");
    fs::write(
        root.join("settings.yaml"),
        "feature_budget:\n  description: ranking budget yaml notes\n",
    )
    .expect("yaml file");
    fs::write(
        root.join("data.csv"),
        "name,description\ncontextforge,ranking budget csv notes\n",
    )
    .expect("csv file");
    fs::write(
        root.join("page.html"),
        "<main><h1>Guide</h1><p>ranking budget html notes</p></main>",
    )
    .expect("html file");
    fs::write(
        root.join("scripts/build.py"),
        "def plan_budget():\n    return 'ranking budget python notes'\n",
    )
    .expect("python file");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(root)
        .arg("ranking budget")
        .assert()
        .success()
        .stdout(predicate::str::contains("settings.yaml"))
        .stdout(predicate::str::contains("data.csv"))
        .stdout(predicate::str::contains("page.html"))
        .stdout(predicate::str::contains("build.py"))
        .stdout(predicate::str::contains("code item"))
        .stdout(predicate::str::contains("table rows"));
}

#[test]
fn search_ignores_existing_contextforge_output_directories() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();

    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::create_dir_all(root.join("rust-final-pack")).expect("generated directory");
    fs::write(
        root.join("docs/requirements.md"),
        "final project requirements belong in the real source document\n",
    )
    .expect("source document");
    fs::write(
        root.join("rust-final-pack/context-bundle.md"),
        "final project requirements repeated from a stale generated bundle\n",
    )
    .expect("generated bundle");
    fs::write(root.join("rust-final-pack/context-manifest.json"), "{}\n").expect("manifest");
    fs::write(root.join("rust-final-pack/context-report.md"), "# report\n").expect("report");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(root)
        .arg("final project requirements")
        .assert()
        .success()
        .stdout(predicate::str::contains("requirements.md"))
        .stdout(predicate::str::contains("context-bundle.md").not())
        .stdout(predicate::str::contains("rust-final-pack").not());
}

fn write_simple_pdf(path: &Path, text: &str) {
    let stream = format!("BT /F1 24 Tf 100 700 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /MediaBox [0 0 612 792] /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();

    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }

    let xref_offset = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(path, bytes).expect("simple pdf");
}

fn write_simple_docx(path: &Path, text: &str) {
    let file = fs::File::create(path).expect("docx file");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    writer
        .start_file("[Content_Types].xml", options)
        .expect("content types");
    writer
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
        )
        .expect("content types xml");
    writer
        .start_file("word/document.xml", options)
        .expect("document xml");
    writer
        .write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body>
</w:document>"#
            )
            .as_bytes(),
        )
        .expect("document body");
    writer.finish().expect("finish docx");
}
