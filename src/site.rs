use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::error::{ContextExt, Errors};

pub struct Page {
    pub path: String,
    pub content: Option<String>,
    pub template: String,
    pub title: String,
    pub description: String,
}

impl Page {
    pub fn new(path: String, template: String, title: String, description: String) -> Self {
        Self {
            path,
            content: None,
            template,
            title,
            description,
        }
    }
}

pub struct Site {
    dist_folder: PathBuf,
    pages: Mutex<HashMap<String, Arc<Page>>>,
}

impl Site {
    pub fn new(path: PathBuf) -> Self {
        Self {
            dist_folder: path,
            pages: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_page(&self, page: Arc<Page>) {
        self.pages.lock().unwrap().insert(page.path.clone(), page);
    }

    pub fn next_unrendered_page(&self) -> Option<Arc<Page>> {
        self.pages
            .lock()
            .unwrap()
            .iter()
            .find(|(_, page)| page.content.is_none())
            .map(|(_, page)| page.clone())
    }

    pub fn get_page(&self, path: &str) -> Option<Arc<Page>> {
        self.pages.lock().unwrap().get(path).cloned()
    }

    pub fn set_page_content(&self, path: &str, content: String) {
        let page = self.get_page(path).unwrap();
        self.add_page(Arc::new(Page {
            path: page.path.clone(),
            content: Some(content),
            template: page.template.clone(),
            title: page.title.clone(),
            description: page.description.clone(),
        }));
    }

    pub fn save(&self) -> Result<(), Errors> {
        for page in self.pages.lock().unwrap().values() {
            if let Some(content) = &page.content {
                let page_path = match page.path.as_str() {
                    "/" => "index.html",
                    path => path,
                };

                let path = self.dist_folder.join(&page_path);
                println!("write to file: {}", path.clone().display());
                let prefix = path.parent().unwrap();
                std::fs::create_dir_all(prefix)
                    .with_context(format!("create directory: {}", prefix.clone().display()))?;
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
        Ok(())
    }
}
