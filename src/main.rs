pub mod config;
pub mod error;
pub mod fs;
pub mod posts;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use clap::Parser;
use config::Config;
use error::Errors;
use site::{Page, Site};
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

#[tokio::main]
async fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let config: Arc<Config> = Arc::new(Config::try_from(args.path.clone())?);
    let template_path = canonicalize(&args.path.join(&config.template))?;
    let posts = Arc::new(init_from_path(&args.path, config.clone()).await?);
    let dist_path = canonicalize(&args.path.join(&config.dist_path))?;
    let site: Arc<Site> = Arc::new(Site::new(dist_path));
    site.add_page(Arc::new(Page::new(
        "/".to_string(),
        "index.html".to_string(),
        config.title.clone(),
        config.description.clone(),
        0,
    )));
    let tera = initialize(
        Arc::from(args.path),
        &template_path,
        config.clone(),
        posts.clone(),
        site.clone(),
    )?;
    while let Some(page) = site.next_unrendered_page() {
        println!("Rendering page: {}", page.path);
        let mut context = Context::new();
        context.insert("config", &config);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        context.insert("page_num", &page.page_num);
        let result = tera.render(page.template.as_str(), &context)?;
        site.set_page_content(page.path.as_str(), result);
    }
    site.save()?;
    Ok(())
}
