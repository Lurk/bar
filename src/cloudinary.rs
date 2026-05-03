use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use cloudinary::{tags::get_tags, transformation::Image as CloudinaryImage};
use futures_core::Stream;
use numeric_sort::cmp;
use tokio_stream::StreamExt;
use yamd::{
    nodes::{Image, Images},
    op::{Content, Node, Op, OpKind},
};

use crate::{
    cache::Cache,
    diagnostic::{BarDiagnostic, ContextExt},
};

fn image_to_ops(image: &Image) -> Vec<Op> {
    vec![
        Op::new_start(Node::Image, Content::Span(0..0)),
        Op::new_start(Node::Title, Content::Span(0..0)),
        Op::new_value(Content::Materialized(image.alt.clone())),
        Op::new_end(Node::Title, Content::Span(0..0)),
        Op::new_start(Node::Destination, Content::Span(0..0)),
        Op::new_value(Content::Materialized(image.src.clone())),
        Op::new_end(Node::Destination, Content::Span(0..0)),
        Op::new_end(Node::Image, Content::Span(0..0)),
    ]
}

fn images_to_ops(images: &Images) -> Vec<Op> {
    let mut ops = vec![Op::new_start(Node::Images, Content::Span(0..0))];
    for image in &images.body {
        ops.extend(image_to_ops(image));
    }
    ops.push(Op::new_end(Node::Images, Content::Span(0..0)));
    ops
}

async fn cloudinary_gallery_to_images(
    args: &str,
    should_alt_text_be_empty: bool,
    base_path: &Path,
) -> Result<Images, BarDiagnostic> {
    let cache = Cache::<Images>::new("cloudinary_gallery", 1, base_path);

    if let Some(images) = cache.get(args)? {
        return Ok(images);
    }

    let Some((cloud_name, tag)) = args.split_once('&') else {
        return Err(
            "cloudinary_gallery embed must have two arguments: cloud_name and tag separated by '&'."
                .into(),
        );
    };

    let mut tags = get_tags(cloud_name.into(), tag.into())
        .await
        .map_err(|e| BarDiagnostic::from(format!("error loading cloudinary tag '{tag}': {e}")))?;

    tags.resources
        .sort_by(|a, b| cmp(&a.public_id, &b.public_id));

    let images = tags
        .resources
        .iter()
        .map(|resource| {
            let mut image = CloudinaryImage::new(cloud_name.into(), resource.public_id.clone());
            image.set_format(resource.format.as_ref());
            let alt_text = if should_alt_text_be_empty {
                String::new()
            } else {
                resource.public_id.to_string()
            };
            Image::new(alt_text, image.to_string())
        })
        .collect::<Vec<Image>>();
    let images = Images::new(images);
    cache.set(args, &images).await?;
    Ok(images)
}

pub fn unwrap_cloudinary<'a>(
    stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send + 'a>>,
    source: &'a str,
    should_alt_text_be_empty: bool,
    base_path: Arc<PathBuf>,
) -> Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send + 'a>> {
    let mut buffer: Vec<Op> = Vec::new();
    let mut in_embed = false;

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

            if !in_embed {
                if matches!(op.kind, OpKind::Start(Node::Embed)) {
                    in_embed = true;
                    buffer.push(op);
                    continue;
                }
                yield Ok(op);
                continue;
            }

            buffer.push(op);

            if !matches!(buffer.last().unwrap().kind, OpKind::End(Node::Embed)) {
                continue;
            }

            // We have a complete embed: Start, Value(kind), Value(|), Value(args), End
            in_embed = false;

            let is_cloudinary = buffer
                .get(1)
                .is_some_and(|op| {
                    matches!(op.kind, OpKind::Value)
                        && op.content.as_str(source) == "cloudinary_gallery"
                });

            if !is_cloudinary {
                for buffered_op in buffer.drain(..) {
                    yield Ok(buffered_op);
                }
                continue;
            }

            let args = buffer
                .get(3)
                .map(|op| op.content.as_str(source).to_owned())
                .unwrap_or_default();

            buffer.clear();

            match cloudinary_gallery_to_images(&args, should_alt_text_be_empty, &base_path)
                .await
                .with_context(|| format!("processing cloudinary_gallery embed with args: {args}"))
            {
                Ok(images) => {
                    for op in images_to_ops(&images) {
                        yield Ok(op);
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

    fn make_paragraph_ops() -> Vec<Op> {
        vec![
            Op::new_start(Node::Paragraph, Content::Span(0..0)),
            Op::new_value(Content::Materialized("hello".into())),
            Op::new_end(Node::Paragraph, Content::Span(0..0)),
        ]
    }

    #[tokio::test]
    async fn non_embed_ops_pass_through() {
        let ops = make_paragraph_ops();
        let expected_len = ops.len();
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result: Vec<Op> = unwrap_cloudinary(stream, "", false, Arc::new(PathBuf::new()))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(result.len(), expected_len);
        assert_eq!(result[0].kind, OpKind::Start(Node::Paragraph));
        assert_eq!(result[1].kind, OpKind::Value);
        assert_eq!(result[2].kind, OpKind::End(Node::Paragraph));
    }

    fn make_youtube_embed_ops() -> Vec<Op> {
        vec![
            Op::new_start(Node::Embed, Content::Span(0..0)),
            Op::new_value(Content::Materialized("youtube".into())),
            Op::new_value(Content::Materialized("|".into())),
            Op::new_value(Content::Materialized("abc123".into())),
            Op::new_end(Node::Embed, Content::Span(0..0)),
        ]
    }

    #[tokio::test]
    async fn non_cloudinary_embed_passes_through() {
        let ops = make_youtube_embed_ops();
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result: Vec<Op> = unwrap_cloudinary(stream, "", false, Arc::new(PathBuf::new()))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].kind, OpKind::Start(Node::Embed));
        assert_eq!(result[1].content, Content::Materialized("youtube".into()));
        assert_eq!(result[4].kind, OpKind::End(Node::Embed));
    }
}
