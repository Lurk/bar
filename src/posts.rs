use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use cloudinary::{tags::get_tags, transformation::Image};

use crate::error::Errors;
use serde::Serialize;
use tokio::fs::read_to_string;
use yamd::{
    deserialize,
    nodes::{
        image,
        image_gallery::{ImageGallery, ImageGalleryNodes},
        yamd::{Yamd, YamdNodes},
    },
};

#[derive(Debug, Serialize)]
pub struct Post {
    pid: Arc<str>,
    content: Yamd,
}

#[derive(Debug, Serialize)]
pub struct PageNumber {
    number: usize,
    is_current: bool,
    display: usize,
}

#[derive(Debug, Serialize)]
pub struct PostPage {
    posts: Vec<Arc<Post>>,
    current_page: usize,
    total_pages: usize,
    numbers: Vec<PageNumber>,
    page_size: usize,
}

impl Post {
    pub fn new(pid: Arc<str>, content: Yamd) -> Self {
        Self { pid, content }
    }
}

pub struct Posts {
    posts: HashMap<Arc<str>, Arc<Post>>,
    order: Vec<Arc<str>>,
    tags: HashMap<Arc<str>, Vec<Arc<Post>>>,
}

impl Posts {
    pub fn new() -> Self {
        Self {
            posts: HashMap::new(),
            order: Vec::new(),
            tags: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: String, value: Yamd) {
        let pid: Arc<str> = Arc::from(key.as_str());
        let post = Arc::new(Post::new(pid.clone(), value.clone()));
        self.posts.insert(pid.clone(), post.clone());
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

    pub fn get(&self, key: &str) -> Option<&Post> {
        self.posts.get(key).map(|post| post.as_ref())
    }

    pub fn get_tags(&self) -> HashSet<String> {
        let mut tags: HashSet<String> = HashSet::new();

        self.tags.keys().for_each(|tag| {
            tags.insert(tag.to_string());
        });
        tags
    }

    pub fn get_posts_by_tag(&self, tag: &str, limit: usize, offset: usize) -> PostPage {
        let posts = self
            .tags
            .get(tag)
            .unwrap_or_else(|| panic!("{tag} must be present"));

        let current_page = offset / limit;
        let total_pages: usize = (posts.len() as f64 / limit as f64).ceil() as usize;

        let mut numbers: Vec<PageNumber> = Vec::with_capacity(total_pages);

        for i in 0..total_pages {
            numbers.push(PageNumber {
                number: i,
                display: i + 1,
                is_current: i == current_page,
            });
        }
        let mut page: PostPage = PostPage {
            posts: Vec::with_capacity(limit),
            current_page,
            total_pages,
            page_size: limit,
            numbers,
        };

        for post in posts.iter().skip(offset).take(limit) {
            page.posts.push(post.clone());
        }
        page
    }
}

impl Default for Posts {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn path_to_yamd(path: PathBuf, should_unwrap_cloudinary: bool) -> Result<Yamd, Errors> {
    let file_contents = read_to_string(&path)
        .await
        .unwrap_or_else(|_| panic!("yamd file: {:?}", &path));
    let yamd = deserialize(file_contents.as_str()).unwrap();
    if should_unwrap_cloudinary {
        let mut nodes: Vec<YamdNodes> = Vec::with_capacity(yamd.nodes.len());
        for node in yamd.nodes.iter() {
            match node {
                YamdNodes::Embed(embed) if embed.kind == "cloudinary_gallery" => {
                    let (cloud_name, tag) = embed.args.split_once('&').unwrap_or_else(
                        || panic!("cloudinary_gallery embed must have two arguments: cloud_name and tag.\n{:?}", path)
                    );
                    let tags = get_tags(cloud_name.into(), tag.into()).await.unwrap();
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

pub async fn init_from_path(path: PathBuf) -> Result<Posts, Errors> {
    let content_paths = std::fs::read_dir(path).unwrap();
    let mut posts_vec: Vec<(String, Yamd)> = Vec::new();
    for path in content_paths {
        let file = path?.path().canonicalize()?;
        // TODO: make this concurrent
        // TODO: make this configurable
        let yamd = path_to_yamd(file.clone(), true).await?;
        posts_vec.push((file.file_stem().unwrap().to_str().unwrap().into(), yamd));
    }
    let mut posts = Posts::new();

    posts_vec.sort_by(|a, b| {
        b.1.metadata
            .date
            .as_ref()
            .unwrap()
            .cmp(a.1.metadata.date.as_ref().unwrap())
    });
    for post in posts_vec {
        posts.add(post.0, post.1);
    }
    Ok(posts)
}
