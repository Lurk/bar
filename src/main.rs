pub mod r#async;
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

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use config::Config;
use error::BarErr;
use renderer::render;
use site::init_site;
use std::path::PathBuf;
use std::sync::Arc;
use templating::initialize;
use tokio::try_join;
use tracing::subscriber;
use tracing_log::AsTrace;
use tracing_subscriber::FmtSubscriber;

use crate::fs::canonicalize_with_context;
use crate::pages::init_pages;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the project directory
    #[clap(default_value = ".")]
    path: PathBuf,
    #[command(flatten)]
    verbose: Verbosity,
}

#[tokio::main]
async fn main() -> Result<(), BarErr> {
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let path: &'static PathBuf = Box::leak(Box::new(args.path.clone()));
    let config: Arc<Config> = Arc::new(Config::try_from(path)?);
    let template_path = args.path.join(&config.template);

    let (template_path, pages, site) = try_join!(
        canonicalize_with_context(&template_path),
        init_pages(path, config.clone()),
        init_site(path, config.clone())
    )?;

    let tera = initialize(path, &template_path, pages.clone(), site.clone())?;

    render(site.clone(), &config, &tera, &pages)?;

    site.save().await?;
    Ok(())
}
