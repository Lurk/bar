use crate::{
    context::BuildContext,
    fs::seahash_checksum,
    gpx_embed::gpx,
    pages::Pages,
    render::RenderedContentCache,
    site::{DynamicPage, Feed, FeedType, Page, Site, StaticPage},
};
use cloudinary::transformation::{
    Image, Transformations,
    aspect_ratio::AspectRatio,
    background::{Auto, AutoModes, Direction, Number},
    crop_mode::CropMode,
    gravity::Gravity,
    pad_mode::PadMode,
};
use data_encoding::BASE64URL_NOPAD;
use gpxtools::{StatsArgs, calculate_stats};
use std::{
    collections::HashMap,
    hash::BuildHasher,
    path::{Path, PathBuf},
    sync::Arc,
};
use tera::{Function, Result, Tera, Value};
use tracing::info;
use url::Url;

#[must_use]
pub fn get_string_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> Option<String> {
    match args.get(key) {
        Some(value) => value.as_str().map(std::string::ToString::to_string),
        None => None,
    }
}

/// # Errors
/// Returns error if the value cannot be parsed as a URL.
pub fn get_url_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> std::result::Result<Option<Url>, tera::Error> {
    match args.get(key) {
        Some(value) => match value.as_str() {
            Some(string) => Url::parse(string)
                .map(Some)
                .map_err(|e| tera::Error::msg(format!("could not parse '{string}' to url: {e}"))),
            None => Ok(None),
        },
        None => Ok(None),
    }
}

pub fn get_arc_str_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> Option<Arc<str>> {
    match args.get(key) {
        Some(value) => value.as_str().map(Arc::from),
        None => None,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn get_usize_arg(args: &HashMap<String, Value>, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(|value| value.as_number())
        .and_then(serde_json::Number::as_u64)
        .map(|n| n as usize)
}

fn add_page(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        let template = get_string_arg(args, "template").unwrap_or("index.html".to_string());
        let title = get_string_arg(args, "title").unwrap_or_default();
        let description = get_string_arg(args, "description").unwrap_or_default();
        let page_num = get_usize_arg(args, "page_num").unwrap_or(0);
        site.add_page(
            DynamicPage {
                path: path.into(),
                template: template.into(),
                title: title.into(),
                description: description.into(),
                page_num,
                content: None,
            }
            .into(),
        );
        Ok(tera::to_value(())?)
    }
}

fn add_static_file(site: Arc<Site>, project_path: Arc<PathBuf>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_arc_str_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for add_static_file"))?;
        let p = &project_path;
        let source = if let Some(source) = get_arc_str_arg(args, "source") {
            p.join(source.trim().trim_start_matches('/'))
        } else {
            p.join(path.trim().trim_start_matches('/'))
        };

        site.add_page(
            StaticPage {
                destination: path.clone().trim().into(),
                source: Some(source),
                fallback: None,
            }
            .into(),
        );
        Ok(tera::to_value(path)?)
    }
}

fn add_feed(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_arc_str_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for add_feed"))?;
        let typ_str = get_arc_str_arg(args, "type")
            .ok_or_else(|| tera::Error::msg("type is required for add_feed"))?;
        let typ: FeedType =
            FeedType::try_from(typ_str).map_err(|e| tera::Error::msg(format!("{e}")))?;
        site.add_page(
            Feed {
                path: path.clone(),
                typ,
                content: None,
            }
            .into(),
        );
        Ok(tera::to_value(path)?)
    }
}

fn get_static_file(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        if let Some(path) = get_string_arg(args, "path") {
            if let Some(page) = site.get_page(path.trim_start_matches('/')) {
                if let Page::Static(inner) = page.as_ref() {
                    let source = inner.source.as_ref().ok_or_else(|| {
                        tera::Error::msg(format!("static file '{path}' has no source"))
                    })?;
                    let hash = seahash_checksum(source)
                        .map_err(|e| tera::Error::msg(format!("failed to hash '{path}': {e}")))?;
                    return Ok(tera::to_value(format!("{path}?cb={hash}"))?);
                }
                return Err(tera::Error::msg(format!(
                    "{path} is not a path to static resource"
                )));
            }
            return Err(tera::Error::msg(format!("{path} not found")));
        }
        Err(tera::Error::msg("path is required"))
    }
}

