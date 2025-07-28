use thiserror::Error;

#[derive(Error, Debug)]
pub enum GPXError {
    #[error("File read error: {0}")]
    FileRead(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP request error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Tile fetch error: {0}")]
    TileFetch(String),
    #[error("Async runtime error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("GPX parse error: {0}")]
    GpxParse(#[from] gpx::errors::GpxError),
}
