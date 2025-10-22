use std::{fmt::Debug, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    PATH,
    error::{BarErr, ContextExt},
    fs::write_file,
    PATH,
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
}

impl<T> Cache<T> {
    pub fn new(kind: &str, version: usize) -> Self {
        Cache {
            kind: kind.to_string(),
            version,
            ttl: None,
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    fn get_path(&self, key: &str) -> PathBuf {
        PATH.get()
            .expect("PATH should be initialized")
            .join(format!(".cache/{}/{}.json", self.kind, key))
    }
}

impl<T: Debug + Serialize + DeserializeOwned> Cache<T> {
    pub async fn set(&self, key: &str, data: &T) -> Result<(), BarErr> {
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

    pub fn get(&self, key: &str) -> Result<Option<T>, BarErr> {
        let full_path = self.get_path(key);

        if !full_path.exists() {
            return Ok(None);
        }

        if !full_path.is_file() {
            return Err(format!("Cache path {full_path:?} is not a file").into());
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

    #[tokio::test]
    async fn test_cache_manager() {
        PATH.get_or_init(|| PathBuf::from("./test/fixtures"));
        let cache = Cache::new("test", 1);

        let key = "test_key";
        let value = "test_value".to_string();

        cache.set(key, &value).await.ok();

        assert_eq!(cache.get(key).ok().unwrap(), Some(value));
    }
}
