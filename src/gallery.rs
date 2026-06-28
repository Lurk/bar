use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use futures_core::Stream;
use numeric_sort::cmp;
use tokio_stream::StreamExt;
use tracing::warn;
use yamd::{
    nodes::{Image, Images},
    op::{Node, Op, OpKind},
};

use crate::{
    diagnostic::{BarDiagnostic, ContextExt},
    gallery_ops::images_to_ops,
};

fn resolve_folder(raw: &str) -> Result<String, BarDiagnostic> {
    let rel = crate::fs::normalize_project_rel(raw).map_err(BarDiagnostic::from)?;
    if rel.is_empty() {
        return Err(BarDiagnostic::from(format!(
            "gallery requires a folder path, got '{raw}'"
        )));
    }
    Ok(rel)
}

pub(crate) async fn gallery_to_images(
    args: &str,
    base_path: &Path,
) -> Result<Images, BarDiagnostic> {
    let rel = resolve_folder(args)?;
    let folder = base_path.join(&rel);
    let mut entries = tokio::fs::read_dir(&folder)
        .await
        .map_err(|e| BarDiagnostic::from(format!("gallery folder '{}': {e}", folder.display())))?;

    let mut names: Vec<String> = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        BarDiagnostic::from(format!(
            "reading gallery folder '{}': {e}",
            folder.display()
        ))
    })? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if !entry.metadata().await.is_ok_and(|m| m.is_file()) {
            continue;
        }
        let ext = Path::new(name.as_ref() as &str)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some(e) if crate::templating::output_format_for_ext(e).is_some() => {
                names.push(name.into_owned());
            }
            _ => warn!(
                "gallery: skipping non-image file '{name}' in '{}'",
                folder.display()
            ),
        }
    }

    if names.is_empty() {
        return Err(BarDiagnostic::from(format!(
            "gallery folder '{}' contains no image files",
            folder.display()
        )));
    }

    names.sort_by(|a, b| cmp(a, b));

    let base = format!("/{rel}");
    let images = names
        .into_iter()
        .map(|name| Image::new(String::new(), format!("{base}/{name}")))
        .collect::<Vec<Image>>();
    Ok(Images::new(images))
}

