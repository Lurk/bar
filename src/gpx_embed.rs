use std::{path::PathBuf, sync::Arc};

use data_encoding::BASE64URL_NOPAD;
use gpxtools::{PlotArgs, plot};
use tracing::debug;

use crate::{
    PATH,
    error::BarErr,
    fs::write_file,
    req::get_client,
    site::{Site, StaticPage},
};

pub async fn gpx(
    site: Arc<Site>,
    base: Vec<Arc<str>>,
    attribution_png: Option<PathBuf>,
    input: PathBuf,
    width: f64,
    height: f64,
) -> Result<String, BarErr> {
    let filename = BASE64URL_NOPAD.encode(
        crc32fast::hash(format!("{}{}", input.to_string_lossy(), base.join("")).as_bytes())
            .to_be_bytes()
            .as_ref(),
    );

    let destination = PATH
        .get()
        .expect("PATH should be initialized")
        .join(format!(".cache/gpx_embed/{width}/{height}/{filename}.png"));

    if !destination.exists() {
        plot(
            |url| Box::pin(async move { read_tile(url).await }),
            PlotArgs {
                input,
                width,
                height,
                base,
                attribution_png: attribution_png
                    .map(|p| PATH.get().expect("PATH should be initialized").join(p)),
                output: destination.clone(),
                force: false,
            },
        )
        .await?;
    }

    let url = format!("/public/gpx_embed/{width}/{height}/{filename}.png");

    site.add_page(
        StaticPage {
            destination: Arc::from(url.clone()),
            source: Some(destination),
            fallback: None,
        }
        .into(),
    );
    Ok(url)
}

async fn read_tile(url: String) -> Result<(String, Vec<u8>), String> {
    debug!("Fetching tile: {}", url);

    let key = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    let destination = PATH
        .get()
        .expect("PATH should be initialized")
        .join(format!(".cache/gpx_tile/{key}"));

    if destination.exists() {
        debug!("Tile found in cache: {}", url);
        let bytes = tokio::fs::read(&destination)
            .await
            .map_err(|e| format!("Failed to read cached tile at {:?}.\n{e}", &destination))?;
        return Ok((url, bytes));
    }

    let response = get_client()
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

    write_file(&destination, &bytes)
        .await
        .map_err(|e| format!("Failed to write tile to cache at {:?}.\n{e}", &destination))?;

    debug!("Tile cached: {}", url);
    Ok((url, bytes))
}
