use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use async_recursion::async_recursion;
use data_encoding::BASE64URL_NOPAD;
use img2text::Img2Text;
use tracing::debug;
use yamd::{
    Yamd,
    nodes::{Image, Images, YamdNodes},
};

use crate::{cache::Cache, config::AltTextGenerator, error::BarErr, fs::write_file};

async fn str_to_path(path: &str, base_path: &Path) -> Result<PathBuf, BarErr> {
    if path.starts_with("http") {
        let destination = base_path.join(format!(".cache/remote_images/{path}"));

        if !destination.exists() {
            debug!(
                "Downloading image from URL: {}\n to: {:?}",
                path, destination
            );
            let response = reqwest::get(path).await?;
            if !response.status().is_success() {
                return Err(
                    format!("Failed to fetch image, status code: {}", response.status()).into(),
                );
            }

            let bytes = response.bytes().await?;
            write_file(&destination, &bytes).await?;
            debug!("Saved image to temporary file: {:?}", destination);
        }
        Ok(destination)
    } else {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(format!("Image file does not exist: {}", path.display()).into());
        }
        Ok(path)
    }
}

async fn yamd_image<F, Fut>(i: &Image, generator: Arc<F>) -> Result<Option<Image>, BarErr>
where
    Fut: Future<Output = Result<String, String>> + 'static + Send,
    F: Fn(&str) -> Pin<Box<Fut>>,
{
    if !i.alt.is_empty() {
        return Ok(None);
    }
    debug!("no alt text found for image: {}", i.src);
    let alt = generator(i.src.as_ref()).await?;
    Ok(Some(Image {
        src: i.src.clone(),
        alt,
    }))
}

#[async_recursion]
pub async fn add_alt_text<F, Fut>(yamd: Yamd, getter: Arc<F>) -> Result<Yamd, BarErr>
where
    Fut: Future<Output = Result<String, String>> + 'static + Send,
    F: Fn(&str) -> Pin<Box<Fut>> + Sync + Send,
{
    let mut nodes: Vec<YamdNodes> = Vec::with_capacity(yamd.body.len());
    for node in yamd.body {
        match node {
            YamdNodes::Image(image) => {
                nodes.push(
                    yamd_image(&image, getter.clone())
                        .await?
                        .map_or_else(|| YamdNodes::Image(image), YamdNodes::Image),
                );
            }
            YamdNodes::Images(images) => {
                let mut new_images: Vec<Image> = Vec::with_capacity(images.body.len());
                for image in images.body {
                    new_images.push(yamd_image(&image, getter.clone()).await?.unwrap_or(image));
                }
                nodes.push(YamdNodes::Images(Images { body: new_images }));
            }
            YamdNodes::Collapsible(collapsible) => {
                let mut new_collapsible = collapsible.clone();
                new_collapsible.body = add_alt_text(
                    Yamd::new(yamd.metadata.clone(), collapsible.body),
                    getter.clone(),
                )
                .await?
                .body;
                nodes.push(YamdNodes::Collapsible(new_collapsible));
            }
            _ => {
                nodes.push(node.clone());
            }
        }
    }
    Ok(Yamd::new(yamd.metadata.clone(), nodes))
}

pub async fn generate_alt_text(
    (generator, pid, yamd, config, base_path): (
        Arc<Img2Text>,
        String,
        Yamd,
        Arc<AltTextGenerator>,
        Arc<PathBuf>,
    ),
) -> Result<(String, Yamd), BarErr> {
    let yamd = add_alt_text(
        yamd,
        Arc::from(|path: &str| {
            let cache: Cache<String> = Cache::new("alt_text", 1, &base_path);
            let generator = generator.clone();
            let path = path.to_string();
            let config = config.clone();
            let base_path = base_path.clone();
            Box::pin(async move {
                let cache_key = format!("{}:{}:{}", path, config.prompt, config.temperature);
                let cache_key = BASE64URL_NOPAD
                    .encode(seahash::hash(cache_key.as_bytes()).to_be_bytes().as_ref());
                let cache_key = format!(
                    "{}/{}/{}",
                    cache_key.chars().take(2).collect::<String>(),
                    cache_key.chars().skip(2).take(2).collect::<String>(),
                    cache_key
                );
                if let Some(cached) = cache.get(&cache_key).map_err(|e| e.to_string())? {
                    Ok(cached)
                } else {
                    let path_buf = str_to_path(&path, &base_path)
                        .await
                        .map_err(|e| e.to_string())?;
                    let alt_text = generator
                        .as_ref()
                        .run(&path_buf, &config.prompt, config.temperature)
                        .await
                        .map_err(|e| e.to_string())?
                        .to_string();

                    cache
                        .set(&cache_key, &alt_text)
                        .await
                        .map_err(|e| e.to_string())?;

                    Ok(alt_text)
                }
            })
        }),
    )
    .await?;
    Ok((pid, yamd))
}
