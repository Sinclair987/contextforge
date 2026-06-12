use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{ContextForgeError, Result};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RustProjectMetrics {
    pub source: PathBuf,
    pub summary: RustMetricsSummary,
    pub assessment: RequirementAssessment,
    pub files: Vec<RustFileMetrics>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RustMetricsSummary {
    pub rust_files: usize,
    pub source_files: usize,
    pub test_files: usize,
    pub total_lines: usize,
    pub blank_lines: usize,
    pub comment_lines: usize,
    pub effective_lines: usize,
    pub source_effective_lines: usize,
    pub test_effective_lines: usize,
    pub modules_declared: usize,
    pub structs: usize,
    pub enums: usize,
    pub traits: usize,
    pub impl_blocks: usize,
    pub functions: usize,
    pub public_items: usize,
    pub async_functions: usize,
    pub generic_item_lines: usize,
    pub lifetime_lines: usize,
    pub result_mentions: usize,
    pub test_functions: usize,
    pub unwrap_calls: usize,
    pub expect_calls: usize,
    pub panic_macros: usize,
    pub todo_macros: usize,
    pub unsafe_mentions: usize,
}

impl RustMetricsSummary {
    fn add_file(&mut self, file: &RustFileMetrics) {
        self.rust_files += 1;
        self.total_lines += file.total_lines;
        self.blank_lines += file.blank_lines;
        self.comment_lines += file.comment_lines;
        self.effective_lines += file.effective_lines;
        self.modules_declared += file.modules_declared;
        self.structs += file.structs;
        self.enums += file.enums;
        self.traits += file.traits;
        self.impl_blocks += file.impl_blocks;
        self.functions += file.functions;
        self.public_items += file.public_items;
        self.async_functions += file.async_functions;
        self.generic_item_lines += file.generic_item_lines;
        self.lifetime_lines += file.lifetime_lines;
        self.result_mentions += file.result_mentions;
        self.test_functions += file.test_functions;
        self.unwrap_calls += file.unwrap_calls;
        self.expect_calls += file.expect_calls;
        self.panic_macros += file.panic_macros;
        self.todo_macros += file.todo_macros;
        self.unsafe_mentions += file.unsafe_mentions;

        match file.area {
            RustFileArea::Source => {
                self.source_files += 1;
                self.source_effective_lines += file.effective_lines;
            }
            RustFileArea::Test => {
                self.test_files += 1;
                self.test_effective_lines += file.effective_lines;
            }
            RustFileArea::Other => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RustFileMetrics {
    pub path: PathBuf,
    pub area: RustFileArea,
    pub total_lines: usize,
    pub blank_lines: usize,
    pub comment_lines: usize,
    pub effective_lines: usize,
    pub modules_declared: usize,
    pub structs: usize,
    pub enums: usize,
    pub traits: usize,
    pub impl_blocks: usize,
    pub functions: usize,
    pub public_items: usize,
    pub async_functions: usize,
    pub generic_item_lines: usize,
    pub lifetime_lines: usize,
    pub result_mentions: usize,
    pub test_functions: usize,
    pub unwrap_calls: usize,
    pub expect_calls: usize,
    pub panic_macros: usize,
    pub todo_macros: usize,
    pub unsafe_mentions: usize,
}

impl RustFileMetrics {
    fn new(path: PathBuf, area: RustFileArea) -> Self {
        Self {
            path,
            area,
            total_lines: 0,
            blank_lines: 0,
            comment_lines: 0,
            effective_lines: 0,
            modules_declared: 0,
            structs: 0,
            enums: 0,
            traits: 0,
            impl_blocks: 0,
            functions: 0,
            public_items: 0,
            async_functions: 0,
            generic_item_lines: 0,
            lifetime_lines: 0,
            result_mentions: 0,
            test_functions: 0,
            unwrap_calls: 0,
            expect_calls: 0,
            panic_macros: 0,
            todo_macros: 0,
            unsafe_mentions: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RustFileArea {
    Source,
    Test,
    Other,
}

impl RustFileArea {
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Test => "test",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequirementAssessment {
    pub checks: Vec<RequirementCheck>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequirementCheck {
    pub name: String,
    pub status: RequirementStatus,
    pub detail: String,
}

impl RequirementCheck {
    fn new(name: &str, status: RequirementStatus, detail: String) -> Self {
        Self {
            name: name.to_string(),
            status,
            detail,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Pass,
    Warn,
    Fail,
}

impl RequirementStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

pub fn analyze_rust_project(source: &Path) -> Result<RustProjectMetrics> {
    if !source.exists() {
        return Err(ContextForgeError::ScanSourceMissing {
            path: source.to_path_buf(),
        });
    }

    if !source.is_dir() {
        return Err(ContextForgeError::ScanSourceNotDirectory {
            path: source.to_path_buf(),
        });
    }

    let mut rust_files = Vec::new();
    collect_rust_files(source, source, &mut rust_files)?;
    rust_files.sort();

    let mut files = Vec::new();
    let mut summary = RustMetricsSummary::default();
    for path in rust_files {
        let metrics = analyze_rust_file(source, &path)?;
        summary.add_file(&metrics);
        files.push(metrics);
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    let assessment = assess_requirements(&summary);
    Ok(RustProjectMetrics {
        source: source.to_path_buf(),
        summary,
        assessment,
        files,
    })
}

fn collect_rust_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|source| ContextForgeError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ContextForgeError::ReadDirectoryEntry {
            path: directory.to_path_buf(),
            source,
        })?;
        entries.push(entry.path());
    }
    entries.sort();

    for path in entries {
        let metadata = fs::metadata(&path).map_err(|source| ContextForgeError::ReadMetadata {
            path: path.to_path_buf(),
            source,
        })?;

        if metadata.is_dir() {
            if should_skip_metrics_directory(root, &path) {
                continue;
            }
            collect_rust_files(root, &path, files)?;
            continue;
        }

        if metadata.is_file() && is_rust_file(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn should_skip_metrics_directory(root: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if matches!(name, ".git" | "target" | "node_modules") {
        return true;
    }

    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|first| first == ".cargo-tools")
}

fn is_rust_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
}

fn analyze_rust_file(root: &Path, path: &Path) -> Result<RustFileMetrics> {
    let content = fs::read_to_string(path).map_err(|source| ContextForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let relative_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let area = classify_file_area(&relative_path);
    let mut metrics = RustFileMetrics::new(relative_path, area);
    let mut state = CommentState::default();
    let mut pending_test_attribute = false;

    for line in content.lines() {
        metrics.total_lines += 1;
        let trimmed_original = line.trim();
        if trimmed_original.is_empty() {
            metrics.blank_lines += 1;
            continue;
        }

        let code = state.strip_comments(line);
        let trimmed_code = code.trim();
        if trimmed_code.is_empty() {
            metrics.comment_lines += 1;
            continue;
        }

        metrics.effective_lines += 1;
        let semantic_code = strip_string_literals(trimmed_code);
        update_file_metrics(
            &mut metrics,
            semantic_code.trim(),
            &mut pending_test_attribute,
        );
    }

    Ok(metrics)
}

fn classify_file_area(relative_path: &Path) -> RustFileArea {
    let components = relative_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();

    if components.first() == Some(&"tests") || components.contains(&"tests") {
        return RustFileArea::Test;
    }

    if components.first() == Some(&"src") {
        return RustFileArea::Source;
    }

    if relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.rs") || name.ends_with("_tests.rs"))
    {
        return RustFileArea::Test;
    }

    RustFileArea::Other
}

fn update_file_metrics(
    metrics: &mut RustFileMetrics,
    code: &str,
    pending_test_attribute: &mut bool,
) {
    let trimmed = code.trim_start();
    if is_test_attribute(trimmed) {
        *pending_test_attribute = true;
    }

    if trimmed.starts_with("pub ") || trimmed.starts_with("pub(") {
        metrics.public_items += 1;
    }

    if has_keyword(code, "mod") {
        metrics.modules_declared += 1;
    }
    if has_keyword(code, "struct") {
        metrics.structs += 1;
    }
    if has_keyword(code, "enum") {
        metrics.enums += 1;
    }
    if has_keyword(code, "trait") {
        metrics.traits += 1;
    }
    if has_keyword(code, "impl") {
        metrics.impl_blocks += 1;
    }
    if has_keyword(code, "fn") {
        metrics.functions += 1;
        if *pending_test_attribute {
            metrics.test_functions += 1;
            *pending_test_attribute = false;
        }
        if has_keyword(code, "async") {
            metrics.async_functions += 1;
        }
    }

    if is_probably_generic_item(code) {
        metrics.generic_item_lines += 1;
    }
    if code.contains("&'") || code.contains("<'") || code.contains("where '") {
        metrics.lifetime_lines += 1;
    }

    metrics.result_mentions += count_word(code, "Result");
    metrics.unwrap_calls += count_call(code, "unwrap");
    metrics.expect_calls += count_call(code, "expect");
    metrics.panic_macros += count_macro(code, "panic");
    metrics.todo_macros += count_macro(code, "todo") + count_macro(code, "unimplemented");

    if has_keyword(code, "unsafe") {
        metrics.unsafe_mentions += 1;
    }
}

fn is_test_attribute(code: &str) -> bool {
    code.starts_with("#[test]")
        || code.starts_with("#[tokio::test")
        || code.starts_with("#[async_std::test")
        || (code.starts_with("#[") && code.contains("test]"))
}

fn is_probably_generic_item(code: &str) -> bool {
    let item_line = ["fn", "struct", "enum", "trait", "impl"]
        .iter()
        .any(|keyword| has_keyword(code, keyword));
    item_line && code.contains('<') && code.contains('>')
}

fn has_keyword(code: &str, keyword: &str) -> bool {
    code.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|token| token == keyword)
}

fn count_word(code: &str, word: &str) -> usize {
    code.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| *token == word)
        .count()
}

fn count_call(code: &str, function_name: &str) -> usize {
    let compact = code.replace(' ', "");
    compact.matches(&format!(".{function_name}(")).count()
}

fn count_macro(code: &str, macro_name: &str) -> usize {
    let compact = code.replace(' ', "");
    compact.matches(&format!("{macro_name}!(")).count()
}

fn strip_string_literals(code: &str) -> String {
    let mut output = String::with_capacity(code.len());
    let mut characters = code.chars().peekable();
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;

    while let Some(character) = characters.next() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => {
                    in_string = false;
                    output.push_str("\"\"");
                }
                _ => {}
            }
            continue;
        }

        if in_char {
            if escaped {
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '\'' => {
                    in_char = false;
                    output.push_str("''");
                }
                _ => {}
            }
            continue;
        }

        if character == 'r' {
            if let Some(hash_count) = raw_string_hash_count(&characters) {
                consume_raw_string(&mut characters, hash_count);
                output.push_str("r\"\"");
                continue;
            }
        }

        match character {
            '"' => {
                in_string = true;
            }
            '\'' if starts_lifetime_or_label(&characters) => {
                output.push(character);
            }
            '\'' if characters.peek().is_some() => {
                in_char = true;
            }
            _ => output.push(character),
        }
    }

    output
}

fn raw_string_hash_count(characters: &std::iter::Peekable<std::str::Chars<'_>>) -> Option<usize> {
    let mut lookahead = characters.clone();
    let mut hashes = 0;
    while matches!(lookahead.peek(), Some('#')) {
        lookahead.next();
        hashes += 1;
    }

    matches!(lookahead.next(), Some('"')).then_some(hashes)
}

fn consume_raw_string(characters: &mut std::iter::Peekable<std::str::Chars<'_>>, hashes: usize) {
    for _ in 0..hashes {
        characters.next();
    }
    characters.next();

    while let Some(character) = characters.next() {
        if character != '"' {
            continue;
        }

        let mut lookahead = characters.clone();
        let mut matched = true;
        for _ in 0..hashes {
            if !matches!(lookahead.next(), Some('#')) {
                matched = false;
                break;
            }
        }

        if matched {
            for _ in 0..hashes {
                characters.next();
            }
            break;
        }
    }
}

fn starts_lifetime_or_label(characters: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut lookahead = characters.clone();
    let Some(first) = lookahead.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    for character in lookahead {
        if character.is_ascii_alphanumeric() || character == '_' {
            continue;
        }
        return character != '\'';
    }

    true
}

#[derive(Debug, Default)]
struct CommentState {
    in_block_comment: bool,
}

impl CommentState {
    fn strip_comments(&mut self, line: &str) -> String {
        let mut output = String::new();
        let mut rest = line;

        loop {
            if self.in_block_comment {
                let Some(end) = rest.find("*/") else {
                    return output;
                };
                self.in_block_comment = false;
                rest = &rest[end + 2..];
                continue;
            }

            match (rest.find("//"), rest.find("/*")) {
                (Some(line_comment), Some(block_comment)) if line_comment < block_comment => {
                    output.push_str(&rest[..line_comment]);
                    return output;
                }
                (Some(_), Some(block_comment)) => {
                    output.push_str(&rest[..block_comment]);
                    self.in_block_comment = true;
                    rest = &rest[block_comment + 2..];
                }
                (Some(line_comment), None) => {
                    output.push_str(&rest[..line_comment]);
                    return output;
                }
                (None, Some(block_comment)) => {
                    output.push_str(&rest[..block_comment]);
                    self.in_block_comment = true;
                    rest = &rest[block_comment + 2..];
                }
                (None, None) => {
                    output.push_str(rest);
                    return output;
                }
            }
        }
    }
}

fn assess_requirements(summary: &RustMetricsSummary) -> RequirementAssessment {
    let checks = vec![
        assess_effective_lines(summary),
        assess_module_count(summary),
        assess_core_type_features(summary),
        assess_error_handling(summary),
        assess_tests(summary),
        assess_engineering_hygiene(summary),
    ];

    RequirementAssessment { checks }
}

fn assess_effective_lines(summary: &RustMetricsSummary) -> RequirementCheck {
    let status = if summary.source_effective_lines >= 1_500 {
        RequirementStatus::Pass
    } else if summary.effective_lines >= 1_500 {
        RequirementStatus::Warn
    } else {
        RequirementStatus::Fail
    };

    RequirementCheck::new(
        "Rust effective code scale",
        status,
        format!(
            "{} effective lines in src, {} effective lines total",
            summary.source_effective_lines, summary.effective_lines
        ),
    )
}

fn assess_module_count(summary: &RustMetricsSummary) -> RequirementCheck {
    let module_signals = summary.modules_declared + summary.source_files;
    let status = if module_signals >= 3 {
        RequirementStatus::Pass
    } else {
        RequirementStatus::Fail
    };

    RequirementCheck::new(
        "Module organization",
        status,
        format!(
            "{} declared modules and {} source files",
            summary.modules_declared, summary.source_files
        ),
    )
}

fn assess_core_type_features(summary: &RustMetricsSummary) -> RequirementCheck {
    let missing = [
        ("struct", summary.structs),
        ("enum", summary.enums),
        ("trait", summary.traits),
        ("impl", summary.impl_blocks),
    ]
    .into_iter()
    .filter_map(|(name, count)| (count == 0).then_some(name))
    .collect::<Vec<_>>();

    let status = if missing.is_empty() {
        RequirementStatus::Pass
    } else {
        RequirementStatus::Warn
    };

    let detail = if missing.is_empty() {
        format!(
            "{} structs, {} enums, {} traits, {} impl blocks",
            summary.structs, summary.enums, summary.traits, summary.impl_blocks
        )
    } else {
        format!("missing visible signals: {}", missing.join(", "))
    };

    RequirementCheck::new("Core Rust type features", status, detail)
}

fn assess_error_handling(summary: &RustMetricsSummary) -> RequirementCheck {
    let status = if summary.result_mentions > 0 {
        RequirementStatus::Pass
    } else {
        RequirementStatus::Warn
    };

    RequirementCheck::new(
        "Result-based error handling",
        status,
        format!("{} Result mentions", summary.result_mentions),
    )
}

fn assess_tests(summary: &RustMetricsSummary) -> RequirementCheck {
    let status = if summary.test_functions >= 3 || summary.test_files >= 2 {
        RequirementStatus::Pass
    } else if summary.test_functions > 0 || summary.test_files > 0 {
        RequirementStatus::Warn
    } else {
        RequirementStatus::Fail
    };

    RequirementCheck::new(
        "Tests",
        status,
        format!(
            "{} test files and {} #[test] functions",
            summary.test_files, summary.test_functions
        ),
    )
}

fn assess_engineering_hygiene(summary: &RustMetricsSummary) -> RequirementCheck {
    let risky_calls = summary.unwrap_calls + summary.expect_calls + summary.panic_macros;
    let expected_test_fixture_calls = summary.test_functions.saturating_mul(5) + 20;
    let has_hygiene_risk = summary.todo_macros > 0
        || summary.unsafe_mentions > 0
        || summary.unwrap_calls > 0
        || risky_calls > expected_test_fixture_calls;
    let status = if has_hygiene_risk {
        RequirementStatus::Warn
    } else {
        RequirementStatus::Pass
    };

    RequirementCheck::new(
        "Engineering hygiene",
        status,
        format!(
            "{} unwrap, {} expect, {} panic, {} todo/unimplemented, {} unsafe signals",
            summary.unwrap_calls,
            summary.expect_calls,
            summary.panic_macros,
            summary.todo_macros,
            summary.unsafe_mentions
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn analyze_rust_project_counts_source_tests_and_features() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        fs::create_dir_all(root.join("src/model")).expect("source subdirectory");
        fs::create_dir_all(root.join("tests")).expect("tests directory");
        fs::create_dir_all(root.join("target")).expect("target directory");

        fs::write(
            root.join("src/lib.rs"),
            r#"
pub mod model;

pub trait Render<T> {
    fn render(&self, value: T) -> Result<String, String>;
}

pub struct Document<'a> {
    title: &'a str,
}

pub enum Format {
    Text,
}

impl<'a> Document<'a> {
    pub async fn load<T>(&self, value: T) -> Result<String, String> {
        Ok(format!("{}", self.title))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unit_test_signal() {
        assert_eq!(1, 1);
    }
}
"#,
        )
        .expect("source file");
        fs::write(root.join("src/model/item.rs"), "pub struct Item;\n").expect("module file");
        fs::write(
            root.join("tests/cli.rs"),
            "#[test]\nfn cli_test_signal() {\n    assert!(true);\n}\n",
        )
        .expect("test file");
        fs::write(root.join("target/generated.rs"), "pub struct Ignored;\n")
            .expect("ignored generated file");

        let metrics = analyze_rust_project(root).expect("project metrics");

        assert_eq!(metrics.summary.rust_files, 3);
        assert_eq!(metrics.summary.source_files, 2);
        assert_eq!(metrics.summary.test_files, 1);
        assert_eq!(metrics.summary.modules_declared, 2);
        assert_eq!(metrics.summary.structs, 2);
        assert_eq!(metrics.summary.enums, 1);
        assert_eq!(metrics.summary.traits, 1);
        assert_eq!(metrics.summary.impl_blocks, 1);
        assert_eq!(metrics.summary.async_functions, 1);
        assert_eq!(metrics.summary.test_functions, 2);
        assert!(metrics
            .assessment
            .checks
            .iter()
            .any(|check| check.name == "Core Rust type features"
                && check.status == RequirementStatus::Pass));
    }

    #[test]
    fn comment_state_removes_line_and_block_comments() {
        let mut state = CommentState::default();

        assert_eq!(
            state.strip_comments("let x = 1; // trailing").trim(),
            "let x = 1;"
        );
        assert_eq!(state.strip_comments("/* block starts").trim(), "");
        assert_eq!(
            state.strip_comments("still comment */ let y = 2;").trim(),
            "let y = 2;"
        );
    }

    #[test]
    fn string_literals_do_not_count_as_feature_keywords() {
        let cleaned = strip_string_literals(
            r#"println!("unsafe struct enum trait Result unwrap panic todo");"#,
        );

        assert!(!has_keyword(&cleaned, "unsafe"));
        assert!(!has_keyword(&cleaned, "struct"));
        assert_eq!(count_word(&cleaned, "Result"), 0);
        assert_eq!(count_call(&cleaned, "unwrap"), 0);
        assert_eq!(count_macro(&cleaned, "panic"), 0);
    }

    #[test]
    fn string_literal_cleaning_preserves_lifetimes() {
        let cleaned = strip_string_literals("fn borrow<'a>(value: &'a str) -> &'a str { value }");

        assert!(cleaned.contains("'a"));
        assert!(has_keyword(&cleaned, "fn"));
    }

    #[test]
    fn raw_string_literals_do_not_count_as_feature_keywords() {
        let cleaned =
            strip_string_literals(r##"let text = r#"unsafe struct enum trait Result"#;"##);

        assert!(!has_keyword(&cleaned, "unsafe"));
        assert!(!has_keyword(&cleaned, "struct"));
        assert_eq!(count_word(&cleaned, "Result"), 0);
    }
}
