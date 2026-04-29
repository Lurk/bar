use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use futures_core::Stream;
use img2text::Img2Text;
use tokio_stream::StreamExt;
use tracing::debug;
use yamd::op::{Content, Node, Op, OpKind};

use crate::{
    cache::Cache,
    config::AltTextGenerator,
    diagnostic::{BarDiagnostic, ContextExt},
};

fn ext_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
}

fn ext_from_content_type(content_type: &str) -> Option<String> {
    let mime = content_type.split(';').next()?.trim();
    match mime {
        "image/jpeg" => Some("jpg".to_string()),
        "image/png" => Some("png".to_string()),
        "image/gif" => Some("gif".to_string()),
        "image/webp" => Some("webp".to_string()),
        "image/svg+xml" => Some("svg".to_string()),
        "image/avif" => Some("avif".to_string()),
        "image/tiff" => Some("tiff".to_string()),
        "image/bmp" => Some("bmp".to_string()),
        _ => None,
    }
}

async fn str_to_path(path: &str, base_path: &Path) -> Result<PathBuf, BarDiagnostic> {
    if !path.starts_with("http") {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(format!("Image file does not exist: {}", path.display()).into());
        }
        return Ok(path);
    }

    let cache: Cache<()> = Cache::new("remote_images", 1, base_path);
    let key = Cache::<()>::make_key(path);

    if let Some(url_ext) = ext_from_url(path) {
        let dest = cache.raw_path(&key, &url_ext);
        if dest.exists() {
            return Ok(dest);
        }
    }

    debug!("Downloading image from URL: {}", path);
    let response = reqwest::get(path).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to fetch image, status code: {}", response.status()).into());
    }

    let ext = ext_from_url(path)
        .or_else(|| {
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(ext_from_content_type)
        })
        .ok_or_else(|| {
            BarDiagnostic::from(format!(
                "Cannot determine file extension for remote image: {path}"
            ))
        })?;

    let bytes = response.bytes().await?;
    cache.set_raw(&key, &ext, &bytes).await?;

    let dest = cache.raw_path(&key, &ext);
    debug!("Saved image to cache: {:?}", dest);
    Ok(dest)
}

async fn generate_alt_for_image(
    src: &str,
    generator: &Img2Text,
    config: &AltTextGenerator,
    base_path: &Path,
) -> Result<String, BarDiagnostic> {
    let cache: Cache<String> = Cache::new("alt_text", 1, base_path);
    let cache_key =
        Cache::<String>::make_key(&format!("{}:{}:{}", src, config.prompt, config.temperature));

    if let Some(cached) = cache.get(&cache_key)? {
        return Ok(cached);
    }

    let path_buf = str_to_path(src, base_path).await?;
    let alt_text = generator
        .run(&path_buf, &config.prompt, config.temperature)
        .await
        .map_err(|e| BarDiagnostic::from(e.to_string()))?
        .to_string();

    cache.set(&cache_key, &alt_text).await?;
    Ok(alt_text)
}

