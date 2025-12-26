use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use directories::BaseDirs;
use img2text::{Img2Text, error::Img2TextError};
use tabled::{
    Table,
    settings::{Style, Width, peaker::Priority},
};
use terminal_size::{Width, terminal_size};
use tokio::{
    fs::{OpenOptions, create_dir_all},
    io::AsyncWriteExt,
};
use tracing::debug;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    pub log_level: Verbosity,
    #[clap(flatten)]
    img2text: Img2TextArgs,
}

#[derive(clap::Parser)]
pub struct Img2TextArgs {
    /// Path or URL to the image, can be specified multiple times
    #[clap(short, long)]
    pub source: Vec<Arc<str>>,
    /// Prompt to generate alt text
    #[clap(short, long)]
    pub prompt: String,
    /// Temperature for generation from 0.0 to 1.0
    #[clap(short, long, default_value_t = 0.1)]
    pub temperature: f64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_max_level(args.log_level.tracing_level_filter())
        .compact()
        .init();
    match generate(&args.img2text, std::sync::Arc::new(Img2Text::new())).await {
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
        Ok(captions) => {
            let (Width(width), _) = terminal_size().expect("terminal size");
            let mut table = Table::new(captions);
            table.with((
                Width::wrap((width) as usize).priority(Priority::max(true)),
                Width::increase((width) as usize).priority(Priority::min(true)),
            ));
            table.with(Style::modern());
            println!("{}", table);
        }
    }
}

async fn get_cache_path(url: &str) -> Result<PathBuf, String> {
    let name = env!("CARGO_PKG_NAME");

    let p = BaseDirs::new()
        .expect("Failed to get home directory")
        .cache_dir()
        .join(format!(
            "{}/{}",
            name,
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
        ));

    create_dir_all(p.parent().expect("cache path to have parent"))
        .await
        .map_err(|e| format!("creating folders for cache path {p:?} failed with error:\n{e:?}"))?;

    Ok(p)
}

async fn read_file(url: Arc<str>) -> Result<PathBuf, String> {
    let destination = get_cache_path(&url).await?;

    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");

    if !destination.exists() {
        let response = reqwest::Client::builder()
            // TODO: add url to repo to user agent
            .user_agent(format!("{}/{}", name, version))
            .build()
            .expect("Failed to build reqwest client")
            .get(url.as_ref())
            .send()
            .await
            .map_err(|e| format!("{e:?}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to fetch image: {}\nstatus code: {}",
                url,
                response.status()
            ));
        }

        let bytes: Vec<u8> = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read bytes from \"{url}\".\n{e}"))?
            .into_iter()
            .collect();

        let prefix = destination.parent().expect("destination to have parent");
        create_dir_all(prefix)
            .await
            .map_err(|e| format!("Failed to create cache directory.\n{e}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&destination)
            .await
            .map_err(|e| format!("Failed to open cache file at {destination:?}.\n{e}"))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| format!("Failed to write to {destination:?}.\n{e}"))?;
        file.flush()
            .await
            .map_err(|e| format!("Failed flush the cache file {destination:?}\n{e}"))?;
        return Ok(destination);
    }

    Ok(destination)
}

#[derive(tabled::Tabled)]
struct Res {
    source: Arc<str>,
    caption: Arc<str>,
}

async fn generate(
    args: &Img2TextArgs,
    generator: Arc<Img2Text>,
) -> Result<Vec<Res>, Img2TextError> {
    let mut captions = Vec::new();
    for source in args.source.iter() {
        debug!("processing source: {}", source);
        let path = if source.starts_with("http") {
            let path_buf = read_file(source.clone())
                .await
                .map_err(Img2TextError::ImageGetter)?;
            debug!("downloaded the image to temporary path: {:?}", path_buf);
            path_buf
        } else {
            PathBuf::from(source.as_ref())
        };

        let caption = generator
            .as_ref()
            .run(&path, &args.prompt, args.temperature)
            .await?;

        captions.push(Res {
            source: source.clone(),
            caption,
        });
    }

    Ok(captions)
}
