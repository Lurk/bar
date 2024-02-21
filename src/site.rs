use std::{
    collections::HashMap,
    fs::remove_dir_all,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tokio::task::JoinSet;

use crate::{
    error::{ContextExt, Errors},
    fs::write_file,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicPage {
    pub path: Arc<str>,
    pub template: Arc<str>,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub content: Option<Arc<str>>,
    pub page_num: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticPage {
    pub destination: Arc<str>,
    pub source: Option<PathBuf>,
    pub fallback: Option<Arc<str>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feed {
    pub path: Arc<str>,
    pub content: Option<Arc<str>>,
    pub typ: FeedType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FeedType {
    Json,
    Atom,
}

impl From<&str> for FeedType {
    fn from(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "atom" => Self::Atom,
            _ => panic!("invalid feed type"),
        }
    }
}

impl From<Arc<str>> for FeedType {
    fn from(value: Arc<str>) -> Self {
        match value.as_ref() {
            "json" => Self::Json,
            "atom" => Self::Atom,
            _ => panic!("invalid feed type"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Page {
    Static(StaticPage),
    Dynamic(DynamicPage),
    Feed(Feed),
}

impl From<StaticPage> for Page {
    fn from(value: StaticPage) -> Self {
        Page::Static(value)
    }
}

impl From<DynamicPage> for Page {
    fn from(value: DynamicPage) -> Self {
        Page::Dynamic(value)
    }
}

impl From<Feed> for Page {
    fn from(value: Feed) -> Self {
        Page::Feed(value)
    }
}

impl Page {
    pub fn get_path(&self) -> Arc<str> {
        match self {
            Self::Static(page) => page.destination.clone(),
            Self::Dynamic(page) => page.path.clone(),
            Self::Feed(page) => page.path.clone(),
        }
    }
}

pub struct Site {
    dist_folder: PathBuf,
    pages: Mutex<HashMap<Arc<str>, Arc<Page>>>,
}

impl Site {
    pub fn new(path: PathBuf) -> Self {
        Self {
            dist_folder: path,
            pages: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_page(&self, page: Page) {
        let path = page.get_path();
        let mut pages = self.pages.lock().unwrap();
        if !pages.contains_key(path.as_ref()) {
            pages.insert(path.clone(), Arc::new(page));
        }
    }

    pub fn get_page(&self, path: &str) -> Option<Arc<Page>> {
        let pages = self.pages.lock().unwrap();
        pages.get(path).cloned()
    }

    pub fn next_unrendered_dynamic_page(&self) -> Option<DynamicPage> {
        let pages = self.pages.lock().unwrap();
        let page = pages
            .iter()
            .find(|(_, page)| {
                if let Page::Dynamic(dynamic) = page.as_ref() {
                    dynamic.content.is_none()
                } else {
                    false
                }
            })
            .map(|(_, page)| page.clone());

        if let Some(page) = page {
            if let Page::Dynamic(dynamic) = page.as_ref() {
                Some(dynamic.clone())
            } else {
                unreachable!()
            }
        } else {
            None
        }
    }

    pub fn next_unrendered_feed(&self) -> Option<Feed> {
        let pages = self.pages.lock().unwrap();
        let page = pages
            .iter()
            .find(|(_, page)| {
                if let Page::Feed(feed) = page.as_ref() {
                    feed.content.is_none()
                } else {
                    false
                }
            })
            .map(|(_, page)| page.clone());

        if let Some(page) = page {
            if let Page::Feed(feed) = page.as_ref() {
                Some(feed.clone())
            } else {
                unreachable!()
            }
        } else {
            None
        }
    }

    pub fn set_page_content(&self, path: Arc<str>, content: Arc<str>) {
        let mut pages = self.pages.lock().unwrap();
        pages
            .entry(path.clone())
            .and_modify(|page| match page.as_ref() {
                Page::Static(_) => panic!("cannot set content on static page"),
                Page::Dynamic(dynamic) => {
                    *page = Arc::new(Page::Dynamic(DynamicPage {
                        path: dynamic.path.clone(),
                        content: Some(content),
                        template: dynamic.template.clone(),
                        title: dynamic.title.clone(),
                        description: dynamic.description.clone(),
                        page_num: dynamic.page_num,
                    }));
                }
                Page::Feed(feed) => {
                    *page = Arc::new(Page::Feed(Feed {
                        path: feed.path.clone(),
                        content: Some(content),
                        typ: feed.typ.clone(),
                    }));
                }
            });
    }

    pub async fn save(&self) -> Result<(), Errors> {
        remove_dir_all(&self.dist_folder)
            .with_context(format!("remove directory: {}", self.dist_folder.display()))?;

        let mut set = JoinSet::new();
        {
            let pages = self.pages.lock().unwrap();

            let dist_folder = Arc::new(self.dist_folder.clone());
            for page in pages.values() {
                let page = page.clone();
                let dist_folder = dist_folder.clone();
                set.spawn(async move { save_page(dist_folder, page).await });
            }
        }

        while (set.join_next().await).is_some() {}

        Ok(())
    }
}

async fn save_page(dist_folder: Arc<PathBuf>, page: Arc<Page>) -> Result<(), Errors> {
    match page.as_ref() {
        Page::Static(page) => {
            let destination = dist_folder.join(page.destination.trim_start_matches('/'));
            if let (None, Some(fallback)) = (page.source.as_ref(), page.fallback.as_ref()) {
                println!("write fallback data to file: {}", destination.display());
                write_file(&destination, fallback).await?;
            } else if let Some(source) = &page.source {
                println!(
                    "copy file: {} to {}",
                    source.display(),
                    &destination.display()
                );
                let prefix = destination.parent().unwrap();
                std::fs::create_dir_all(prefix)
                    .with_context(format!("create directory: {}", prefix.display()))?;
                std::fs::copy(source, &destination).with_context(format!(
                    "copy file: {:?} -> {}",
                    source,
                    &destination.display()
                ))?;
            } else {
                panic!("source or fallback is required");
            }
        }
        Page::Dynamic(page) => {
            if let Some(content) = &page.content {
                let page_path = if page.path == "/".into() {
                    "index.html"
                } else {
                    page.path.trim_start_matches('/')
                };

                let path = dist_folder.join(page_path);
                println!("write to file: {}", path.clone().display());
                write_file(&path, content).await?;
            }
        }
        Page::Feed(page) => {
            if let Some(content) = &page.content {
                let path = dist_folder.join(page.path.trim_start_matches('/'));
                println!("write to file: {}", path.clone().display());
                write_file(&path, content).await?;
            }
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_type_from_string() {
        assert_eq!(FeedType::from("json"), FeedType::Json);
        assert_eq!(FeedType::from("atom"), FeedType::Atom);
    }

    #[test]
    fn static_page() {
        let page = StaticPage {
            destination: "/static".into(),
            source: Some("/".into()),
            fallback: None,
        };
        assert_eq!(Page::from(page.clone()), Page::Static(page.clone()));
        assert_eq!(Page::from(page.clone()).get_path(), Arc::from("/static"));
    }
}
