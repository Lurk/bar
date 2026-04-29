mod args;
pub mod r#async;
mod cache;
mod cloudinary;
pub mod config;
pub mod context;
pub mod diagnostic;
pub mod fs;
mod gpx_embed;
mod image_alt;
pub mod json_feed;
mod metadata;
pub mod pages;
pub mod renderer;
mod req;
pub mod site;
pub mod syntax_highlight;
pub mod templating;

use args::{Args, ArticleArgs, BuildArgs, Commands};
use clap::Parser;
use config::Config;
use context::{BuildConfig, BuildContext};
use diagnostic::BarDiagnostic;
use fs::write_file;
use metadata::Metadata;
use renderer::render;
use site::init_site;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use templating::initialize;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, try_exists};
use tokio::try_join;
use tracing::subscriber;
use tracing_log::AsTrace;
use tracing_subscriber::FmtSubscriber;
use yamd::Yamd;
use yamd::nodes::Paragraph;

use crate::diagnostic::ContextExt;
use crate::fs::canonicalize_with_context;
use crate::pages::init_pages;
use crate::syntax_highlight::init;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let handle = tokio::spawn(async {
        let res = match args.command {
            Some(Commands::Build(build_args)) => build(build_args).await,
            Some(Commands::Article(article_args)) => create_article(article_args).await,
            Some(Commands::Clear(clear_rgs)) => clear(clear_rgs).await,
            None => {
                build(BuildArgs {
                    path: PathBuf::from_str("./").expect("current directory path is valid"),
                })
                .await
            }
        };
        if let Err(e) = res {
            eprintln!("{e:?}");
        }
    });

    handle.await.expect("tokio task panicked");
}

async fn build(args: BuildArgs) -> Result<(), BarDiagnostic> {
    let build_config = BuildConfig {
        config: Config::try_from(&args.path)?,
        path: args.path,
    };

    let template_path = build_config.path.join(&build_config.config.template);

    let (template_path, pages, site) = try_join!(
        canonicalize_with_context(&template_path),
        init_pages(&build_config),
        init_site(&build_config)
    )?;

    let syntax_set = init()?;

    let ctx = Arc::new(BuildContext {
        config: build_config,
        pages,
        site,
        syntax_set,
    });

    let tera = initialize(&ctx, &template_path)?;

    let ctx_clone = ctx.clone();
    tokio::task::spawn_blocking(move || render(&ctx_clone, &tera)).await??;

    ctx.site.save().await?;
    Ok(())
}

async fn create_article(args: ArticleArgs) -> Result<(), BarDiagnostic> {
    let path = PathBuf::from(format!("./{}.yamd", args.title));

    if try_exists(&path).await? {
        if args.force {
            remove_file(&path).await?;
        } else {
            return Err(format!(
                "Article with title '{}' already exists at path '{}'",
                args.title,
                path.display()
            )
            .as_str()
            .into());
        }
    }

    let metadata = Metadata {
        title: args.title.clone(),
        date: chrono::Utc::now().into(),
        image: Some(String::new()),
        preview: Some(String::new()),
        tags: Some(vec![]),
        is_draft: Some(true),
    };

    let article = Yamd::new(
        Some(serde_yaml::to_string(&metadata)?),
        vec![Paragraph::new(vec![args.title.clone().into()]).into()],
    );

    write_file(&path, article.to_string().as_bytes()).await?;

    println!("Article '{}' is written to: {}", args.title, path.display());

    Ok(())
}

async fn clear(args: BuildArgs) -> Result<(), BarDiagnostic> {
    let build_config = BuildConfig {
        config: Config::try_from(&args.path)?,
        path: args.path,
    };
    let cache_path = build_config.path.join(".cache");
    let dist_path = build_config.path.join(&build_config.config.dist_path);

    create_dir_all(&dist_path)
        .await
        .with_context(|| format!("create directory: {}", dist_path.display()))?;

    remove_dir_all(&dist_path)
        .await
        .with_context(|| format!("remove directory: {}", dist_path.display()))?;

    create_dir_all(&cache_path)
        .await
        .with_context(|| format!("create directory: {}", cache_path.display()))?;

    remove_dir_all(&cache_path)
        .await
        .with_context(|| format!("remove directory: {}", cache_path.display()))?;

    Ok(())
}
