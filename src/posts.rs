use std::{collections::HashMap, fs, path::PathBuf};

use yamd::{deserialize, nodes::yamd::Yamd};

use crate::error::Errors;

pub struct Posts {
    posts: HashMap<String, Yamd>,
    order: Vec<String>,
}

impl Posts {
    pub fn new() -> Self {
        Self {
            posts: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn add(&mut self, key: String, value: Yamd) {
        self.posts.insert(key.clone(), value);
        self.order.push(key);
    }

    pub fn keys(&self) -> &Vec<String> {
        &self.order
    }

    pub fn get(&self, key: &str) -> Option<&Yamd> {
        self.posts.get(key)
    }

    pub fn get_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = Vec::new();
        for post in self.posts.values() {
            for tag in post.metadata.tags.iter() {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
        }
        tags
    }

    pub fn get_posts_by_tag(&self, tag: &str) -> Vec<&Yamd> {
        let mut posts: Vec<&Yamd> = Vec::new();
        for pid in self.keys() {
            let post = self.get(pid).unwrap();
            if post.metadata.tags.contains(&tag.to_string()) {
                posts.push(post);
            }
        }
        posts
    }
}

pub fn init_from_path(path: PathBuf) -> Result<Posts, Errors> {
    let content_paths = std::fs::read_dir(path).unwrap();
    let mut posts_vec: Vec<(String, Yamd)> = Vec::new();
    for path in content_paths {
        let file = path?.path().canonicalize()?;
        let file_contents = fs::read_to_string(&file)?;
        let yamd = deserialize(file_contents.as_str()).unwrap();
        posts_vec.push((file.file_stem().unwrap().to_str().unwrap().into(), yamd));
    }
    let mut posts = Posts::new();

    posts_vec.sort_by(|a, b| {
        b.1.metadata
            .timestamp
            .as_ref()
            .unwrap()
            .cmp(&a.1.metadata.timestamp.as_ref().unwrap())
    });
    for post in posts_vec {
        posts.add(post.0, post.1);
    }
    Ok(posts)
}
