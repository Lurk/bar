use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
};

use serde::Serialize;
use yamd::{deserialize, nodes::yamd::Yamd};

use crate::error::{ContextExt, Errors};

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

pub fn init_from_path(path: PathBuf) -> Result<Posts, Errors> {
    let content_paths = std::fs::read_dir(path).unwrap();
    let mut posts_vec: Vec<(String, Yamd)> = Vec::new();
    for path in content_paths {
        let file = path?.path().canonicalize()?;
        let file_contents =
            fs::read_to_string(&file).with_context(format!("yamd file: {:?}", &file))?;
        let yamd = deserialize(file_contents.as_str()).unwrap();
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
