use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{scanner::ScanOptions, ContextForgeError, Result};

pub const DEFAULT_CONFIG_FILE: &str = "contextforge.toml";

pub const EXAMPLE_CONFIG: &str = r#"# ContextForge configuration

[scanner]
max_file_bytes = 1048576
ignore_patterns = [".git", "target", "node_modules", "dist", "build", "out", "demo-output", "venv"]
include_paths = []
exclude_paths = []

[output]
bundle = "context-bundle.md"
manifest = "context-manifest.json"
report = "context-report.md"
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    scanner: ScannerConfig,
    #[serde(default)]
    output: OutputConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct ScannerConfig {
    max_file_bytes: Option<u64>,
    ignore_patterns: Option<Vec<String>>,
    include_paths: Option<Vec<String>>,
    exclude_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct OutputConfig {
    bundle: Option<String>,
    manifest: Option<String>,
    report: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputConfigValues {
    pub bundle: String,
    pub manifest: String,
    pub report: String,
}

impl Default for OutputConfigValues {
    fn default() -> Self {
        Self {
            bundle: "context-bundle.md".to_string(),
            manifest: "context-manifest.json".to_string(),
            report: "context-report.md".to_string(),
        }
    }
}

impl AppConfig {
    pub fn scan_options(&self) -> ScanOptions {
        let mut options = ScanOptions::default();
        if let Some(max_file_bytes) = self.scanner.max_file_bytes {
            options.max_file_bytes = max_file_bytes;
        }
        if let Some(ignore_patterns) = &self.scanner.ignore_patterns {
            for pattern in ignore_patterns {
                if !options.ignored_directories.contains(pattern) {
                    options.ignored_directories.push(pattern.clone());
                }
            }
        }
        if let Some(include_paths) = &self.scanner.include_paths {
            options.included_paths = include_paths.iter().map(PathBuf::from).collect();
        }
        if let Some(exclude_paths) = &self.scanner.exclude_paths {
            options.excluded_paths = exclude_paths.iter().map(PathBuf::from).collect();
        }
        options
    }

    pub fn output_values(&self) -> OutputConfigValues {
        let defaults = OutputConfigValues::default();
        OutputConfigValues {
            bundle: self.output.bundle.clone().unwrap_or(defaults.bundle),
            manifest: self.output.manifest.clone().unwrap_or(defaults.manifest),
            report: self.output.report.clone().unwrap_or(defaults.report),
        }
    }
}

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

pub fn load_config(path: Option<&Path>) -> Result<AppConfig> {
    load_config_for_source(path, Path::new("."))
}

pub fn load_config_for_source(path: Option<&Path>, source: &Path) -> Result<AppConfig> {
    let Some(path) = resolve_config_path(path, source) else {
        return Ok(AppConfig::default());
    };

    let content = fs::read_to_string(&path).map_err(|source| ContextForgeError::ReadConfig {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&content).map_err(|source| ContextForgeError::ParseConfig { path, source })
}

fn resolve_config_path(path: Option<&Path>, source: &Path) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(path.to_path_buf());
    }

    let source_root = if source.is_file() {
        source.parent().unwrap_or_else(|| Path::new("."))
    } else {
        source
    };
    let source_config = source_root.join(DEFAULT_CONFIG_FILE);
    if source_config.exists() {
        return Some(source_config);
    }

    let current_config = PathBuf::from(DEFAULT_CONFIG_FILE);
    current_config.exists().then_some(current_config)
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
    fn example_config_can_be_loaded() {
        let config: AppConfig = toml::from_str(EXAMPLE_CONFIG).expect("example config parses");

        assert_eq!(config.scan_options().max_file_bytes, 1_048_576);
        assert_eq!(config.output_values().bundle, "context-bundle.md");
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

    #[test]
    fn load_config_reads_scanner_and_output_settings() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join(DEFAULT_CONFIG_FILE);
        fs::write(
            &path,
            "[scanner]\nmax_file_bytes = 64\nignore_patterns = [\"target\", \"custom-cache\"]\n\n[output]\nbundle = \"bundle.md\"\n",
        )
        .expect("config file");

        let config = load_config(Some(&path)).expect("loaded config");
        let scan_options = config.scan_options();
        let output = config.output_values();

        assert_eq!(scan_options.max_file_bytes, 64);
        assert!(scan_options
            .ignored_directories
            .contains(&".git".to_string()));
        assert!(scan_options
            .ignored_directories
            .contains(&"custom-cache".to_string()));
        assert_eq!(
            scan_options
                .ignored_directories
                .iter()
                .filter(|pattern| pattern.as_str() == "target")
                .count(),
            1
        );
        assert_eq!(output.bundle, "bundle.md");
        assert_eq!(output.manifest, "context-manifest.json");
    }

    #[test]
    fn load_config_reads_path_filters() {
        let config: AppConfig = toml::from_str(
            "[scanner]\ninclude_paths = [\"docs\"]\nexclude_paths = [\"docs/private\"]\n",
        )
        .expect("config parses");

        let options = config.scan_options();

        assert_eq!(options.included_paths, vec![PathBuf::from("docs")]);
        assert_eq!(options.excluded_paths, vec![PathBuf::from("docs/private")]);
    }
}
