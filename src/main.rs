pub mod error;
pub mod posts;

use clap::Parser;
use error::Errors;
use serde::{Deserialize, Serialize};
use std::default;
use std::path::PathBuf;
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
}

impl Page {
    fn new(path: String, template: String) -> Self {
        Self {
            path,
            content: None,
            template,
        }
    }
}

struct Site {
    pages: HashMap<String, Page>,
}

impl Site {
    fn new() -> Self {
        Self {
            pages: HashMap::new(),
        }
    }

    fn add_page(&mut self, page: Page) {
        self.pages.insert(page.path.clone(), page);
    }

    fn next_unrendered_page(&self) -> Option<&Page> {
        self.pages
            .iter()
            .filter(|(_, page)| page.content.is_none())
            .map(|(_, page)| page)
            .next()
    }
}

fn get_title(default_title: String) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| match args.get("path") {
        Some(path) => Ok(tera::to_value("foo")?),
        None => Ok(tera::to_value(&default_title)?),
    }
}

fn get_description(default_description: String) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| match args.get("path") {
        Some(path) => Ok(tera::to_value("foo")?),
        None => Ok(tera::to_value(&default_description)?),
    }
}

fn get_host(host: String) -> impl Function + 'static {
    move |_: &HashMap<String, Value>| Ok(tera::to_value(&host)?)
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let path = args.path.clone().join("config.yaml");
    let f = File::open(path)?;
    let config: Config = serde_yaml::from_reader(f)?;
    let template_path = args.path.join(&config.template).canonicalize()?;
    let posts = init_from_path(args.path.join(&config.content_path).canonicalize()?)?;
    let mut site = Site::new();
    site.add_page(Page::new("/".to_string(), "index.html".to_string()));

    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("get_title", get_title(config.title.clone()));
    tera.register_function(
        "get_description",
        get_description(config.description.clone()),
    );
    tera.register_function("get_host", get_host(config.domain.clone()));
    while let Some(page) = site.next_unrendered_page() {
        let mut context = Context::new();
        context.insert("domain", &config.domain);
        let result = tera.render(page.template.as_str(), &context)?;
        println!("{}", result);
    }
    Ok(())
}
