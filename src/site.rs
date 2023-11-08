use std::{
    collections::HashMap,
    fs::{remove_dir_all, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::error::{ContextExt, Errors};

#[derive(Debug)]
pub struct DynamicPage {
    pub path: Arc<str>,
    pub template: Arc<str>,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub content: Option<Arc<str>>,
    pub page_num: usize,
}

#[derive(Debug)]
pub struct StaticPage {
    pub path: Arc<str>,
    pub file: PathBuf,
}

#[derive(Debug)]
pub enum Page {
    Static(StaticPage),
    Dynamic(DynamicPage),
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

impl Page {
    pub fn get_path(&self) -> Arc<str> {
        match self {
            Self::Static(page) => page.path.clone(),
            Self::Dynamic(page) => page.path.clone(),
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
        if !self.pages.lock().unwrap().contains_key(path.as_ref()) {
            self.pages
                .lock()
                .unwrap()
                .insert(path.clone(), Arc::new(page));
        }
    }

    pub fn next_unrendered_page(&self) -> Option<Arc<Page>> {
        self.pages
            .lock()
            .unwrap()
            .iter()
            .find(|(_, page)| {
                if let Page::Dynamic(dynamic) = page.as_ref() {
                    dynamic.content.is_none()
                } else {
                    false
                }
            })
            .map(|(_, page)| page.clone())
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
            });
    }

    pub fn save(&self) -> Result<(), Errors> {
        remove_dir_all(&self.dist_folder)
            .with_context(format!("remove directory: {}", self.dist_folder.display()))?;
        for page in self.pages.lock().unwrap().values() {
            match page.as_ref() {
                Page::Static(page) => {
                    let static_file_path = self.dist_folder.join(page.path.trim_start_matches('/'));
                    println!(
                        "copy file: {} to {}",
                        &page.file.display(),
                        &static_file_path.display()
                    );
                    let prefix = static_file_path.parent().unwrap();
                    std::fs::create_dir_all(prefix)
                        .with_context(format!("create directory: {}", prefix.display()))?;
                    std::fs::copy(page.file.clone(), &static_file_path).with_context(format!(
                        "copy file: {:?} -> {}",
                        page.file,
                        &static_file_path.display()
                    ))?;
                }
                Page::Dynamic(page) => {
                    if let Some(content) = &page.content {
                        let page_path = if page.path == "/".into() {
                            "index.html"
                        } else {
                            page.path.trim_start_matches('/')
                        };

                        let path = self.dist_folder.join(page_path);
                        println!("write to file: {}", path.clone().display());
                        let prefix = path.parent().unwrap();
                        std::fs::create_dir_all(prefix)
                            .with_context(format!("create directory: {}", prefix.display()))?;
                        let mut file = OpenOptions::new()
                            .write(true)
                            .create(true)
                            .append(false)
                            .open(&path)
                            .with_context(format!("open file: {}", &path.display()))?;
                        file.write_all(content.as_bytes())
                            .with_context(format!("write to file: {}", &path.display()))?;
                    }
                }
            };
        }

        Ok(())
    }
}
