pub mod error;
pub mod posts;

use clap::Parser;
use error::Errors;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, fs::File};
use tera::{Context, Function, Tera, Value};

use crate::posts::init_from_path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the project directory
    #[arg(short, long)]
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    content_path: PathBuf,
    template: PathBuf,
    domain: String,
    title: String,
    description: String,
}

struct Page {
    path: String,
    content: Option<String>,
    template: String,
    title: String,
    description: String,
}

impl Page {
    fn new(path: String, template: String) -> Self {
        let title = "".to_string();
        let description = "".to_string();
        Self {
            path,
            content: None,
            template,
            title,
            description,
        }
    }
}

struct Site {
    pages: Mutex<HashMap<String, Arc<Page>>>,
}

impl Site {
    fn new() -> Self {
        Self {
            pages: Mutex::new(HashMap::new()),
        }
    }

    fn add_page(&self, page: Arc<Page>) {
        self.pages.lock().unwrap().insert(page.path.clone(), page);
    }

    fn next_unrendered_page(&self) -> Option<Arc<Page>> {
        self.pages
            .lock()
            .unwrap()
            .iter()
            .find(|(_, page)| page.content.is_none())
            .map(|(_, page)| page.clone())
    }

    fn get_page(&self, path: &str) -> Option<Arc<Page>> {
        self.pages
            .lock()
            .unwrap()
            .get(path)
            .map(|page| page.clone())
    }

    fn set_page_content(&self, path: &str, content: String) {
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

fn get_string_arg(args: &HashMap<String, Value>, key: &str) -> Option<String> {
    match args.get(key) {
        Some(value) => match value.as_str() {
            Some(string) => Some(string.to_string().clone()),
            None => None,
        },
        None => None,
    }
}

fn get_title(site: Arc<Site>, config: Arc<Config>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        match site.get_page(path.as_str()) {
            Some(page) => Ok(tera::to_value(&page.title)?),
            None => Ok(tera::to_value(&config.title)?),
        }
    }
}

fn get_description(site: Arc<Site>, config: Arc<Config>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        match site.get_page(path.as_str()) {
            Some(page) => Ok(tera::to_value(&page.description)?),
            None => Ok(tera::to_value(&config.description)?),
        }
    }
}

fn get_host(host: String) -> impl Function + 'static {
    move |_: &HashMap<String, Value>| Ok(tera::to_value(&host)?)
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let path = args.path.clone().join("config.yaml");
    let f = File::open(path)?;
    let config: Arc<Config> = Arc::new(serde_yaml::from_reader(f)?);
    let template_path = args.path.join(&config.template).canonicalize()?;
    let posts = init_from_path(args.path.join(&config.content_path).canonicalize()?)?;
    let site: Arc<Site> = Arc::new(Site::new());
    site.add_page(Arc::new(Page::new(
        "/".to_string(),
        "index.html".to_string(),
    )));

    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("get_title", get_title(site.clone(), config.clone()));
    tera.register_function(
        "get_description",
        get_description(site.clone(), config.clone()),
    );
    tera.register_function("get_host", get_host(config.domain.clone()));
    while let Some(page) = site.next_unrendered_page() {
        let mut context = Context::new();
        context.insert("domain", &config.domain);
        let result = tera.render(page.template.as_str(), &context)?;
        site.set_page_content(page.path.as_str(), result);
    }
    Ok(())
}
