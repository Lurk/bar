use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use data_encoding::BASE64URL_NOPAD;
use futures_core::Stream;
use img2text::Img2Text;
use tokio_stream::StreamExt;
use tracing::debug;
use yamd::op::{Content, Node, Op, OpKind};

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

async fn generate_alt_for_image(
    src: &str,
    generator: &Img2Text,
    config: &AltTextGenerator,
    base_path: &Path,
) -> Result<String, BarErr> {
    let cache: Cache<String> = Cache::new("alt_text", 1, base_path);
    let cache_key = format!("{}:{}:{}", src, config.prompt, config.temperature);
    let cache_key =
        BASE64URL_NOPAD.encode(seahash::hash(cache_key.as_bytes()).to_be_bytes().as_ref());
    let cache_key = format!(
        "{}/{}/{}",
        cache_key.chars().take(2).collect::<String>(),
        cache_key.chars().skip(2).take(2).collect::<String>(),
        cache_key
    );

    if let Some(cached) = cache.get(&cache_key)? {
        return Ok(cached);
    }

    let path_buf = str_to_path(src, base_path).await?;
    let alt_text = generator
        .run(&path_buf, &config.prompt, config.temperature)
        .await
        .map_err(|e| BarErr::from(e.to_string()))?
        .to_string();

    cache.set(&cache_key, &alt_text).await?;
    Ok(alt_text)
}

pub fn add_alt_text<'a>(
    stream: Pin<Box<dyn Stream<Item = Result<Op, BarErr>> + Send + 'a>>,
    source: &'a str,
    generator: Arc<Img2Text>,
    config: Arc<AltTextGenerator>,
    base_path: Arc<PathBuf>,
) -> Pin<Box<dyn Stream<Item = Result<Op, BarErr>> + Send + 'a>> {
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

            match generate_alt_for_image(&src, &generator, &config, &base_path).await {
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
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarErr>> + Send>> =
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

    #[tokio::test]
    async fn image_with_existing_alt_passes_through() {
        let ops = make_image_ops_with_alt("existing alt", "img.jpg");
        let expected_len = ops.len();
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarErr>> + Send>> =
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
