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

use crate::error::Errors;

pub async fn canonicalize_and_ensure_path(path: &PathBuf) -> Result<PathBuf, Errors> {
    create_dir_all(path).await?;
    Ok(canonicalize(path).await?)
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

pub async fn write_file(path: &Path, content: &Arc<str>) -> Result<(), Errors> {
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

pub fn crc32_checksum(path: &PathBuf) -> Result<String, Errors> {
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
