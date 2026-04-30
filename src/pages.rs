use crate::{
    r#async::try_map,
    cloudinary::unwrap_cloudinary,
    context::BuildConfig,
    diagnostic::{BarDiagnostic, ContextExt},
    fs::{canonicalize_with_context, get_files_by_ext_deep},
    image_alt::add_alt_text,
    metadata::Metadata,
};

use futures_core::Stream;
use img2text::Img2Text;
use itertools::Itertools;
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
};
use tokio::fs::read_to_string;
use tokio_stream::StreamExt;
use tracing::info;
use url::Url;
use yamd::op::{self, Node, Op, OpKind};

#[derive(Debug, Serialize)]
pub struct Page {
    pub pid: Arc<str>,
    #[serde(skip)]
    pub ops: Vec<Op>,
    #[serde(skip)]
    pub source: String,
    pub metadata: Metadata,
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl Eq for Page {}

impl PartialOrd for Page {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Page {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // newest first
        match self.metadata.date.cmp(&other.metadata.date).reverse() {
            Ordering::Less => Ordering::Less,
            // if time is the same sort by pid
            Ordering::Equal => self.pid.cmp(&other.pid),
            Ordering::Greater => Ordering::Greater,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SliceNumber {
    number: usize,
    is_current: bool,
    display: usize,
}

#[derive(Debug, Serialize)]
pub struct PagesSlice {
    pages: BTreeSet<Arc<Page>>,
    current_slice: usize,
    total_slices: usize,
    numbers: Vec<SliceNumber>,
    slice_size: usize,
}

impl Page {
    #[must_use]
    pub fn new(pid: Arc<str>, ops: Vec<Op>, source: String, metadata: Metadata) -> Self {
        Self {
            pid,
            ops,
            source,
            metadata,
        }
    }

    #[must_use]
    pub fn get_title(&self) -> String {
        self.metadata.title.clone()
    }

    #[must_use]
    pub fn get_image(&self, base_url: &Url) -> Option<Url> {
        self.metadata.image.as_ref().and_then(|image| {
            if image.starts_with("http") {
                return Url::parse(image.as_str()).ok();
            }

            let mut url = base_url.clone();
            url.set_path(image.as_str());

            Some(url)
        })
    }
}

pub struct Pages {
    pages: HashMap<Arc<str>, Arc<Page>>,
    tags: HashMap<Arc<str>, BTreeSet<Arc<Page>>>,
}

impl Pages {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            tags: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: &str, ops: Vec<Op>, source: String, metadata: Metadata) {
        let pid: Arc<str> = Arc::from(key);
        self.push(Page::new(pid, ops, source, metadata));
    }

    pub fn push(&mut self, page: Page) {
        let page = Arc::new(page);

        self.pages.insert(page.pid.clone(), page.clone());

        let Some(tags) = &page.metadata.tags else {
            return;
        };

        for tag in tags {
            self.tags
                .entry(tag.clone())
                .and_modify(|pages| {
                    pages.insert(page.clone());
                })
                .or_insert(BTreeSet::from([page.clone()]));
        }
    }

    #[must_use]
    pub fn keys(&self) -> Vec<Arc<str>> {
        self.pages.keys().cloned().collect()
    }

    #[must_use]
    pub fn get(&self, pid: &str) -> Option<&Page> {
        self.pages.get(pid).map(std::convert::AsRef::as_ref)
    }

    #[must_use]
    pub fn get_tags(&self) -> HashSet<Arc<str>> {
        let mut tags: HashSet<Arc<str>> = HashSet::new();

        self.tags.keys().for_each(|tag| {
            tags.insert(tag.clone());
        });
        tags
    }

    #[must_use]
    pub fn get_posts_by_tag(&self, tag: &str, limit: usize, offset: usize) -> Option<PagesSlice> {
        let pages = self.tags.get(tag)?;

        let current_slice = offset / limit;
        let total_slices: usize = pages.len().div_ceil(limit);

        let mut numbers: Vec<SliceNumber> = Vec::with_capacity(total_slices);

        for i in 0..total_slices {
            numbers.push(SliceNumber {
                number: i,
                display: i + 1,
                is_current: i == current_slice,
            });
        }
        let mut slice = PagesSlice {
            pages: BTreeSet::new(),
            current_slice,
            total_slices,
            slice_size: limit,
            numbers,
        };

        for page in pages.iter().skip(offset).take(limit) {
            slice.pages.insert(page.clone());
        }
        Some(slice)
    }

    #[must_use]
    pub fn get_similar(&self, pid: &str, max: usize) -> Vec<Arc<str>> {
        let Some(page) = self.get(pid) else {
            return vec![];
        };

        let Some(tags) = page.metadata.tags.as_ref() else {
            return vec![];
        };

        let mut leaderboard: HashMap<Arc<str>, usize> = HashMap::new();

        for tag in tags {
            let Some(tag_pages) = self.tags.get(tag) else {
                continue;
            };
            for other in tag_pages {
                if page.pid == other.pid {
                    continue;
                }

                leaderboard
                    .entry(other.pid.clone())
                    .and_modify(|score| *score += 1)
                    .or_insert(1);
            }
        }

        let leaderboard: Vec<(&Arc<str>, &usize)> = leaderboard
            .iter()
            .sorted_by(|(_, left), (_, right)| right.cmp(left))
            .collect();

        leaderboard
            .iter()
            .take(max)
            .map(|(pid, _)| (*pid).clone())
            .collect()
    }
}

impl Default for Pages {
    fn default() -> Self {
        Self::new()
    }
}

async fn path_to_yamd(
    (path, content_path): (PathBuf, Arc<PathBuf>),
) -> Result<(String, String, Vec<Op>), BarDiagnostic> {
    let path = canonicalize_with_context(&path).await?;
    let file_contents = read_to_string(&path).await?;

    let ops = op::parse(&file_contents);

    let path_no_ext = path.with_extension("");
    let path_str = path_no_ext.to_str().ok_or_else(|| {
        BarDiagnostic::from(format!(
            "path is not valid UTF-8: {}",
            path_no_ext.display()
        ))
    })?;
    let content_str = content_path.to_str().ok_or_else(|| {
        BarDiagnostic::from(format!(
            "content path is not valid UTF-8: {}",
            content_path.display()
        ))
    })?;
    let pid = path_str
        .trim_start_matches(content_str)
        .to_string()
        .replace('\\', "/");

    Ok((pid, file_contents, ops))
}

fn extract_metadata<'a>(ops: &'a [Op], source: &'a str) -> Option<&'a str> {
    let mut in_metadata = false;
    for op in ops {
        match &op.kind {
            OpKind::Start(Node::Metadata) => in_metadata = true,
            OpKind::Value if in_metadata => return Some(op.content.as_str(source)),
            OpKind::End(Node::Metadata) => return None,
            _ => {}
        }
    }
    None
}

/// # Errors
/// Returns error if content files cannot be read or parsed.
pub async fn init_pages(build_config: &BuildConfig) -> Result<Arc<Pages>, BarDiagnostic> {
    let base_path = Arc::new(build_config.path.clone());
    let content_path = Arc::new(
        canonicalize_with_context(&build_config.path.join(&build_config.config.content_path))
            .await?,
    );
    info!("processing YAMD from {:?}", content_path);
    let input = get_files_by_ext_deep(&content_path, &["yamd"])
        .await?
        .into_iter()
        .map(|path| (path, content_path.clone()));

    let pages_vec = try_map(50, input, path_to_yamd).await?;
    info!("processing YAMD complete");

    let should_generate_alt_text = build_config
        .config
        .yamd_processors
        .generate_alt_text
        .is_some();

    let convert_cloudinary = build_config.config.yamd_processors.convert_cloudinary_embed;

    let alt_text_config = build_config
        .config
        .yamd_processors
        .generate_alt_text
        .as_ref()
        .map(|config| {
            let generator = Arc::from(Img2Text::new());
            let config = Arc::new(config.clone());
            (generator, config)
        });

    let mut pages = Pages::new();

    for (pid, source_text, ops) in pages_vec {
        let stream: Pin<Box<dyn Stream<Item = Result<Op, BarDiagnostic>> + Send>> =
            Box::pin(tokio_stream::iter(ops.into_iter().map(Ok)));

        let stream = if convert_cloudinary {
            unwrap_cloudinary(
                stream,
                &source_text,
                should_generate_alt_text,
                base_path.clone(),
            )
        } else {
            stream
        };

        let stream = if let Some((ref generator, ref config)) = alt_text_config {
            add_alt_text(
                stream,
                &source_text,
                generator.clone(),
                config.clone(),
                base_path.clone(),
            )
        } else {
            stream
        };

        let ops: Vec<Op> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("processing content file: {pid}.yamd"))?;

        let metadata_str = extract_metadata(&ops, &source_text)
            .ok_or_else(|| BarDiagnostic::from(format!("{pid} is missing metadata")))?;
        let metadata: Metadata = serde_yaml::from_str(metadata_str)
            .map_err(|e| BarDiagnostic::from(format!("{pid} has invalid yaml metadata: {e}")))?;

        if metadata.is_draft.unwrap_or(false) {
            info!("skipping draft: {pid}");
            continue;
        }

        pages.add(&pid, ops, source_text, metadata);
    }

