use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use cloudinary::{tags::get_tags, transformation::Image};
use url::Url;

use crate::{
    config::Config,
    error::Errors,
    fs::{canonicalize, get_files_by_ext_deep},
};
use serde::Serialize;
use tokio::{fs::read_to_string, task::JoinSet};
use yamd::{
    deserialize,
    nodes::{
        image,
        image_gallery::{ImageGallery, ImageGalleryNodes},
        yamd::{Yamd, YamdNodes},
    },
};

#[derive(Debug, Serialize)]
pub struct Page {
    pub pid: Arc<str>,
    pub content: Yamd,
}

#[derive(Debug, Serialize)]
pub struct SliceNumber {
    number: usize,
    is_current: bool,
    display: usize,
}

#[derive(Debug, Serialize)]
pub struct PagesSlice {
    pages: Vec<Arc<Page>>,
    current_slice: usize,
    total_slices: usize,
    numbers: Vec<SliceNumber>,
    slice_size: usize,
}

impl Page {
    pub fn new(pid: Arc<str>, content: Yamd) -> Self {
        Self { pid, content }
    }

    pub fn get_title(&self) -> String {
        self.content
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| "Untitled".into())
    }

    pub fn get_image(&self, base_url: &Url) -> Option<Url> {
        if let Some(image) = self.content.metadata.image.clone() {
            if image.starts_with("http") {
                let image = Url::parse(image.as_str()).unwrap();
                return Some(image);
            }

            let mut url = base_url.clone();
            url.set_path(image.as_str());
            return Some(url);
        }
        None
    }
}

pub struct Pages {
    pages: HashMap<Arc<str>, Arc<Page>>,
    order: Vec<Arc<str>>,
    tags: HashMap<Arc<str>, Vec<Arc<Page>>>,
}

impl Pages {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            order: Vec::new(),
            tags: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: String, value: Yamd) {
        let pid: Arc<str> = Arc::from(key.as_str());
        let post = Arc::new(Page::new(pid.clone(), value.clone()));
        self.pages.insert(pid.clone(), post.clone());
        self.order.push(pid.clone());
        if let Some(tags) = value.metadata.tags {
            tags.iter().for_each(|tag| {
                let tag: Arc<str> = Arc::from(tag.as_str());
                if let Entry::Vacant(e) = self.tags.entry(tag.clone()) {
                    e.insert(vec![post.clone()]);
                } else {
                    self.tags.get_mut(&tag).unwrap().push(post.clone());
                }
            });
        }
    }

    pub fn keys(&self) -> &Vec<Arc<str>> {
        &self.order
    }

    pub fn get(&self, pid: &str) -> Option<&Page> {
        self.pages.get(pid).map(|page| page.as_ref())
    }

    pub fn get_tags(&self) -> HashSet<String> {
        let mut tags: HashSet<String> = HashSet::new();

        self.tags.keys().for_each(|tag| {
            tags.insert(tag.to_string());
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
            pages: Vec::with_capacity(limit),
            current_slice,
            total_slices,
            slice_size: limit,
            numbers,
        };

        for page in pages.iter().skip(offset).take(limit) {
            slice.pages.push(page.clone());
        }
        slice
    }
}

impl Default for Pages {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn path_to_yamd(path: PathBuf, should_unwrap_cloudinary: &bool) -> Result<Yamd, Errors> {
    let file_contents = read_to_string(&path)
        .await
        .unwrap_or_else(|_| panic!("yamd file: {:?}", &path));
    let yamd = deserialize(file_contents.as_str()).unwrap();
    if *should_unwrap_cloudinary {
        let mut nodes: Vec<YamdNodes> = Vec::with_capacity(yamd.nodes.len());
        for node in yamd.nodes.iter() {
            match node {
                // TODO embed also can be part of Accordion Tab
                YamdNodes::Embed(embed) if embed.kind == "cloudinary_gallery" => {
                    let (cloud_name, tag) = embed.args.split_once('&').unwrap_or_else(
                        || panic!("cloudinary_gallery embed must have two arguments: cloud_name and tag.\n{:?}", path)
                    );
                    let tags = get_tags(cloud_name.into(), tag.into())
                        .await
                        .unwrap_or_else(|_| panic!("error loading cloudinary tag: {}", tag));
                    let images = tags
                        .resources
                        .iter()
                        .map(|resource| {
                            let mut image =
                                Image::new(cloud_name.into(), resource.public_id.clone());
                            image.set_format(resource.format.as_ref());
                            ImageGalleryNodes::Image(image::Image::new(
                                resource.public_id.to_string(),
                                image.to_string(),
                            ))
                        })
                        .collect::<Vec<ImageGalleryNodes>>();
                    nodes.push(ImageGallery::new(images).into());
                }
                _ => nodes.push(node.clone()),
            }
        }
        return Ok(Yamd::new(Some(yamd.metadata), nodes));
    }
    Ok(yamd)
}

pub async fn init_from_path(path: &Path, config: Arc<Config>) -> Result<Arc<Pages>, Errors> {
    let content_path = canonicalize(&path.join(&config.content_path)).await?;
    let mut pages_vec: Vec<(String, Yamd)> = Vec::new();
    let should_unwrap_cloudinary = config
        .get("should_unpack_cloudinary".into())
        .map(|v| v.as_bool().unwrap_or(&false))
        .unwrap_or(&false);

    let mut set = JoinSet::new();

    for path in get_files_by_ext_deep(&content_path, "yamd").await? {
        let file = path.canonicalize()?;
        let content_path = content_path.clone();
        let should_unwrap_cloudinary = *should_unwrap_cloudinary;
        set.spawn(async move {
            let yamd = path_to_yamd(file.clone(), &should_unwrap_cloudinary).await;
            let pid = file
                .with_extension("")
                .to_str()
                .unwrap()
                .trim_start_matches(content_path.to_str().unwrap())
                .to_string();

            (pid, yamd)
        });
    }

    while let Some(res) = set.join_next().await {
        if let (pid, Ok(yamd)) = res.unwrap() {
            pages_vec.push((pid, yamd));
        }
    }

    let mut pages = Pages::new();

    pages_vec.sort_by(|a, b| {
        b.1.metadata
            .date
            .as_ref()
            .unwrap()
            .cmp(a.1.metadata.date.as_ref().unwrap())
    });
    for page in pages_vec {
        pages.add(page.0, page.1);
    }
    Ok(Arc::new(pages))
}
