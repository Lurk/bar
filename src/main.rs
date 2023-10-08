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
use std::time::{SystemTime, UNIX_EPOCH};
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

fn get_usize_arg(args: &HashMap<String, Value>, key: &str) -> Option<usize> {
    match args.get(key) {
        Some(value) => value
            .as_number()
            .map(|number| number.as_u64().unwrap() as usize),
        None => None,
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

fn add_static_file(site: Arc<Site>, config: Arc<Config>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        if let (Some(path), Some(file_path)) = (
            get_string_arg(args, "path"),
            get_string_arg(args, "file_path"),
        ) {
            let now = SystemTime::now();
            let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
            site.add_static_file(path.clone(), config.template.join(file_path));
            return Ok(tera::to_value(format!(
                "{}?cb={}",
                &path,
                since_the_epoch.as_millis()
            ))?);
        }
        Err(tera::Error::msg("path and file_path are required"))
    }
}

fn get_posts_by_tag(posts: Arc<posts::Posts>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let tag = get_string_arg(args, "tag").unwrap_or("".to_string());
        let amount = get_usize_arg(args, "amount").unwrap_or(3);
        let posts = posts.get_posts_by_tag(tag.as_str(), amount);
        Ok(tera::to_value(&posts)?)
    }
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let config_path = args.path.clone().join("config.yaml");
    let f = File::open(&config_path).with_context(format!("config file: {:?}", &config_path))?;
    let config: Arc<Config> = Arc::new(serde_yaml::from_reader(f)?);
    let template_path = canonicalize(&args.path.join(&config.template))?;
    let posts = Arc::new(init_from_path(canonicalize(
        &args.path.join(&config.content_path),
    )?)?);
    let dist_path = canonicalize(&args.path.join(&config.dist_path))?;
    let site: Arc<Site> = Arc::new(Site::new(dist_path));
    site.add_page(Arc::new(Page::new(
        "/".to_string(),
        "index.html".to_string(),
        config.title.clone(),
        config.description.clone(),
    )));
    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function(
        "add_static_file",
        add_static_file(site.clone(), config.clone()),
    );
    tera.register_function("get_posts_by_tag", get_posts_by_tag(posts.clone()));
    while let Some(page) = site.next_unrendered_page() {
        let mut context = Context::new();
        context.insert("domain", &config.domain);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        let result = tera.render(page.template.as_str(), &context)?;
        site.set_page_content(page.path.as_str(), result);
    }
    site.save()?;
    Ok(())
}
