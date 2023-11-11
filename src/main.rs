pub mod config;
pub mod error;
pub mod fs;
pub mod json_feed;
pub mod pages;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use clap::Parser;
use config::Config;
use error::Errors;
use json_feed::{JsonFeedBuilder, JsonFeedItem};
use site::{DynamicPage, FeedType, Site};
use std::path::PathBuf;
use std::sync::Arc;
use templating::initialize;
use tera::Context;

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
    let template_path = canonicalize(&args.path.join(&config.template))?;
    let pages = Arc::new(init_from_path(&args.path, config.clone()).await?);
    let dist_path = canonicalize(&args.path.join(&config.dist_path))?;
    let site: Arc<Site> = Arc::new(Site::new(dist_path));
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
    let tera = initialize(
        Arc::from(args.path),
        &template_path,
        config.clone(),
        pages.clone(),
        site.clone(),
    )?;

    let mut feed_items: Vec<JsonFeedItem> = vec![];

    while let Some(page) = site.next_unrendered_dynamic_page() {
        println!("Rendering page: {}", page.path);
        let mut context = Context::new();
        context.insert("config", &config);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        context.insert("page_num", &page.page_num);
        let result = tera.render(&page.template, &context)?;
        site.set_page_content(page.path.clone(), result.into());
        if let Some(page) = pages.get(&page.path.trim_end_matches(".html")) {
            feed_items.push(JsonFeedItem::new(page, config.domain.as_ref()));
        }
    }

    while let Some(page) = site.next_unrendered_feed() {
        match page.typ {
            FeedType::Json => {
                let feed_url = config.domain.join(&page.path)?;
                let icon = if site.get_page("/icon.png").is_some() {
                    Some(config.domain.join("/icon.png")?)
                } else {
                    None
                };

                let favicon = if site.get_page("/favicon.ico").is_some() {
                    Some(config.domain.join("/favicon.ico")?)
                } else {
                    None
                };
                let mut feed = JsonFeedBuilder {
                    title: config.title.clone(),
                    home_page_url: config.domain.clone(),
                    feed_url,
                    icon,
                    favicon,
                    language: Arc::from("en"),
                }
                .build();

                feed.add_items(feed_items.clone());
                site.set_page_content(page.path.clone(), feed.to_string().into());
            }
            FeedType::Atom => todo!(),
        }
    }
    site.save().await?;
    Ok(())
}
