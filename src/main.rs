pub mod config;
pub mod error;
pub mod fs;
pub mod json_feed;
pub mod pages;
pub mod renderer;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use clap::Parser;
use config::Config;
use error::Errors;
use renderer::render;
use site::{DynamicPage, Site};
use std::path::PathBuf;
use std::sync::Arc;
use templating::initialize;

use crate::fs::canonicalize;
use crate::pages::init_from_path;
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
    let template_path = args.path.join(&config.template);
    let dist_path = args.path.join(&config.dist_path);
    if let (Ok(template_path), Ok(dist_path), Ok(pages)) = tokio::join!(
        canonicalize(&template_path),
        canonicalize(&dist_path),
        init_from_path(&args.path, config.clone()),
    ) {
        let site: Arc<Site> = Arc::new(Site::new(dist_path));
        let tera = initialize(
            Arc::from(args.path),
            &template_path,
            config.clone(),
            pages.clone(),
            site.clone(),
        )?;

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

        render(site.clone(), &config, &tera, &pages)?;

        site.save().await?;
    }
    Ok(())
}
