use std::path::{Path, PathBuf};

use tokio::fs::read_dir;

use crate::error::{ContextExt, Errors};

pub fn canonicalize(path: &PathBuf) -> Result<PathBuf, Errors> {
    std::fs::create_dir_all(path)
        .with_context(format!("create directory: {}", path.clone().display()))?;
    Ok(path
        .canonicalize()
        .with_context(format!("canonicalize path: {}", path.clone().display()))?)
}

pub async fn get_files_by_ext_deep(path: &Path, ext: &str) -> Result<Vec<PathBuf>, Errors> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    dirs.push(path.to_path_buf());
    while let Some(dir) = dirs.pop() {
        let mut entries = read_dir(dir).await?;
        {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if Some(ext) == path.extension().unwrap_or_default().to_str() {
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}
