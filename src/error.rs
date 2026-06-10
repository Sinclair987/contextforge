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
}
