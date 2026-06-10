use std::{fs, path::Path};

use crate::{ContextForgeError, Result};

pub const DEFAULT_CONFIG_FILE: &str = "contextforge.toml";

pub const EXAMPLE_CONFIG: &str = r#"# ContextForge configuration

[scanner]
max_file_bytes = 1048576
ignore_patterns = [".git", "target", "node_modules"]
include_extensions = ["md", "txt", "rs", "toml", "json"]

[chunking]
max_chunk_tokens = 800
overlap_tokens = 80

[ranking]
path_weight = 1.2
title_weight = 1.5
privacy_penalty = 2.0

[output]
bundle = "context-bundle.md"
manifest = "context-manifest.json"
report = "context-report.md"
"#;

pub fn write_default_config(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(ContextForgeError::ConfigExists {
            path: path.to_path_buf(),
        });
    }

    fs::write(path, EXAMPLE_CONFIG).map_err(|source| ContextForgeError::WriteConfig {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextForgeError;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn example_config_contains_scanner_defaults() {
        assert!(EXAMPLE_CONFIG.contains("[scanner]"));
        assert!(EXAMPLE_CONFIG.contains("max_file_bytes"));
    }

    #[test]
    fn write_default_config_creates_file_when_missing() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join(DEFAULT_CONFIG_FILE);

        write_default_config(&path).expect("write config");

        let content = fs::read_to_string(path).expect("generated config");
        assert!(content.contains("[output]"));
    }

    #[test]
    fn write_default_config_returns_error_when_file_exists() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join(DEFAULT_CONFIG_FILE);
        fs::write(&path, "existing = true\n").expect("seed config");

        let error = write_default_config(&path).expect_err("existing file error");

        assert!(matches!(error, ContextForgeError::ConfigExists { .. }));
    }
}
