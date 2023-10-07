pub mod error;
pub mod fs;
pub mod posts;
pub mod site;

use clap::Parser;
use error::{ContextExt, Errors};
use serde::{Deserialize, Serialize};
use site::{Page, Site};
use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, fs::File};
use tera::{Context, Function, Tera, Value};

use crate::fs::canonicalize;
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
    dist_path: PathBuf,
    content_path: PathBuf,
    template: PathBuf,
    domain: String,
    title: String,
    description: String,
}

fn get_string_arg(args: &HashMap<String, Value>, key: &str) -> Option<String> {
    match args.get(key) {
        Some(value) => value.as_str().map(|string| string.to_string().clone()),
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

fn add_page(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        let template = get_string_arg(args, "template").unwrap_or("index.html".to_string());
        let title = get_string_arg(args, "title").unwrap_or("".to_string());
        let description = get_string_arg(args, "description").unwrap_or("".to_string());
        site.add_page(Arc::new(Page::new(path, template, title, description)));
        Ok(tera::to_value(())?)
    }
}

fn get_host(host: String) -> impl Function + 'static {
    move |_: &HashMap<String, Value>| Ok(tera::to_value(&host)?)
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let config_path = args.path.clone().join("config.yaml");
    let f = File::open(&config_path).with_context(format!("config file: {:?}", &config_path))?;
    let config: Arc<Config> = Arc::new(serde_yaml::from_reader(f)?);
    let template_path = canonicalize(&args.path.join(&config.template))?;
    let posts = init_from_path(canonicalize(&args.path.join(&config.content_path))?)?;
    let dist_path = canonicalize(&args.path.join(&config.dist_path))?;
    let site: Arc<Site> = Arc::new(Site::new(dist_path));
    site.add_page(Arc::new(Page::new(
        "/".to_string(),
        "index.html".to_string(),
        config.title.clone(),
        config.description.clone(),
    )));
    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("get_title", get_title(site.clone(), config.clone()));
    tera.register_function(
        "get_description",
        get_description(site.clone(), config.clone()),
    );
    tera.register_function("get_host", get_host(config.domain.clone()));
    tera.register_function("add_page", add_page(site.clone()));
    while let Some(page) = site.next_unrendered_page() {
        let mut context = Context::new();
        context.insert("domain", &config.domain);
        let result = tera.render(page.template.as_str(), &context)?;
        site.set_page_content(page.path.as_str(), result);
    }
    site.save()?;
    Ok(())
}
