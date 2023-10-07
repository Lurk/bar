use std::path::PathBuf;

use crate::error::{ContextExt, Errors};

pub fn canonicalize(path: &PathBuf) -> Result<PathBuf, Errors> {
    std::fs::create_dir_all(path)
        .with_context(format!("create directory: {}", path.clone().display()))?;
    Ok(path
        .canonicalize()
        .with_context(format!("canonicalize path: {}", path.clone().display()))?)
}
