use std::{path::PathBuf, sync::Arc, time::Duration};

use data_encoding::BASE64URL_NOPAD;
use gpxtools::{plot, PlotArgs};
use tracing::debug;

use crate::{
    cache::Cache,
    error::BarErr,
    req::get_client,
    site::{Site, StaticPage},
    PATH,
};

pub async fn gpx(
    site: Arc<Site>,
    base: Vec<Arc<str>>,
    copyright: Option<PathBuf>,
    input: PathBuf,
    width: f64,
    height: f64,
) -> Result<String, BarErr> {
    let path = BASE64URL_NOPAD.encode(
        crc32fast::hash(input.to_string_lossy().as_bytes())
            .to_be_bytes()
            .as_ref(),
    );

    let destination = PATH
        .get()
        .expect("PATH should be initialized")
        .join(format!(".cache/gpx_embed/{width}/{height}/{path}.png"));

    plot(
        |url| Box::pin(async move { read_tile(url).await }),
        PlotArgs {
            input,
            width,
            height,
            base,
            copyright,
            output: destination.clone(),
            force: false,
        },
    )
    .await?;

    let url = format!("/public/gpx_embed/{width}/{height}/{path}.png");

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
    let cache = Cache::<(String, Vec<u8>)>::new("gpx_tiles", 1)
        .with_ttl(Duration::from_secs(60 * 60 * 24 * 31));

    let key = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    if let Some(cached) = cache.get(key).map_err(|e| format!("{e}"))? {
        debug!("Tile found in cache: {}", url);
        return Ok(cached);
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

    cache
        .set(key, &(url.clone(), bytes.clone()))
        .await
        .map_err(|e| format!("{e}"))?;

    debug!("Tile cached: {}", url);
    Ok((url, bytes))
}
