use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::fs::{copy, create_dir_all, remove_dir_all};
use tracing::{debug, info};

use crate::{
    r#async::try_for_each,
    context::BuildConfig,
    diagnostic::{BarDiagnostic, ContextExt},
    fs::{canonicalize_with_context, get_files_by_ext_deep, write_file},
};

use tracing::warn;

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

impl TryFrom<&str> for FeedType {
    type Error = BarDiagnostic;
    fn try_from(value: &str) -> Result<Self, BarDiagnostic> {
        match value {
            "json" => Ok(Self::Json),
            "atom" => Ok(Self::Atom),
            _ => Err(format!("invalid feed type: {value}").into()),
        }
    }
}

impl TryFrom<Arc<str>> for FeedType {
    type Error = BarDiagnostic;
    fn try_from(value: Arc<str>) -> Result<Self, BarDiagnostic> {
        FeedType::try_from(value.as_ref())
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
    #[must_use]
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
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            dist_folder: path,
            pages: Mutex::new(HashMap::new()),
        }
    }

    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub fn add_page(&self, page: Page) {
        let path = page.get_path();
        let mut pages = self.pages.lock().expect("Site pages mutex poisoned");
        if !pages.contains_key(path.as_ref()) {
            pages.insert(path.clone(), Arc::new(page));
        }
    }

    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub fn get_page(&self, path: &str) -> Option<Arc<Page>> {
        let pages = self.pages.lock().expect("Site pages mutex poisoned");
        pages.get(path).cloned()
    }

    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub fn next_unrendered_dynamic_page(&self) -> Option<DynamicPage> {
        let pages = self.pages.lock().expect("Site pages mutex poisoned");
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

    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub fn next_unrendered_feed(&self) -> Option<Feed> {
        let pages = self.pages.lock().expect("Site pages mutex poisoned");
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

    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub fn set_page_content(&self, path: &Arc<str>, content: Arc<str>) {
        let mut pages = self.pages.lock().expect("Site pages mutex poisoned");
        pages
            .entry(path.clone())
            .and_modify(|page| match page.as_ref() {
                Page::Static(_) => {
                    warn!("attempted to set content on static page: {path}");
                }
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

    /// # Errors
    /// Returns error if files cannot be written to the dist folder.
    ///
    /// # Panics
    /// Panics if the pages mutex is poisoned.
    pub async fn save(&self) -> Result<(), BarDiagnostic> {
        info!("clean up dist folder");
        create_dir_all(&self.dist_folder)
            .await
            .with_context(|| format!("create directory: {}", self.dist_folder.display()))?;

        remove_dir_all(&self.dist_folder)
            .await
            .with_context(|| format!("remove directory: {}", self.dist_folder.display()))?;
        info!("cleanup complete");

        info!("writing data");
        let dist_folder = Arc::new(self.dist_folder.clone());
        let input: Vec<(Arc<PathBuf>, Arc<Page>)> = self
            .pages
            .lock()
            .expect("Site pages mutex poisoned")
            .values()
            .map(|page| (dist_folder.clone(), page.clone()))
            .collect();

        try_for_each(50, input, save_page).await?;
        info!("writing data complete");
        Ok(())
    }
}

async fn save_page((dist_folder, page): (Arc<PathBuf>, Arc<Page>)) -> Result<(), BarDiagnostic> {
    match page.as_ref() {
        Page::Static(page) => {
            let destination = dist_folder.join(page.destination.trim_start_matches('/'));
            if let (None, Some(fallback)) = (page.source.as_ref(), page.fallback.as_ref()) {
                debug!("write fallback data to file: {}", destination.display());
                write_file(&destination, fallback.as_bytes()).await?;
            } else if let Some(source) = &page.source {
                debug!(
                    "copy file: {} to {}",
                    source.display(),
                    &destination.display()
                );
                let prefix = destination.parent().unwrap();
                create_dir_all(prefix)
                    .await
                    .with_context(|| format!("create directory: {}", prefix.display()))?;
                copy(source, &destination).await.with_context(|| {
                    format!(
                        "copy file: {} -> {}",
                        source.display(),
                        destination.display()
                    )
                })?;
            } else {
                return Err("static page must have either source or fallback".into());
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
                debug!("write to file: {}", path.clone().display());
                write_file(&path, content.as_bytes()).await?;
            }
        }
        Page::Feed(page) => {
            if let Some(content) = &page.content {
                let path = dist_folder.join(page.path.trim_start_matches('/'));
                debug!("write to file: {}", path.clone().display());
                write_file(&path, content.as_bytes()).await?;
            }
        }
    }
    Ok(())
}

fn create_destination_path(source: &Path, prefix: &PathBuf) -> Result<String, BarDiagnostic> {
    let stripped = source.strip_prefix(prefix).with_context(|| {
        format!(
            "strip prefix {} from {}",
            prefix.display(),
            source.display()
        )
    })?;
    let s = stripped.to_str().ok_or_else(|| {
        BarDiagnostic::from(format!("path is not valid UTF-8: {}", stripped.display()))
    })?;
    Ok(s.replace('\\', "/"))
}

/// Initialize the site with static files and entry point.
/// The function will scan the source and template directories for files with the given extensions.
///
/// Static files priories:
/// 1. Source files
/// 2. Template files
/// 3. BAR defaults
///
/// # Errors
/// Returns error if static files cannot be discovered or paths are invalid.
pub async fn init_site(build_config: &BuildConfig) -> Result<Arc<Site>, BarDiagnostic> {
    info!("init static files");
    let base_path = &build_config.path;
    let config = &build_config.config;
    let site = Arc::new(Site::new(base_path.join(&config.dist_path)));
    let source_path = base_path.join(&config.static_source_path);
    let template_static_path = base_path.join(&config.template).join("static/");
    let (source_path, template_static_path) = tokio::try_join!(
        canonicalize_with_context(&source_path),
        canonicalize_with_context(&template_static_path)
    )?;

    let extensions = config
        .static_files_extensions
        .iter()
        .map(std::string::String::as_str)
        .collect::<Vec<&str>>();

    for file in get_files_by_ext_deep(&source_path, &extensions).await? {
        let destination = create_destination_path(&file, &source_path)?;
        site.add_page(
            StaticPage {
                destination: Arc::from(destination),
                source: Some(file),
                fallback: None,
            }
            .into(),
        );
    }

    for file in get_files_by_ext_deep(&template_static_path, &extensions).await? {
        let destination = create_destination_path(&file, &template_static_path)?;
        site.add_page(
            StaticPage {
                destination: Arc::from(destination),
                source: Some(file),
                fallback: None,
            }
            .into(),
        );
    }

    site.add_page(
        StaticPage {
            destination: Arc::from("robots.txt"),
            source: None,
            fallback: Some("User-agent: *\nAllow: /".into()),
        }
        .into(),
    );

    site.add_page(
        DynamicPage {
            path: "/".into(),
            template: "index.html".into(),
            title: config.title.clone(),
            description: config.description.clone(),
            content: None,
            page_num: 0,
        }
        .into(),
    );

    if config.template.join("404.html").exists() {
        site.add_page(
            DynamicPage {
                path: "/404.html".into(),
                template: "404.html".into(),
                title: config.title.clone(),
                description: config.description.clone(),
                content: None,
                page_num: 0,
            }
            .into(),
        );
    }

    info!("static files initialization complete");
    Ok(site)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_type_from_string() {
        assert_eq!(FeedType::try_from("json").unwrap(), FeedType::Json);
        assert_eq!(FeedType::try_from("atom").unwrap(), FeedType::Atom);
        assert!(FeedType::try_from("invalid").is_err());
    }

    #[test]
    fn feed_type_from_arc_str() {
        assert_eq!(
            FeedType::try_from(Arc::from("json")).unwrap(),
            FeedType::Json
        );
        assert_eq!(
            FeedType::try_from(Arc::from("atom")).unwrap(),
            FeedType::Atom
        );
        assert!(FeedType::try_from(Arc::from("invalid")).is_err());
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

    #[test]
    fn dynamic_page() {
        let page = DynamicPage {
            path: "/".into(),
            template: "index.html".into(),
            title: "title".into(),
            description: "description".into(),
            content: None,
            page_num: 0,
        };
        assert_eq!(Page::from(page.clone()), Page::Dynamic(page.clone()));
        assert_eq!(Page::from(page.clone()).get_path(), Arc::from("/"));
    }
}
