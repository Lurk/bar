use std::{
    collections::{HashMap, HashSet},
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

impl Post {
    pub fn new(pid: Arc<str>, content: Yamd) -> Self {
        Self { pid, content }
    }
}

pub struct Posts {
    posts: HashMap<Arc<str>, Post>,
    order: Vec<Arc<str>>,
}

impl Posts {
    pub fn new() -> Self {
        Self {
            posts: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn add(&mut self, key: String, value: Yamd) {
        let pid: Arc<str> = Arc::from(key.as_str());
        self.posts
            .insert(pid.clone(), Post::new(pid.clone(), value));
        self.order.push(pid.clone());
    }

    pub fn keys(&self) -> &Vec<Arc<str>> {
        &self.order
    }

    pub fn get(&self, key: &str) -> Option<&Post> {
        self.posts.get(key)
    }

    pub fn get_tags(&self) -> HashSet<String> {
        let mut tags: HashSet<String> = HashSet::new();
        for post in self.posts.values() {
            for tag in post.content.metadata.tags.iter() {
                tags.insert(tag.clone());
            }
        }
        tags
    }

    pub fn get_posts_by_tag(&self, tag: &str, amount: usize) -> Vec<&Post> {
        let mut posts: Vec<&Post> = Vec::with_capacity(amount);
        for pid in self.keys() {
            let post = self.get(pid).unwrap();
            if post.content.metadata.tags.contains(&tag.to_string()) {
                posts.push(post);
            }
            if posts.len() == amount {
                break;
            }
        }
        posts
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
                YamdNodes::CloudinaryImageGallery(cloudinary) => {
                    let cloud_name: Arc<str> = cloudinary.cloud_name.clone().into();
                    let tags = get_tags(cloud_name.clone(), cloudinary.tag.clone().into())
                        .await
                        .unwrap();
                    let images = tags
                        .resources
                        .iter()
                        .map(|resource| {
                            let mut image =
                                Image::new(cloud_name.clone(), resource.public_id.clone());
                            image.set_format(resource.format.as_ref());
                            ImageGalleryNodes::Image(image::Image::new(
                                false,
                                resource.public_id.to_string(),
                                image.to_string(),
                            ))
                        })
                        .collect::<Vec<ImageGalleryNodes>>();
                    nodes.push(
                        ImageGallery::new_with_nodes(cloudinary.consumed_all_input, images).into(),
                    );
                }
                _ => nodes.push(node.clone()),
            }
        }
        return Ok(Yamd::new_with_nodes(Some(yamd.metadata), nodes));
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
            .timestamp
            .as_ref()
            .unwrap()
            .cmp(a.1.metadata.timestamp.as_ref().unwrap())
    });
    for post in posts_vec {
        posts.add(post.0, post.1);
    }
    Ok(posts)
}
