mod args;
pub mod r#async;
mod cache;
mod cloudinary;
pub mod config;
pub mod error;
pub mod fs;
pub mod json_feed;
mod metadata;
pub mod pages;
pub mod renderer;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use args::{Args, ArticleArgs, BuildArgs, Commands};
use clap::Parser;
use config::Config;
use error::BarErr;
use fs::write_file;
use metadata::Metadata;
use renderer::render;
use site::init_site;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use templating::initialize;
use tokio::fs::{remove_file, try_exists};
use tokio::try_join;
use tracing::subscriber;
use tracing_log::AsTrace;
use tracing_subscriber::FmtSubscriber;
use yamd::nodes::Paragraph;
use yamd::Yamd;

use crate::fs::canonicalize_with_context;
use crate::pages::init_pages;

static CONFIG: OnceLock<Config> = OnceLock::new();
static PATH: OnceLock<PathBuf> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<(), BarErr> {
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    match args.command {
        Some(Commands::Build(build_args)) => {
            build(build_args).await?;
        }
        Some(Commands::Article(article_args)) => {
            create_article(article_args).await?;
        }
        None => {
            build(BuildArgs {
                path: PathBuf::from_str("./").expect("current directory path is valid"),
            })
            .await?;
        }
    }

    Ok(())
}

async fn build(args: BuildArgs) -> Result<(), BarErr> {
    PATH.set(args.path.clone())
        .expect("Failed to set global path");
    if CONFIG.get().is_none() {
        CONFIG
            .set(Config::try_from(PATH.get().expect("Path to initialized"))?)
            .expect("Failed to set global config");
    }
    let template_path = args
        .path
        .join(&CONFIG.get().expect("config to be initialized").template);

    let (template_path, pages, site) = try_join!(
        canonicalize_with_context(&template_path),
        init_pages(),
        init_site()
    )?;

    let tera = initialize(&template_path, pages.clone(), site.clone())?;

    render(site.clone(), &tera, &pages)?;

    site.save().await?;
    Ok(())
}

async fn create_article(args: ArticleArgs) -> Result<(), BarErr> {
    let path = PathBuf::from(format!("./{}.yamd", args.title));

    if try_exists(&path).await? {
        if args.force {
            remove_file(&path).await?;
        } else {
            return Err(format!(
                "Article with title '{}' already exists at path '{:?}'",
                args.title, path
            )
            .as_str()
            .into());
        }
    }

    let metadata = Metadata {
        title: args.title.clone(),
        date: chrono::Utc::now().into(),
        image: Some("".to_string()),
        preview: Some("".to_string()),
        tags: Some(vec![]),
        is_draft: Some(true),
    };

    let article = Yamd::new(
        Some(serde_yaml::to_string(&metadata)?),
        vec![Paragraph::new(vec![args.title.clone().into()]).into()],
    );

    write_file(&path, Arc::from(article.to_string())).await?;

    println!("Article '{}' is written to: {:?}", args.title, path);

    Ok(())
}