pub fn add_alt_text<'a>(
    stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send + 'a>>,
    source: &'a str,
    generator: Arc<Img2Text>,
    config: Arc<AltTextGenerator>,
    base_path: Arc<PathBuf>,
) -> Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send + 'a>> {
    let mut buffer: Vec<Op> = Vec::new();
    let mut in_image = false;

    Box::pin(async_stream::stream! {
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            let op = match item {
                Ok(op) => op,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            if !in_image {
                if matches!(op.kind, OpKind::Start(Node::Image)) {
                    in_image = true;
                    buffer.push(op);
                    continue;
                }
                yield Ok(op);
                continue;
            }

            buffer.push(op);

            if !matches!(buffer.last().unwrap().kind, OpKind::End(Node::Image)) {
                continue;
            }

            in_image = false;

            // Find the title value (index 2: Start(Image), Start(Title), Value(alt), ...)
            let alt_is_empty = buffer
                .get(2)
                .is_some_and(|op| {
                    matches!(op.kind, OpKind::Value) && op.content.as_str(source).is_empty()
                });

            if !alt_is_empty {
                for buffered_op in buffer.drain(..) {
                    yield Ok(buffered_op);
                }
                continue;
            }

            // Find the src value (index 5: ..., Start(Destination), Value(src), ...)
            let src = buffer
                .get(5)
                .map(|op| op.content.as_str(source).to_owned())
                .unwrap_or_default();

            debug!("no alt text found for image: {}", src);

            match generate_alt_for_image(&src, &generator, &config, &base_path)
                .await
                .with_context(|| format!("generating alt text for image: {src}"))
            {
                Ok(alt_text) => {
                    // Replace the title value op with the generated alt text
                    buffer[2] = Op::new_value(Content::Materialized(alt_text));
                    for buffered_op in buffer.drain(..) {
                        yield Ok(buffered_op);
                    }
                }
                Err(e) => {
                    yield Err(e);
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::iter;

    fn make_image_ops_with_alt(alt: &str, src: &str) -> Vec<Op> {
        vec![
            Op::new_start(Node::Image, Content::Span(0..0)),
            Op::new_start(Node::Title, Content::Span(0..0)),
            Op::new_value(Content::Materialized(alt.into())),
            Op::new_end(Node::Title, Content::Span(0..0)),
            Op::new_start(Node::Destination, Content::Span(0..0)),
            Op::new_value(Content::Materialized(src.into())),
            Op::new_end(Node::Destination, Content::Span(0..0)),
            Op::new_end(Node::Image, Content::Span(0..0)),
        ]
    }

    fn make_paragraph_ops() -> Vec<Op> {
        vec![
            Op::new_start(Node::Paragraph, Content::Span(0..0)),
            Op::new_value(Content::Materialized("hello".into())),
            Op::new_end(Node::Paragraph, Content::Span(0..0)),
        ]
    }

    fn stub_generator() -> Arc<Img2Text> {
        Arc::from(Img2Text::new())
    }

    fn stub_config() -> Arc<AltTextGenerator> {
        Arc::new(AltTextGenerator {
            prompt: "describe".into(),
            temperature: 0.1,
        })
    }

    #[tokio::test]
    async fn non_image_ops_pass_through() {
        let ops = make_paragraph_ops();
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result: Vec<Op> = add_alt_text(
            stream,
            "",
            stub_generator(),
            stub_config(),
            Arc::new(PathBuf::new()),
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].kind, OpKind::Start(Node::Paragraph));
    }

    #[test]
    fn ext_from_content_type_jpeg() {
        assert_eq!(ext_from_content_type("image/jpeg"), Some("jpg".to_string()));
    }

    #[test]
    fn ext_from_content_type_png() {
        assert_eq!(ext_from_content_type("image/png"), Some("png".to_string()));
    }

    #[test]
    fn ext_from_content_type_webp() {
        assert_eq!(
            ext_from_content_type("image/webp"),
            Some("webp".to_string())
        );
    }

    #[test]
    fn ext_from_content_type_with_charset() {
        assert_eq!(
            ext_from_content_type("image/jpeg; charset=utf-8"),
            Some("jpg".to_string())
        );
    }

    #[test]
    fn ext_from_content_type_unknown() {
        assert_eq!(ext_from_content_type("application/octet-stream"), None);
    }

    #[test]
    fn ext_from_url_simple_path() {
        assert_eq!(
            ext_from_url("https://example.com/photo.jpg"),
            Some("jpg".to_string())
        );
    }

    #[test]
    fn ext_from_url_with_query_params() {
        assert_eq!(
            ext_from_url("https://example.com/photo.png?w=100&h=200"),
            Some("png".to_string())
        );
    }

    #[test]
    fn ext_from_url_no_extension() {
        assert_eq!(ext_from_url("https://example.com/image/12345"), None);
    }

    #[test]
    fn ext_from_url_local_path() {
        assert_eq!(
            ext_from_url("/local/path/image.webp"),
            Some("webp".to_string())
        );
    }

    #[tokio::test]
    async fn image_with_existing_alt_passes_through() {
        let ops = make_image_ops_with_alt("existing alt", "img.jpg");
        let expected_len = ops.len();
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result: Vec<Op> = add_alt_text(
            stream,
            "",
            stub_generator(),
            stub_config(),
            Arc::new(PathBuf::new()),
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(result.len(), expected_len);
        assert_eq!(result[0].kind, OpKind::Start(Node::Image));
        assert_eq!(
            result[2].content,
            Content::Materialized("existing alt".into())
        );
    }
}