    Ok(Arc::new(pages))
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use chrono::prelude::*;

    use crate::{config::Config, context::BuildConfig, metadata::Metadata, pages::init_pages};

    use super::{Page, Pages};

    #[tokio::test]
    async fn init_from_path_test() {
        let config_path = Path::new("./test/fixtures/").to_path_buf();
        let build_config = BuildConfig {
            config: Config::try_from(&config_path).unwrap(),
            path: config_path,
        };
        let pages = init_pages(&build_config).await.unwrap();

        assert_eq!(pages.keys().len(), 2);
        assert_eq!(
            pages.get("/test").unwrap().get_title(),
            "test 1".to_string()
        );
        assert_eq!(
            pages.get("/test2").unwrap().get_title(),
            "test 2".to_string()
        );
        assert_eq!(pages.get_tags().len(), 4);
    }

    #[tokio::test]
    async fn init_pages_excludes_drafts() {
        let config_path = Path::new("./test/fixtures/").to_path_buf();
        let build_config = BuildConfig {
            config: Config::try_from(&config_path).unwrap(),
            path: config_path,
        };
        let pages = init_pages(&build_config).await.unwrap();

        assert!(
            pages.get("/draft").is_none(),
            "draft yamd must be excluded from Pages, got keys: {:?}",
            pages.keys()
        );
        assert!(
            !pages
                .get_tags()
                .iter()
                .any(|t| t.as_ref() == "draft only tag"),
            "tags from a draft must not appear in get_tags(): {:?}",
            pages.get_tags()
        );
    }

