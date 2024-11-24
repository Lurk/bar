use crate::{
    cloudinary::unwrap_cloudinary,
    config::Config,
    error::Errors,
    fs::{canonicalize_with_context, get_files_by_ext_deep},
    metadata::Metadata,
    r#async::try_map,
};

use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs::read_to_string;
use tracing::info;
use url::Url;
use yamd::{deserialize, nodes::Yamd};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Page {
    pub pid: Arc<str>,
    pub content: Yamd,
    pub metadata: Metadata,
}

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
    pub fn new(pid: Arc<str>, content: Yamd, metadata: Metadata) -> Self {
        Self {
            pid,
            content,
            metadata,
        }
    }

    pub fn get_title(&self) -> String {
        self.metadata.title.clone()
    }

    pub fn get_image(&self, base_url: &Url) -> Option<Url> {
        self.metadata.image.as_ref().map(|image| {
            if image.starts_with("http") {
                return Url::parse(image.as_str()).unwrap();
            }

            let mut url = base_url.clone();
            url.set_path(image.as_str());

            url
        })
    }
}

pub struct Pages {
    pages: HashMap<Arc<str>, Arc<Page>>,
    tags: HashMap<Arc<str>, BTreeSet<Arc<Page>>>,
}

impl Pages {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            tags: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: String, value: Yamd) {
        let pid: Arc<str> = Arc::from(key.as_str());
        let metadata = serde_yaml::from_str(
            value
                .metadata
                .as_ref()
                .unwrap_or_else(|| panic!("{pid} to have metadata"))
                .as_str(),
        )
        .unwrap_or_else(|_| panic!("{pid} to have valid yaml metadata"));

        self.push(Page::new(pid.clone(), value, metadata));
    }

    pub fn push(&mut self, page: Page) {
        let page = Arc::new(page);

        self.pages.insert(page.pid.clone(), page.clone());

        let Some(tags) = &page.metadata.tags else {
            return;
        };

        tags.iter().for_each(|tag| {
            self.tags
                .entry(tag.clone())
                .and_modify(|pages| {
                    pages.insert(page.clone());
                })
                .or_insert(BTreeSet::from([page.clone()]));
        });
    }

    pub fn keys(&self) -> Vec<Arc<str>> {
        self.pages.keys().cloned().collect()
    }

    pub fn get(&self, pid: &str) -> Option<&Page> {
        self.pages.get(pid).map(|page| page.as_ref())
    }

    pub fn get_tags(&self) -> HashSet<Arc<str>> {
        let mut tags: HashSet<Arc<str>> = HashSet::new();

        self.tags.keys().for_each(|tag| {
            tags.insert(tag.clone());
        });
        tags
    }

    pub fn get_posts_by_tag(&self, tag: &str, limit: usize, offset: usize) -> PagesSlice {
        let pages = self
            .tags
            .get(tag)
            .unwrap_or_else(|| panic!("{tag} must be present"));

        let current_slice = offset / limit;
        let total_slices: usize = (pages.len() as f64 / limit as f64).ceil() as usize;

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
        slice
    }

    pub fn get_similar(&self, pid: &str, max: usize) -> Vec<Arc<str>> {
        let Some(page) = self.get(pid) else {
            return vec![];
        };

        let Some(tags) = page.metadata.tags.as_ref() else {
            return vec![];
        };

        let mut leaderboard: HashMap<Arc<str>, usize> = HashMap::new();

        for tag in tags.iter() {
            for other in self
                .tags
                .get(tag)
                .unwrap_or_else(|| panic!("{tag} must be present"))
            {
                if page.pid == other.pid {
                    continue;
                }

                let Some(other_tags) = other.metadata.tags.as_ref() else {
                    continue;
                };

                leaderboard.entry(other.pid.clone()).or_insert_with(|| {
                    other_tags
                        .iter()
                        .filter(|other_tag| tags.contains(other_tag))
                        .count()
                });
            }
        }

        let mut leaderboard: Vec<(&Arc<str>, &usize)> = leaderboard.iter().collect();
        leaderboard.sort_by(|(_, left), (_, right)| right.cmp(left));

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
) -> Result<(String, Yamd), Errors> {
    let path = canonicalize_with_context(&path).await?;
    let file_contents = read_to_string(&path).await?;

    let yamd = deserialize(file_contents.as_str());

    let pid = path
        .with_extension("")
        .to_str()
        .unwrap()
        .trim_start_matches(content_path.to_str().unwrap())
        .to_string()
        .replace('\\', "/");

    Ok((pid, yamd))
}

pub async fn init_pages(path: &Path, config: Arc<Config>) -> Result<Arc<Pages>, Errors> {
    let content_path = Arc::new(canonicalize_with_context(&path.join(&config.content_path)).await?);
    info!("processing YAMD from {:?}", content_path);
    let input = get_files_by_ext_deep(&content_path, &["yamd"])
        .await?
        .into_iter()
        .map(|path| (path, content_path.clone()))
        .collect();

    let mut pages_vec = try_map(input, path_to_yamd).await?;
    info!("processing YAMD complete");

    if config.yamd_processors.convert_cloudinary_embed {
        info!("unwrapping cloudinary");
        pages_vec = try_map(pages_vec, unwrap_cloudinary).await?;
        info!("unwrapping cloudinary complete");
    }

    let mut pages = Pages::new();
    for page in pages_vec {
        pages.add(page.0, page.1);
    }

    Ok(Arc::new(pages))
}

#[cfg(test)]
mod test {
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };

    use chrono::prelude::*;
    use yamd::Yamd;

    use crate::{config::Config, metadata::Metadata, pages::init_pages};

    use super::{Page, Pages};

    #[tokio::test]
    async fn init_from_path_test() {
        let config_path = Path::new("./test/fixtures/");
        let pages = init_pages(
            config_path,
            Arc::new(
                Config::try_from(&<&std::path::Path as Into<PathBuf>>::into(config_path)).unwrap(),
            ),
        )
        .await
        .unwrap();

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

    #[test]
    fn get_similar() {
        let mut pages = Pages::new();
        pages.push(Page::new(
            "1".into(),
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
            Yamd::new(None, vec![]),
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
