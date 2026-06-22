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
fn search_can_read_epub_documents() {
    let temp = tempdir().expect("temporary directory");
    let path = temp.path().join("book.epub");
    write_simple_epub(&path);

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(temp.path())
        .arg("semantic orchid")
        .assert()
        .success()
        .stdout(predicate::str::contains("book.epub"))
        .stdout(predicate::str::contains("semantic orchid blooms"));
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

#[test]
fn search_skips_unreadable_documents_and_keeps_valid_results() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path();
    fs::write(root.join("broken.pdf"), b"%PDF-1.4 broken document").expect("broken pdf");
    fs::write(root.join("broken.epub"), b"not a zip archive").expect("broken EPUB");
    fs::write(
        root.join("requirements.md"),
        "course project requirements remain searchable\n",
    )
    .expect("valid document");

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .args(["search", "--source"])
        .arg(root)
        .arg("course project requirements")
        .assert()
        .success()
        .stdout(predicate::str::contains("requirements.md"))
        .stdout(predicate::str::contains("Extraction warnings: 2"))
        .stderr(predicate::str::contains("broken.pdf"))
        .stderr(predicate::str::contains("broken.epub"));
}

#[test]
fn search_limits_terminal_results_and_supports_unlimited_output() {
    let temp = tempdir().expect("temporary directory");
    for index in 0..12 {
        fs::write(
            temp.path().join(format!("match-{index:02}.md")),
            format!("ranking budget match number {index}\n"),
        )
        .expect("source file");
    }

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["search", "ranking budget"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Showing 10 of 12 matches."))
        .stdout(predicate::str::contains("match-10.md").not());

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["search", "ranking budget", "--limit", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("match-10.md"))
        .stdout(predicate::str::contains("Showing 10 of 12 matches.").not());
}

#[test]
fn search_prints_paths_relative_to_the_source_directory() {
    let temp = tempdir().expect("temporary directory");
    fs::create_dir_all(temp.path().join("docs")).expect("docs directory");
    fs::write(
        temp.path().join("docs/requirements.md"),
        "course project requirements\n",
    )
    .expect("source file");
    let absolute_root = temp.path().display().to_string();

    Command::cargo_bin("contextforge")
        .expect("contextforge binary")
        .current_dir(temp.path())
        .args(["search", "course project requirements"])
        .assert()
        .success()
        .stdout(predicate::str::contains("docs"))
        .stdout(predicate::str::contains("requirements.md"))
        .stdout(predicate::str::contains(absolute_root).not());
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

fn write_simple_epub(path: &Path) {
    let file = fs::File::create(path).expect("epub file");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    let entries = [
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles><rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
        ),
        (
            "OPS/package.opf",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest>
    <item id="first" href="first.xhtml" media-type="application/xhtml+xml"/>
    <item id="second" href="second.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="first"/><itemref idref="second"/></spine>
</package>"#,
        ),
        (
            "OPS/first.xhtml",
            "<html><body><h1>First</h1><p>ordinary opening chapter</p></body></html>",
        ),
        (
            "OPS/second.xhtml",
            "<html><body><h1>Second</h1><p>semantic orchid blooms here</p></body></html>",
        ),
    ];

    for (name, content) in entries {
        writer.start_file(name, options).expect("EPUB entry");
        writer.write_all(content.as_bytes()).expect("EPUB content");
    }
    writer.finish().expect("finish EPUB");
}
