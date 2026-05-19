use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AutoparqError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parquet error: {0}")]
    ParquetError(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),
    #[error("Unsupported type: {0}")]
    UnsupportedType(String),
    #[error("Unsupported codec: {0}")]
    UnsupportedCodec(String),
}
