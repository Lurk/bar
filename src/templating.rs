use crate::{
    PATH,
    fs::crc32_checksum,
    gpx_embed::gpx,
    pages::Pages,
    site::{DynamicPage, Feed, FeedType, Page, Site, StaticPage},
    syntax_highlight::code,
    CONFIG, PATH,
};
use cloudinary::transformation::{
    Image, Transformations,
    aspect_ratio::AspectRatio,
    background::{Auto, AutoModes, Direction, Number},
    crop_mode::CropMode,
    gravity::Gravity,
    pad_mode::PadMode,
};
use crc32fast::Hasher;
use data_encoding::BASE64URL_NOPAD;
use std::{collections::HashMap, path::Path, sync::Arc};
use syntect::parsing::SyntaxSet;
use tera::{Function, Result, Tera, Value};
use tracing::info;
use url::Url;

pub fn get_string_arg(args: &HashMap<String, Value>, key: &str) -> Option<String> {
    match args.get(key) {
        Some(value) => value.as_str().map(|string| string.to_string().clone()),
        None => None,
    }
}

pub fn get_url_arg(args: &HashMap<String, Value>, key: &str) -> Option<Url> {
    match args.get(key) {
        Some(value) => value.as_str().map(|string| {
            Url::parse(string).unwrap_or_else(|_| panic!("could not parse {string} to url"))
        }),
        None => None,
    }
}

pub fn get_arc_str_arg(args: &HashMap<String, Value>, key: &str) -> Option<Arc<str>> {
    match args.get(key) {
        Some(value) => value.as_str().map(Arc::from),
        None => None,
    }
}

fn get_usize_arg(args: &HashMap<String, Value>, key: &str) -> Option<usize> {
    match args.get(key) {
        Some(value) => value
            .as_number()
            .map(|number| number.as_u64().unwrap() as usize),
        None => None,
    }
}

fn add_page(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        let template = get_string_arg(args, "template").unwrap_or("index.html".to_string());
        let title = get_string_arg(args, "title").unwrap_or("".to_string());
        let description = get_string_arg(args, "description").unwrap_or("".to_string());
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

fn add_feed(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_arc_str_arg(args, "path").unwrap();
        let typ: FeedType = get_arc_str_arg(args, "type").unwrap().into();
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
                    let hash = crc32_checksum(
                        inner
                            .source
                            .as_ref()
                            .expect("source of static file to be present"),
                    )
                    .unwrap();
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
        let tag = get_string_arg(args, "tag").unwrap_or("".to_string());
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        let offset = get_usize_arg(args, "offset").unwrap_or(0);
        let pages = pages.get_posts_by_tag(tag.as_str(), limit, offset);
        Ok(tera::to_value(pages)?)
    }
}

fn get_similar(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid").expect("pid is required");
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        Ok(tera::to_value(pages.get_similar(&pid, limit))?)
    }
}

fn get_page_by_path(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").expect("path is required");
        let pid = path.trim_end_matches(".html");
        let page = pages.get(pid);
        Ok(tera::to_value(page)?)
    }
}

fn get_page_by_pid(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid").expect("pid is required");
        let page = pages.get(&pid);
        Ok(tera::to_value(page)?)
    }
}

fn get_transformations(with: Option<usize>, height: Option<usize>) -> Result<Transformations> {
    match (with, height) {
        (None, None) => Err(tera::Error::msg("width or height is required")),
        (None, Some(height)) => Ok(Transformations::Crop(CropMode::FillByHeight {
            height: height as u32,
            ar: None,
            gravity: Some(Gravity::Center),
        })),
        (Some(width), None) => Ok(Transformations::Pad(PadMode::PadByWidth {
            width: width as u32,
            // TODO control aspect_ratio from template
            ar: Some(AspectRatio::Sides(16, 9)),
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

fn get_image_url(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_string_arg(args, "src").expect("get url from src");
        if src.starts_with('/') {
            site.add_page(
                StaticPage {
                    destination: src.trim().into(),
                    source: Some(
                        PATH.get()
                            .expect("Path to be initialized")
                            .join(src.trim().trim_start_matches('/')),
                    ),
                    fallback: None,
                }
                .into(),
            );

            return Ok(tera::to_value(src)?);
        }
        if !src.is_empty() {
            let src = Url::parse(src.as_str()).expect("parse url from src");
            match Image::try_from(src.clone()) {
                Ok(image) => {
                    let transformation = get_transformations(
                        get_usize_arg(args, "width"),
                        get_usize_arg(args, "height"),
                    )?;
                    let result = image.clone().add_transformation(transformation);
                    Ok(tera::to_value(result.to_string())?)
                }
                Err(_) => Ok(tera::to_value(src.to_string())?),
            }
        } else {
            Ok(tera::to_value(src)?)
        }
    }
}

fn render_gpx(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input = get_string_arg(args, "input").expect("input is required");
        let width = get_usize_arg(args, "width").unwrap_or(800) as f64;
        let height = get_usize_arg(args, "height").unwrap_or(600) as f64;
        let base = CONFIG
            .get()
            .expect("CONFIG to be initialized")
            .gpx_embedding
            .base
            .clone();

        let copyright = CONFIG
            .get()
            .expect("CONFIG to be initialized")
            .gpx_embedding
            .copyright_png
            .clone();

        let map_url = tokio::runtime::Handle::current()
            .block_on(async {
                gpx(
                    site.clone(),
                    base,
                    copyright,
                    PATH.get()
                        .expect("Path to be initialized")
                        .join(input.trim().trim_start_matches('/')),
                    width,
                    height,
                )
                .await
            })
            .map_err(|e| format!("{e}"))?;
        Ok(tera::to_value(map_url)?)
    }
}

fn crc32(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let val = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("crc32 filter requires a string value"))?;
    let mut hasher = Hasher::new();
    hasher.update(val.as_bytes());
    let digest = hasher.finalize();
    Ok(tera::to_value(
        BASE64URL_NOPAD.encode(digest.to_be_bytes().as_ref()),
    )?)
}

pub fn initialize(
    template_path: &Path,
    posts: Arc<Pages>,
    site: Arc<Site>,
    syntax_highlighter: Arc<SyntaxSet>,
) -> Result<Tera> {
    let templates = format!("{}/**/*.html", template_path.to_str().unwrap());
    info!("initialize teplates: {}", templates);
    let mut tera = Tera::new(&templates)?;
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function("get_static_file", get_static_file(site.clone()));
    tera.register_function("get_pages_by_tag", get_pages_by_tag(posts.clone()));
    tera.register_function("get_page_by_path", get_page_by_path(posts.clone()));
    tera.register_function("get_page_by_pid", get_page_by_pid(posts.clone()));
    tera.register_function("get_similar", get_similar(posts.clone()));
    tera.register_function("code", code(syntax_highlighter));
    tera.register_function("get_image_url", get_image_url(site.clone()));
    tera.register_function("add_feed", add_feed(site.clone()));
    tera.register_function("render_gpx", render_gpx(site.clone()));

    tera.register_filter("crc32", crc32);

    info!("template initialization complete");
    Ok(tera)
}

#[cfg(test)]
mod tests {
    use super::get_transformations;

    #[test]
    fn get_transformations_test() {
        assert!(get_transformations(None, None).is_err());
        assert!(get_transformations(Some(100), None).is_ok());
        assert!(get_transformations(None, Some(100)).is_ok());
        assert!(get_transformations(Some(100), Some(100)).is_ok());
    }
}
