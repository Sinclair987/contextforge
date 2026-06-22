use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, ContextForgeError>;

#[derive(Debug, thiserror::Error)]
pub enum ContextForgeError {
    #[error("context goal must not be blank")]
    InvalidGoal,

    #[error("token budget must be greater than zero")]
    InvalidBudget,

    #[error("token budget is too small for bundle metadata; use at least {minimum} tokens")]
    BudgetTooSmall { minimum: usize },

    #[error("no context matched the goal: {goal}")]
    NoMatchingContext { goal: String },

    #[error("configuration file already exists: {path}")]
    ConfigExists { path: PathBuf },

    #[error("failed to write configuration file `{path}`")]
    WriteConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read configuration file `{path}`")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse configuration file `{path}`")]
    ParseConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to write output file `{path}`")]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize context manifest")]
    SerializeManifest {
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to serialize command output")]
    SerializeOutput {
        #[source]
        source: serde_json::Error,
    },

    #[error("privacy gate failed: {count} finding(s) at or above {severity}")]
    PrivacyGateFailed { severity: String, count: usize },

    #[error("failed to extract PDF text from `{path}`")]
    ExtractPdf {
        path: PathBuf,
        #[source]
        source: pdf_extract::OutputError,
    },

    #[error("PDF extractor could not process `{path}`: {message}")]
    ExtractPdfPanic { path: PathBuf, message: String },

    #[error("failed to open DOCX archive `{path}`")]
    OpenDocxArchive {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("failed to read DOCX document XML from `{path}`")]
    ReadDocxEntry {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("failed to parse DOCX XML from `{path}`")]
    ParseDocxXml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },

    #[error("failed to decode DOCX XML text from `{path}`")]
    DecodeDocxXml {
        path: PathBuf,
        #[source]
        source: quick_xml::encoding::EncodingError,
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

    #[error("unsupported file kind for extraction: {path}")]
    UnsupportedFileKind { path: PathBuf },
}