fn get_pages_by_tag(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let tag = get_string_arg(args, "tag").unwrap_or_default();
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        let offset = get_usize_arg(args, "offset").unwrap_or(0);
        let pages = pages
            .get_posts_by_tag(tag.as_str(), limit, offset)
            .ok_or_else(|| tera::Error::msg(format!("tag '{tag}' not found")))?;
        Ok(tera::to_value(pages)?)
    }
}

fn get_similar(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid")
            .ok_or_else(|| tera::Error::msg("pid is required for get_similar"))?;
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        Ok(tera::to_value(pages.get_similar(&pid, limit))?)
    }
}

fn get_page_by_path(
    pages: Arc<Pages>,
    rendered_cache: RenderedContentCache,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for get_page_by_path"))?;
        let pid = path.trim_end_matches(".html");
        let page = pages.get(pid);
        let mut val = tera::to_value(page)?;
        if let Some(obj) = val.as_object_mut() {
            let cache = rendered_cache.lock().expect("rendered cache poisoned");
            if let Some(rendered) = cache.get(pid) {
                obj.insert("rendered_html".into(), rendered.html.clone().into());
                obj.insert("rendered_css".into(), rendered.css.clone().into());
            }
        }
        Ok(val)
    }
}

fn get_page_by_pid(
    pages: Arc<Pages>,
    rendered_cache: RenderedContentCache,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid")
            .ok_or_else(|| tera::Error::msg("pid is required for get_page_by_pid"))?;
        let page = pages.get(&pid);
        let mut val = tera::to_value(page)?;
        if let Some(obj) = val.as_object_mut() {
            let cache = rendered_cache.lock().expect("rendered cache poisoned");
            if let Some(rendered) = cache.get(pid.as_str()) {
                obj.insert("rendered_html".into(), rendered.html.clone().into());
                obj.insert("rendered_css".into(), rendered.css.clone().into());
            }
        }
        Ok(val)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::similar_names)]
