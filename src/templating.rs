use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tera::{Function, Tera, Value};

use crate::{
    error::Errors,
    posts::Posts,
    site::{Page, Site},
    Config,
};

fn get_string_arg(args: &HashMap<String, Value>, key: &str) -> Option<String> {
    match args.get(key) {
        Some(value) => value.as_str().map(|string| string.to_string().clone()),
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
        Ok(tera::to_value(&posts)?)
    }
}

pub fn initialize(
    template_path: &PathBuf,
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
    Ok(tera)
}
