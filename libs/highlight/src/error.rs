use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to load syntax set: {0}")]
    Load(String),
    #[error("syntect error: {0}")]
    Syntect(#[from] syntect::Error),
}
