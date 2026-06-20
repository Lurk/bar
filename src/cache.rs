use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    time::Duration,
};

use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    diagnostic::{BarDiagnostic, ContextExt},
    fs::write_file,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Value<T> {
    data: T,
    version: usize,
    created_at: std::time::SystemTime,
}

pub struct Cache<T> {
    __phantom: std::marker::PhantomData<T>,
    kind: String,
    ttl: Option<Duration>,
    version: usize,
    base_path: PathBuf,
}

/// Path of a raw cache entry under `base_path`, independent of any `Cache`
/// instance — useful for callers (e.g. image variants) that own the cache key
/// and layout themselves and only need the on-disk location.
pub fn raw_cache_path(base_path: &Path, kind: &str, key: &str, ext: &str) -> PathBuf {
    base_path.join(format!(".cache/{kind}/{key}.{ext}"))
}

/// Two-level shard directory prefix (`ab/cd`) from the first four characters of
/// `hash`, so cache entries spread across directories instead of piling into a
/// single one. Callers join it ahead of the entry's own filename.
#[must_use]
pub fn shard_prefix(hash: &str) -> String {
    format!("{}/{}", &hash[..2], &hash[2..4])
}

impl<T> Cache<T> {
    pub fn new(kind: &str, version: usize, base_path: &Path) -> Self {
        Cache {
            kind: kind.to_string(),
            version,
            ttl: None,
            base_path: base_path.to_path_buf(),
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn make_key(input: &str) -> String {
        let hash = BASE64URL_NOPAD.encode(seahash::hash(input.as_bytes()).to_be_bytes().as_ref());
        format!("{}/{hash}", shard_prefix(&hash))
    }

    pub fn raw_path(&self, key: &str, ext: &str) -> PathBuf {
        raw_cache_path(&self.base_path, &self.kind, key, ext)
    }

    pub async fn set_raw(&self, key: &str, ext: &str, data: &[u8]) -> Result<(), BarDiagnostic> {
        let full_path = self.raw_path(key, ext);
        write_file(&full_path, data)
            .await
            .with_context(|| format!("Failed to write raw cache for key: {key}"))
    }

    fn get_path(&self, key: &str) -> PathBuf {
        self.base_path
            .join(format!(".cache/{}/{}.json", self.kind, key))
    }
}

impl<T: Debug + Serialize + DeserializeOwned> Cache<T> {
    pub async fn set(&self, key: &str, data: &T) -> Result<(), BarDiagnostic> {
        let cache = Value {
            data,
            version: self.version,
            created_at: std::time::SystemTime::now(),
        };

        let serialized = serde_json::to_string(&cache)?;
        let full_path = self.get_path(key);

        write_file(&full_path, serialized.as_bytes())
            .await
            .with_context(|| format!("Failed to write cache for key: {key}"))
    }

    pub fn get(&self, key: &str) -> Result<Option<T>, BarDiagnostic> {
        let full_path = self.get_path(key);

        if !full_path.exists() {
            return Ok(None);
        }

        if !full_path.is_file() {
            return Err(format!("Cache path {} is not a file", full_path.display()).into());
        }

        let rdr = std::fs::File::open(&full_path)
            .with_context(|| format!("Failed to open cache file for key: {key}"))?;

        let cache: Value<T> = serde_json::from_reader(rdr)
            .with_context(|| format!("Failed to deserialize cache file for key: {key}"))?;

        if cache.version == self.version {
            if let Some(ttl) = self.ttl {
                if cache.created_at.elapsed().unwrap_or_default() < ttl {
                    return Ok(Some(cache.data));
                }
            } else {
                return Ok(Some(cache.data));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_key_produces_hierarchical_path() {
        let key = Cache::<String>::make_key("hello");
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 2);
        assert_eq!(parts[1].len(), 2);
        assert!(parts[2].len() > 4);
        assert!(parts[2].starts_with(parts[0]));
        assert!(parts[2][2..].starts_with(parts[1]));
    }

    #[test]
    fn make_key_is_deterministic() {
        assert_eq!(
            Cache::<String>::make_key("same input"),
            Cache::<String>::make_key("same input")
        );
    }

    #[test]
    fn make_key_differs_for_different_inputs() {
        assert_ne!(
            Cache::<String>::make_key("input a"),
            Cache::<String>::make_key("input b")
        );
    }

    #[test]
    fn raw_path_uses_kind_key_and_ext() {
        let cache = Cache::<()>::new("remote_images", 1, Path::new("/base"));
        let path = cache.raw_path("ab/cd/abcdef", "jpg");
        assert_eq!(
            path,
            PathBuf::from("/base/.cache/remote_images/ab/cd/abcdef.jpg")
        );
    }

    #[test]
    fn shard_prefix_uses_first_four_hash_chars() {
        assert_eq!(super::shard_prefix("abcdef123"), "ab/cd");
    }

    #[test]
    fn raw_cache_path_matches_method_without_an_instance() {
        let path = super::raw_cache_path(Path::new("/base"), "image_variants", "v1-h-p-1x1", "jpg");
        assert_eq!(
            path,
            PathBuf::from("/base/.cache/image_variants/v1-h-p-1x1.jpg")
        );
    }

    #[tokio::test]
    async fn set_raw_writes_binary_data() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::<()>::new("bin", 1, dir.path());
        let key = "ab/cd/abcdef";
        let data = b"hello binary";

        cache.set_raw(key, "png", data).await.unwrap();

        let path = cache.raw_path(key, "png");
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[tokio::test]
    async fn test_cache_manager() {
        let cache = Cache::new("test", 1, Path::new("./test/fixtures"));

        let key = "test_key";
        let value = "test_value".to_string();

        cache.set(key, &value).await.ok();

        assert_eq!(cache.get(key).ok().unwrap(), Some(value));
    }
}
