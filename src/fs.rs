use crc32fast::Hasher;
use data_encoding::BASE64URL_NOPAD;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{canonicalize, create_dir_all, read_dir, OpenOptions},
    io::AsyncWriteExt,
};

use crate::error::{BarErr, ContextExt};

pub async fn canonicalize_with_context(path: &PathBuf) -> Result<PathBuf, BarErr> {
    canonicalize(path)
        .await
        .with_context(|| format!("canonicalize path: {:?}", path))
}

/// Get all files with given extensions in the given directory and its subdirectories.
/// The extension should not include the dot.
pub async fn get_files_by_ext_deep(path: &Path, ext: &[&str]) -> Result<Vec<PathBuf>, BarErr> {
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
                    .expect("file extension to be valid UTF-8"),
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub async fn write_file(path: &Path, content: &Arc<str>) -> Result<(), BarErr> {
    let prefix = path.parent().unwrap();
    create_dir_all(prefix).await?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .await?;
    file.write_all(content.as_bytes()).await?;
    Ok(())
}

pub fn crc32_checksum(path: &PathBuf) -> Result<String, BarErr> {
    let input = File::open(path)?;
    let mut reader = BufReader::new(input);

    let digest = {
        let mut hasher = Hasher::new();
        let mut buffer = [0; 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        hasher.finalize()
    };

    Ok(BASE64URL_NOPAD.encode(digest.to_be_bytes().as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_files_happy_path() -> Result<(), BarErr> {
        let files = get_files_by_ext_deep(&PathBuf::from("./test/"), &["yamd"]).await?;
        assert_eq!(
            files,
            vec![
                PathBuf::from("./test/fixtures/content/test.yamd"),
                PathBuf::from("./test/fixtures/content/test2.yamd")
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn get_crc32_happy_path() -> Result<(), BarErr> {
        let checksum = crc32_checksum(&PathBuf::from("./test/fixtures/static/1.png"))?;
        assert_eq!(checksum, String::from("jBiXwg"));
        Ok(())
    }
}
