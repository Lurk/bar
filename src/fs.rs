use data_encoding::BASE64URL_NOPAD;
use seahash::SeaHasher;
use std::{
    fs::File,
    hash::Hasher,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{OpenOptions, canonicalize, create_dir_all, read_dir},
    io::AsyncWriteExt,
};

use crate::diagnostic::{BarDiagnostic, ContextExt};

/// Validate and normalize a user-supplied, project-relative path into a clean
/// relative string: no leading `/`, no `.`/`..`/`//` left in the output. Rejects
/// any path that escapes the project root via `..` or carries an absolute
/// prefix. The result is fs-join-safe and safe to emit as a published URL path.
/// Returns a plain message on rejection so callers wrap it in their own error
/// type. May return an empty string for empty / all-`.` input — callers that
/// require a non-empty path must check.
///
/// # Errors
/// Returns a message if the path escapes the root or is absolute.
pub fn normalize_project_rel(raw: &str) -> Result<String, String> {
    let rel = raw.trim().trim_start_matches('/');
    let mut parts: Vec<String> = Vec::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => parts.push(c.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts
                    .pop()
                    .ok_or_else(|| format!("path '{raw}' escapes the project root"))?;
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("path '{raw}' must be project-relative"));
            }
        }
    }
    Ok(parts.join("/"))
}

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

    #[test]
    fn normalize_strips_leading_slash_and_cur_dir() {
        assert_eq!(
            normalize_project_rel("/photos/trip").unwrap(),
            "photos/trip"
        );
        assert_eq!(
            normalize_project_rel("photos/./trip").unwrap(),
            "photos/trip"
        );
        assert_eq!(
            normalize_project_rel("photos//trip").unwrap(),
            "photos/trip"
        );
        assert_eq!(
            normalize_project_rel("/photos/trip/").unwrap(),
            "photos/trip"
        );
    }

    #[test]
    fn normalize_resolves_interior_parent_dir() {
        assert_eq!(
            normalize_project_rel("photos/x/../trip").unwrap(),
            "photos/trip"
        );
        assert_eq!(
            normalize_project_rel("tracks/../tracks/run.gpx").unwrap(),
            "tracks/run.gpx"
        );
    }

    #[test]
    fn normalize_rejects_escaping_and_absolute() {
        assert!(normalize_project_rel("/../etc").is_err());
        assert!(normalize_project_rel("a/../../b").is_err());
    }

    #[test]
    fn normalize_allows_empty() {
        assert_eq!(normalize_project_rel("").unwrap(), "");
        assert_eq!(normalize_project_rel("/").unwrap(), "");
    }

    #[tokio::test]
    async fn get_seahash_happy_path() -> Result<(), BarDiagnostic> {
        let checksum = seahash_checksum(&PathBuf::from("./test/fixtures/static/1.png"))?;
        assert_eq!(checksum, String::from("digdyOjp4_o"));
        Ok(())
    }
}
