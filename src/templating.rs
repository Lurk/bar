use crate::{
    error::Errors,
    fs::crc32_checksum,
    pages::Pages,
    site::{DynamicPage, Feed, FeedType, Site, StaticPage},
    syntax_highlight::{code, init},
    Config,
};
use cloudinary::transformation::{
    aspect_ratio::AspectRatio,
    background::{Auto, AutoModes, Direction, Number},
    crop_mode::CropMode,
    gravity::Gravity,
    pad_mode::PadMode,
    Image, Transformations,
};
use std::{collections::HashMap, path::Path, sync::Arc};
use tera::{Function, Tera, Value};
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

fn get_vec_of_usize_arg(args: &HashMap<String, Value>, key: &str) -> Option<Vec<usize>> {
    match args.get(key) {
        Some(value) => value.as_array().map(|array| {
            array
                .iter()
                .map(|number| number.as_u64().unwrap() as usize)
                .collect()
        }),
        None => None,
    }
}

fn get_bool_arg(args: &HashMap<String, Value>, key: &str) -> Option<bool> {
    match args.get(key) {
        Some(value) => value.as_bool(),
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

fn add_static_file(
    site: Arc<Site>,
    config: Arc<Config>,
    content_path: &'static Path,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        if let (Some(path), Some(file_path)) = (
            get_arc_str_arg(args, "path"),
            get_arc_str_arg(args, "file_path"),
        ) {
            let is_content = get_bool_arg(args, "is_content").unwrap_or(false);
            let static_path = if is_content {
                content_path.join(file_path.trim())
            } else {
                config.template.join(file_path.trim())
            };
            let hash = crc32_checksum(&static_path).unwrap();
            site.add_page(
                StaticPage {
                    destination: path.clone(),
                    source: static_path,
                }
                .into(),
            );
            return Ok(tera::to_value(format!("{}?cb={}", path, hash))?);
        }
        Err(tera::Error::msg("path and file_path are required"))
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

fn get_page_by_path(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").expect("path is required");
        let pid = path.trim_end_matches(".html");
        let page = pages.get(pid);
        Ok(tera::to_value(page)?)
    }
}

fn prepare_srcset_for_cloudinary_image() -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_string_arg(args, "src").unwrap();
        if src.starts_with('/') {
            return Ok(tera::to_value(src)?);
        }
        let src = Url::parse(src.as_str()).expect("parse url from src");
        match Image::try_from(src.clone()) {
            Ok(image) => {
                let result: String = get_vec_of_usize_arg(args, "breakpoints")
                    .unwrap()
                    .iter()
                    .map(|width| {
                        let local_image = image.clone().add_transformation(Transformations::Pad(
                            PadMode::PadByWidth {
                                width: *width as u32,
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
                            },
                        ));
                        format!("{} {}w", local_image, width)
                    })
                    .collect::<Vec<String>>()
                    .join(",");
                Ok(tera::to_value(result)?)
            }
            Err(_) => todo!(),
        }
    }
}

fn get_image_url(site: Arc<Site>, path: &'static Path) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let crop_mode: CropMode =
            match (get_usize_arg(args, "width"), get_usize_arg(args, "height")) {
                (None, None) => return Err(tera::Error::msg("width or height is required")),
                (None, Some(height)) => CropMode::FillByHeight {
                    height: height as u32,
                    ar: None,
                    gravity: Some(Gravity::AutoClassic),
                },
                (Some(width), None) => CropMode::FillByWidth {
                    width: width as u32,
                    ar: None,
                    gravity: Some(Gravity::AutoClassic),
                },
                (Some(width), Some(height)) => CropMode::Fill {
                    width: width as u32,
                    height: height as u32,
                    gravity: Some(Gravity::AutoClassic),
                },
            };
        let src = get_string_arg(args, "src").expect("get url from src");
        if src.starts_with('/') {
            site.add_page(
                StaticPage {
                    destination: src.trim().into(),
                    source: path.join(src.trim().trim_start_matches('/')),
                }
                .into(),
            );

            return Ok(tera::to_value(src)?);
        }
        let src = Url::parse(src.as_str()).expect("parse url from src");
        match Image::try_from(src.clone()) {
            Ok(image) => {
                let result = image
                    .clone()
                    .add_transformation(Transformations::Crop(crop_mode));
                Ok(tera::to_value(result.to_string())?)
            }
            Err(_) => Ok(tera::to_value(src.to_string())?),
        }
    }
}

pub fn initialize(
    path: &'static Path,
    template_path: &Path,
    config: Arc<Config>,
    posts: Arc<Pages>,
    site: Arc<Site>,
) -> Result<Tera, Errors> {
    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function(
        "add_static_file",
        add_static_file(site.clone(), config.clone(), path),
    );
    tera.register_function("get_pages_by_tag", get_pages_by_tag(posts.clone()));
    tera.register_function("get_page_by_path", get_page_by_path(posts.clone()));
    tera.register_function(
        "prepare_srcset_for_cloudinary_image",
        prepare_srcset_for_cloudinary_image(),
    );
    tera.register_function("code", code(init()?));
    tera.register_function("get_image_url", get_image_url(site.clone(), path));
    tera.register_function("add_feed", add_feed(site.clone()));
    Ok(tera)
}
