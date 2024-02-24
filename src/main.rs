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
use site::{DynamicPage, Site, StaticPage};
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
    let path: &'static PathBuf = Box::leak(Box::new(args.path.clone()));
    let config: Arc<Config> = Arc::new(Config::try_from(args.path.clone())?);
    let template_path = args.path.join(&config.template);
    let dist_path = args.path.join(&config.dist_path);
    if let (Ok(template_path), Ok(dist_path), Ok(pages)) = tokio::join!(
        canonicalize(&template_path),
        canonicalize(&dist_path),
        init_from_path(path, config.clone()),
    ) {
        let site: Arc<Site> = Arc::new(Site::new(dist_path.clone()));
        let tera = initialize(
            path,
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
        let page = if let Some(robots) = config.robots_txt.as_ref() {
            StaticPage {
                destination: Arc::from("robots.txt"),
                source: Some(robots.to_path_buf()),
                fallback: None,
            }
        } else {
            StaticPage {
                destination: Arc::from("robots.txt"),
                source: None,
                fallback: Some("User-agent: *\nAllow: /".into()),
            }
        };
        site.add_page(page.into());

        render(site.clone(), &config, &tera, &pages)?;

        site.save().await?;
    }
    Ok(())
}
