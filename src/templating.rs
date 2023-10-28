use crate::{
    error::Errors,
    posts::Posts,
    site::{Page, Site},
    syntax_highlight::{code, init},
    Config,
};
use cloudinary::transformation::{crop_mode::CropMode, gravity::Gravity, Image, Transformations};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
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
        Some(value) => value.as_str().map(|string| Url::parse(string).unwrap()),
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

fn add_page(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        let template = get_string_arg(args, "template").unwrap_or("index.html".to_string());
        let title = get_string_arg(args, "title").unwrap_or("".to_string());
        let description = get_string_arg(args, "description").unwrap_or("".to_string());
        site.add_page(Arc::new(Page::new(path, template, title, description)));
        Ok(tera::to_value(())?)
    }
}

fn add_static_file(site: Arc<Site>, config: Arc<Config>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        if let (Some(path), Some(file_path)) = (
            get_string_arg(args, "path"),
            get_string_arg(args, "file_path"),
        ) {
            let now = SystemTime::now();
            let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
            site.add_static_file(path.clone(), config.template.join(file_path));
            return Ok(tera::to_value(format!(
                "{}?cb={}",
                &path,
                since_the_epoch.as_millis()
            ))?);
        }
        Err(tera::Error::msg("path and file_path are required"))
    }
}

fn get_posts_by_tag(posts: Arc<Posts>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let tag = get_string_arg(args, "tag").unwrap_or("".to_string());
        let amount = get_usize_arg(args, "amount").unwrap_or(3);
        let posts = posts.get_posts_by_tag(tag.as_str(), amount);
        Ok(tera::to_value(posts)?)
    }
}

fn get_post_by_path(posts: Arc<Posts>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap();
        let pid = path.trim_end_matches(".html").trim_start_matches("post/");
        let post = posts.get(pid);
        Ok(tera::to_value(post)?)
    }
}

fn prepare_srcset_for_cloudinary_image() -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_url_arg(args, "src").unwrap();

        match Image::try_from(src.clone()) {
            Ok(image) => {
                let result: String = get_vec_of_usize_arg(args, "breakpoints")
                    .unwrap()
                    .iter()
                    .map(|width| {
                        let local_image = image.clone().add_transformation(Transformations::Crop(
                            CropMode::FillByWidth {
                                width: *width as u32,
                                ar: None,
                                gravity: Some(Gravity::AutoClassic),
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

fn get_image_url() -> impl Function + 'static {
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
        let src = get_url_arg(args, "src").unwrap();
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
    template_path: &Path,
    config: Arc<Config>,
    posts: Arc<Posts>,
    site: Arc<Site>,
) -> Result<Tera, Errors> {
    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function(
        "add_static_file",
        add_static_file(site.clone(), config.clone()),
    );
    tera.register_function("get_posts_by_tag", get_posts_by_tag(posts.clone()));
    tera.register_function("get_post_by_path", get_post_by_path(posts.clone()));
    tera.register_function(
        "prepare_srcset_for_cloudinary_image",
        prepare_srcset_for_cloudinary_image(),
    );
    tera.register_function("code", code(init()?));
    tera.register_function("get_image_url", get_image_url());
    Ok(tera)
}