fn get_transformations(
    with: Option<usize>,
    height: Option<usize>,
    aspect_ratio: Option<(u32, u32)>,
) -> Result<Transformations> {
    match (with, height) {
        (None, None) => Err(tera::Error::msg("width or height is required")),
        (None, Some(height)) => Ok(Transformations::Crop(CropMode::FillByHeight {
            height: height as u32,
            ar: aspect_ratio.map(|(w, h)| AspectRatio::Sides(w, h)),
            gravity: Some(Gravity::Center),
        })),
        (Some(width), None) => Ok(Transformations::Pad(PadMode::PadByWidth {
            width: width as u32,
            ar: Some(
                aspect_ratio.map_or(AspectRatio::Sides(16, 9), |(w, h)| AspectRatio::Sides(w, h)),
            ),
            gravity: Some(Gravity::Center),
            background: Some(
                Auto {
                    mode: Some(AutoModes::BorderGradient),
                    number: Some(Number::Four),
                    direction: Some(Direction::Vertical),
                    palette: None,
                }
                .into(),
            ),
        })),
        (Some(width), Some(height)) => Ok(Transformations::Crop(CropMode::Fill {
            width: width as u32,
            height: height as u32,
            gravity: Some(Gravity::Center),
        })),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn get_image_url(site: Arc<Site>, project_path: Arc<PathBuf>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_string_arg(args, "src")
            .ok_or_else(|| tera::Error::msg("src is required for get_image_url"))?;
        if src.starts_with('/') {
            site.add_page(
                StaticPage {
                    destination: src.trim().into(),
                    source: Some(project_path.join(src.trim().trim_start_matches('/'))),
                    fallback: None,
                }
                .into(),
            );

            return Ok(tera::to_value(src)?);
        }
        if src.is_empty() {
            Ok(tera::to_value(src)?)
        } else {
            let src = Url::parse(src.as_str())
                .map_err(|e| tera::Error::msg(format!("failed to parse url '{src}': {e}")))?;
            match Image::try_from(src.clone()) {
                Ok(image) => {
                    let ar = match (
                        get_usize_arg(args, "ar_width"),
                        get_usize_arg(args, "ar_height"),
                    ) {
                        (Some(w), Some(h)) => Some((w as u32, h as u32)),
                        _ => None,
                    };
                    let transformation = get_transformations(
                        get_usize_arg(args, "width"),
                        get_usize_arg(args, "height"),
                        ar,
                    )?;
                    let result = image.clone().add_transformation(transformation);
                    Ok(tera::to_value(result.to_string())?)
                }
                Err(_) => Ok(tera::to_value(src.to_string())?),
            }
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn render_gpx(
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input = get_string_arg(args, "input")
            .ok_or_else(|| tera::Error::msg("input is required for render_gpx"))?;
        let width = get_usize_arg(args, "width").unwrap_or(800) as f64;
        let height = get_usize_arg(args, "height").unwrap_or(600) as f64;
        let base = config.gpx_embedding.base.clone();
        let copyright = config.gpx_embedding.attribution_png.clone();

        let map_url = tokio::runtime::Handle::try_current()
            .map_err(|e| format!("{e}"))?
            .block_on(async {
                gpx(
                    site.clone(),
                    base,
                    copyright,
                    project_path.join(input.trim().trim_start_matches('/')),
                    width,
                    height,
                    project_path.as_ref().clone(),
                )
                .await
            })
            .map_err(|e| format!("{e}"))?;
        Ok(tera::to_value(map_url)?)
    }
}

fn get_gpx_stats(project_path: Arc<PathBuf>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input_arg = get_string_arg(args, "input")
            .ok_or_else(|| tera::Error::msg("input is required for get_gpx_stats"))?;
        let input = project_path.join(input_arg.trim().trim_start_matches('/'));

        let stats = calculate_stats(&StatsArgs {
            input: vec![input.clone()],
        })
        .map_err(|e| format!("{e}"))?;

        let stat = stats
            .first()
            .ok_or_else(|| tera::Error::msg(format!("no stats for {}", input.display())))?;

        Ok(tera::to_value(stat)?)
    }
}

fn crc32(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let val = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("crc32 filter requires a string value"))?;
    Ok(tera::to_value(BASE64URL_NOPAD.encode(
        seahash::hash(val.as_bytes()).to_be_bytes().as_ref(),
    ))?)
}

pub fn register_functions(
    tera: &mut Tera,
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
    pages: &Arc<Pages>,
    rendered_cache: RenderedContentCache,
) {
    tera.register_function("add_feed", add_feed(site.clone()));
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function(
        "add_static_file",
        add_static_file(site.clone(), project_path.clone()),
    );
    tera.register_function("get_gpx_stats", get_gpx_stats(project_path.clone()));
    tera.register_function(
        "get_image_url",
        get_image_url(site.clone(), project_path.clone()),
    );
    tera.register_function("get_pages_by_tag", get_pages_by_tag(pages.clone()));
    tera.register_function(
        "get_page_by_path",
        get_page_by_path(pages.clone(), rendered_cache.clone()),
    );
    tera.register_function(
        "get_page_by_pid",
        get_page_by_pid(pages.clone(), rendered_cache),
    );
    tera.register_function("get_similar", get_similar(pages.clone()));
    tera.register_function("get_static_file", get_static_file(site.clone()));
    tera.register_function("render_gpx", render_gpx(site, config, project_path));
    tera.register_filter("crc32", crc32);
}

/// # Errors
/// Returns error if templates cannot be loaded or parsed.
pub fn initialize(
    ctx: &BuildContext,
    template_path: &Path,
    rendered_cache: RenderedContentCache,
) -> Result<Tera> {
    let templates = format!(
        "{}/**/*.html",
        template_path
            .to_str()
            .ok_or_else(|| tera::Error::msg(format!(
                "template path is not valid UTF-8: {}",
                template_path.display()
            )))?
    );
    info!("initialize teplates: {}", templates);
    let project_path = Arc::new(ctx.config.path.clone());
    let config = Arc::new(ctx.config.config.clone());
    let mut tera = Tera::new(&templates)?;
    register_functions(
        &mut tera,
        ctx.site.clone(),
        config,
        project_path,
        &ctx.pages,
        rendered_cache,
    );
    info!("template initialization complete");
    Ok(tera)
}

#[cfg(test)]
mod tests {
    use super::get_transformations;

    #[test]
    fn get_transformations_test() {
        assert!(get_transformations(None, None, None).is_err());
        assert!(get_transformations(Some(100), None, None).is_ok());
        assert!(get_transformations(None, Some(100), None).is_ok());
        assert!(get_transformations(Some(100), Some(100), None).is_ok());
        assert!(get_transformations(Some(100), None, Some((4, 3))).is_ok());
        assert!(get_transformations(None, Some(100), Some((4, 3))).is_ok());
    }
}
