pub mod error;
pub mod fs;
pub mod posts;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use clap::Parser;
use error::{ContextExt, Errors};
use serde::{Deserialize, Serialize};
use site::{Page, Site};
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use templating::initialize;
use tera::Context;

use crate::fs::canonicalize;
use crate::posts::init_from_path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the project directory
    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    dist_path: PathBuf,
    content_path: PathBuf,
    template: PathBuf,
    domain: String,
    title: String,
    description: String,
    index_tags: Vec<String>,
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
    let tera = initialize(&template_path, config.clone(), posts.clone(), site.clone())?;
    while let Some(page) = site.next_unrendered_page() {
        let mut context = Context::new();
        context.insert("config", &config);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        let result = tera.render(page.template.as_str(), &context)?;
        site.set_page_content(page.path.as_str(), result);
    }
    site.save()?;
    Ok(())
}