    #[test]
    fn get_similar() {
        let mut pages = Pages::new();
        pages.push(Page::new(
            "1".into(),
            vec![],
            String::new(),
            Metadata {
                title: "1".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        ));
        pages.push(Page::new(
            "2".into(),
            vec![],
            String::new(),
            Metadata {
                title: "2".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t7".into()]),
                is_draft: None,
            },
        ));
        pages.push(Page::new(
            "3".into(),
            vec![],
            String::new(),
            Metadata {
                title: "3".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        ));
        pages.push(Page::new(
            "4".into(),
            vec![],
            String::new(),
            Metadata {
                title: "4".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t5".into()]),
                is_draft: None,
            },
        ));
        pages.push(Page::new(
            "5".into(),
            vec![],
            String::new(),
            Metadata {
                title: "5".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec![
                    "t1".into(),
                    "t2".into(),
                    "t3".into(),
                    "t4".into(),
                    "t5".into(),
                ]),
                is_draft: None,
            },
        ));
        pages.push(Page::new(
            "6".into(),
            vec![],
            String::new(),
            Metadata {
                title: "6".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t3".into(), "t5".into()]),
                is_draft: None,
            },
        ));

        assert_eq!(
            pages.get_similar("1", 3),
            vec!["5".into(), "3".into(), "6".into()]
        );
    }

    #[test]
    fn cmp_for_page_with_different_times() {
        let one = Page::new(
            "1".into(),
            vec![],
            String::new(),
            Metadata {
                title: "1".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        );
        let two = Page::new(
            "2".into(),
            vec![],
            String::new(),
            Metadata {
                title: "2".into(),
                date: Utc::now().into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        );

        assert!(one > two);
    }

    #[test]
    fn cmp_for_page_with_same_time() {
        let time = Utc::now();

        let one = Page::new(
            "1".into(),
            vec![],
            String::new(),
            Metadata {
                title: "1".into(),
                date: time.into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        );
        let two = Page::new(
            "2".into(),
            vec![],
            String::new(),
            Metadata {
                title: "2".into(),
                date: time.into(),
                image: None,
                preview: None,
                tags: Some(vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()]),
                is_draft: None,
            },
        );

        assert!(one < two);
    }
}
