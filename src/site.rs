use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
    pages: Mutex<HashMap<String, Arc<Page>>>,
}

impl Site {
    pub fn new() -> Self {
        Self {
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
        self.pages
            .lock()
            .unwrap()
            .get(path)
            .map(|page| page.clone())
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
}
