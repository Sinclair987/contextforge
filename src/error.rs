use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, ContextForgeError>;

#[derive(Debug, thiserror::Error)]
pub enum ContextForgeError {
    #[error("configuration file already exists: {path}")]
    ConfigExists { path: PathBuf },

    #[error("failed to write configuration file `{path}`")]
    WriteConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("scan source does not exist: {path}")]
    ScanSourceMissing { path: PathBuf },

    #[error("scan source is not a directory: {path}")]
    ScanSourceNotDirectory { path: PathBuf },

    #[error("failed to read directory `{path}`")]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read directory entry under `{path}`")]
    ReadDirectoryEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read metadata for `{path}`")]
    ReadMetadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read file `{path}`")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