pub fn unwrap_gallery<'a>(
    stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send + 'a>>,
    source: &'a str,
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

            // Complete embed buffered: Start, Value(kind), Value(|), Value(args), End
            in_embed = false;

            let is_gallery = buffer.get(1).is_some_and(|op| {
                matches!(op.kind, OpKind::Value)
                    && op.content.as_str(source) == "gallery"
            });

            if !is_gallery {
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

            match gallery_to_images(&args, &base_path)
                .await
                .with_context(|| format!("processing gallery embed with args: {args}"))
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
    use std::fs;
    use tokio_stream::iter;

    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"x").unwrap();
    }

    fn collect(stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>>) -> Vec<Op> {
        // tests are sync; drive the stream on a small runtime
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async {
                stream
                    .collect::<Vec<_>>()
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            })
    }

    #[tokio::test]
    async fn builds_sorted_images_with_rooted_srcs_and_empty_alt() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("photos/trip");
        fs::create_dir_all(&gallery).unwrap();
        // Out-of-order + numeric names to prove numeric_sort ordering.
        touch(&gallery, "img10.jpg");
        touch(&gallery, "img2.jpg");
        touch(&gallery, "img1.png");

        let images = gallery_to_images("/photos/trip", dir.path()).await.unwrap();
        let srcs: Vec<&str> = images.body.iter().map(|i| i.src.as_str()).collect();
        assert_eq!(
            srcs,
            vec![
                "/photos/trip/img1.png",
                "/photos/trip/img2.jpg",
                "/photos/trip/img10.jpg",
            ]
        );
        assert!(images.body.iter().all(|i| i.alt.is_empty()));
    }

    #[tokio::test]
    async fn non_image_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");
        touch(&gallery, "README.md");
        touch(&gallery, "notes.txt");

        let images = gallery_to_images("/g", dir.path()).await.unwrap();
        let srcs: Vec<&str> = images.body.iter().map(|i| i.src.as_str()).collect();
        assert_eq!(srcs, vec!["/g/a.jpg"]);
    }

    #[tokio::test]
    async fn formats_imgtools_cannot_process_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");
        touch(&gallery, "anim.gif");
        touch(&gallery, "logo.svg");

        let images = gallery_to_images("/g", dir.path()).await.unwrap();
        let srcs: Vec<&str> = images.body.iter().map(|i| i.src.as_str()).collect();
        assert_eq!(srcs, vec!["/g/a.jpg"]);
    }

    #[tokio::test]
    async fn directories_with_image_extensions_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");
        fs::create_dir_all(gallery.join("thumbs.png")).unwrap();

        let images = gallery_to_images("/g", dir.path()).await.unwrap();
        let srcs: Vec<&str> = images.body.iter().map(|i| i.src.as_str()).collect();
        assert_eq!(srcs, vec!["/g/a.jpg"]);
    }

    #[tokio::test]
    async fn messy_args_normalize_in_both_read_and_src() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("photos/trip");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");

        for arg in [
            "photos/./trip",
            "photos//trip",
            "photos/x/../trip",
            "/photos/trip/",
        ] {
            let images = gallery_to_images(arg, dir.path()).await.unwrap();
            assert_eq!(
                images
                    .body
                    .iter()
                    .map(|i| i.src.as_str())
                    .collect::<Vec<_>>(),
                vec!["/photos/trip/a.jpg"],
                "arg {arg:?}"
            );
        }
    }

    #[tokio::test]
    async fn dotfiles_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");
        touch(&gallery, ".DS_Store");

        let images = gallery_to_images("/g", dir.path()).await.unwrap();
        assert_eq!(images.body.len(), 1);
        assert_eq!(images.body[0].src, "/g/a.jpg");
    }

    #[tokio::test]
    async fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            gallery_to_images("/does-not-exist", dir.path())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn empty_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "README.md"); // present but no image files
        assert!(gallery_to_images("/g", dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn escaping_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(gallery_to_images("/../etc", dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn empty_args_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(gallery_to_images("", dir.path()).await.is_err());
        assert!(gallery_to_images("/", dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn args_without_leading_slash_works() {
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        touch(&gallery, "a.jpg");

        let images = gallery_to_images("g", dir.path()).await.unwrap();
        assert_eq!(images.body.len(), 1);
        assert_eq!(images.body[0].src, "/g/a.jpg");
    }

    #[test]
    fn non_embed_ops_pass_through() {
        use yamd::op::{Content, Node};
        let ops = vec![
            Op::new_start(Node::Paragraph, Content::Span(0..0)),
            Op::new_value(Content::Materialized("hello".into())),
            Op::new_end(Node::Paragraph, Content::Span(0..0)),
        ];
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result = collect(unwrap_gallery(stream, "", Arc::new(PathBuf::new())));
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].kind, OpKind::Start(Node::Paragraph));
    }

    #[test]
    fn non_gallery_embed_passes_through() {
        use yamd::op::{Content, Node};
        let ops = vec![
            Op::new_start(Node::Embed, Content::Span(0..0)),
            Op::new_value(Content::Materialized("youtube".into())),
            Op::new_value(Content::Materialized("|".into())),
            Op::new_value(Content::Materialized("abc123".into())),
            Op::new_end(Node::Embed, Content::Span(0..0)),
        ];
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result = collect(unwrap_gallery(stream, "", Arc::new(PathBuf::new())));
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].kind, OpKind::Start(Node::Embed));
        assert_eq!(result[1].content, Content::Materialized("youtube".into()));
    }

    #[test]
    fn gallery_embed_expands_to_images() {
        use std::fs;
        use yamd::op::{Content, Node};
        let dir = tempfile::tempdir().unwrap();
        let gallery = dir.path().join("g");
        fs::create_dir_all(&gallery).unwrap();
        fs::write(gallery.join("a.jpg"), b"x").unwrap();
        fs::write(gallery.join("b.jpg"), b"x").unwrap();

        let source = "gallery|/g";
        let ops = vec![
            Op::new_start(Node::Embed, Content::Span(0..0)),
            Op::new_value(Content::Materialized("gallery".into())),
            Op::new_value(Content::Materialized("|".into())),
            Op::new_value(Content::Materialized("/g".into())),
            Op::new_end(Node::Embed, Content::Span(0..0)),
        ];
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(iter(ops.into_iter().map(Ok)));
        let result = collect(unwrap_gallery(
            stream,
            source,
            Arc::new(dir.path().to_path_buf()),
        ));

        assert_eq!(result.first().unwrap().kind, OpKind::Start(Node::Images));
        assert_eq!(result.last().unwrap().kind, OpKind::End(Node::Images));
        let dest_values: Vec<&str> = result
            .iter()
            .zip(result.iter().skip(1))
            .filter(|(a, _)| a.kind == OpKind::Start(Node::Destination))
            .map(|(_, b)| b.content.as_str(source))
            .collect();
        assert_eq!(dest_values, vec!["/g/a.jpg", "/g/b.jpg"]);
    }
}
