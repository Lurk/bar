use std::path::PathBuf;

use clap::Parser;
use directories::BaseDirs;
use gpxtools::{PathsArgs, PlotArgs, StatsArgs, calculate_stats, join, plot, stats::Stats};
use tabled::{
    Table,
    settings::{Border, Style, object::Rows},
};
use tokio::{
    fs::{OpenOptions, create_dir_all},
    io::AsyncWriteExt,
};

#[derive(clap::Subcommand)]
enum Command {
    /// Join multiple files into one
    Join(PathsArgs),
    /// Plot GPX data to an image
    Plot(PlotArgs),
    /// Show statistics about a GPX file
    Stats(StatsArgs),
}

#[derive(clap::Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let result = match args.command {
        Command::Join(paths) => join(paths),
        Command::Plot(p) => plot(|src| Box::pin(async move { read_tile(src).await }), p).await,
        Command::Stats(s) => {
            let stats = calculate_stats(s);
            match stats {
                Ok(mut results) => {
                    let sum = results.iter().fold(
                        Stats {
                            file: "total".to_string(),
                            distance_km: 0.,
                            total_ascent_m: 0.,
                        },
                        |mut acc, row| {
                            acc.distance_km += row.distance_km;
                            acc.total_ascent_m += row.total_ascent_m;
                            acc
                        },
                    );
                    results.push(sum);
                    let mut table = Table::new(results);
                    table.with(Style::sharp());
                    table.modify(Rows::last(), Border::inherit(Style::modern()));
                    println!("{}", table);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn get_cache_path(url: &str) -> Result<PathBuf, String> {
    let p = BaseDirs::new()
        .expect("Failed to get home directory")
        .cache_dir()
        .join(format!(
            "gpxtools/tiles/{}",
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
        ));

    create_dir_all(p.parent().expect("cache path to have parent"))
        .await
        .map_err(|e| format!("creating folders for cache path {p:?} failed with error:\n{e:?}"))?;

    Ok(p)
}

async fn read_tile(url: String) -> Result<(String, Vec<u8>), String> {
    let destination = get_cache_path(&url).await?;

    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");

    if !destination.exists() {
        let response = reqwest::Client::builder()
            // TODO: add url to repo to user agent
            .user_agent(format!("{}/{}", name, version))
            .build()
            .expect("Failed to build reqwest client")
            .get(&url)
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
        return Ok((url, bytes));
    }

    let bytes = tokio::fs::read(&destination)
        .await
        .map_err(|e| format!("Failed to read file {destination:?}: {e}"))?;

    Ok((url, bytes))
}
