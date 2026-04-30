use data_encoding::BASE64URL_NOPAD;
use seahash::SeaHasher;
use std::{
    fs::File,
    hash::Hasher,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{OpenOptions, canonicalize, create_dir_all, read_dir},
    io::AsyncWriteExt,
};

use crate::diagnostic::{BarDiagnostic, ContextExt};

/// # Errors
/// Returns error if the path cannot be canonicalized.
pub async fn canonicalize_with_context(path: &PathBuf) -> Result<PathBuf, BarDiagnostic> {
    canonicalize(path)
        .await
        .with_context(|| format!("canonicalize path: {}", path.display()))
}

/// Get all files with given extensions in the given directory and its subdirectories.
/// The extension should not include the dot.
///
/// # Errors
/// Returns error if the directory cannot be read.
pub async fn get_files_by_ext_deep(
    path: &Path,
    ext: &[&str],
) -> Result<Vec<PathBuf>, BarDiagnostic> {
    let mut files = Vec::new();
    let mut dirs = vec![path.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut entries = read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if ext.contains(
                &path
                    .extension()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default(),
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// # Errors
/// Returns error if the file cannot be read.
pub async fn read_to_string(path: &Path) -> Result<Arc<str>, BarDiagnostic> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    Ok(Arc::from(content))
}

/// # Errors
/// Returns error if the file cannot be written.
pub async fn write_file(path: &Path, content: &[u8]) -> Result<(), BarDiagnostic> {
    let prefix = path.parent().ok_or_else(|| {
        BarDiagnostic::from(format!("path has no parent directory: {}", path.display()))
    })?;
    create_dir_all(prefix).await?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .await?;
    file.write_all(content).await?;
    file.flush().await?;
    Ok(())
}

/// # Errors
/// Returns error if the file cannot be read.
pub fn seahash_checksum(path: &PathBuf) -> Result<String, BarDiagnostic> {
    let input = File::open(path)?;
    let mut reader = BufReader::new(input);

    let digest = {
        let mut hasher = SeaHasher::new();
        let mut buffer = [0; 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }

            Hasher::write(&mut hasher, &buffer[..count]);
        }
        hasher.finish()
    };

    Ok(BASE64URL_NOPAD.encode(digest.to_be_bytes().as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_files_happy_path() -> Result<(), BarDiagnostic> {
        let files = get_files_by_ext_deep(&PathBuf::from("./test/"), &["yamd"]).await?;
        assert_eq!(
            files,
            vec![
                PathBuf::from("./test/fixtures/content/draft.yamd"),
                PathBuf::from("./test/fixtures/content/test.yamd"),
                PathBuf::from("./test/fixtures/content/test2.yamd")
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn get_seahash_happy_path() -> Result<(), BarDiagnostic> {
        let checksum = seahash_checksum(&PathBuf::from("./test/fixtures/static/1.png"))?;
        assert_eq!(checksum, String::from("digdyOjp4_o"));
        Ok(())
    }
}
