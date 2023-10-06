pub mod error;
pub mod posts;

use clap::Parser;
use error::Errors;
use serde::{Deserialize, Serialize};
use std::default;
use std::path::PathBuf;
use std::{collections::HashMap, fs::File};
use tera::{Context, Function, Tera, Value};

use crate::posts::init_from_path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the project directory
    #[arg(short, long)]
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    content_path: PathBuf,
    template: PathBuf,
    domain: String,
    title: String,
    description: String,
}

struct Page {
    path: String,
    content: String,
    template: String,
}

fn get_title(default_title: String) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| match args.get("path") {
        Some(path) => Ok(tera::to_value("foo")?),
        None => Ok(tera::to_value(&default_title)?),
    }
}

fn get_description(default_description: String) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| match args.get("path") {
        Some(path) => Ok(tera::to_value("foo")?),
        None => Ok(tera::to_value(&default_description)?),
    }
}

fn get_host(host: String) -> impl Function + 'static {
    move |_: &HashMap<String, Value>| Ok(tera::to_value(&host)?)
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let path = args.path.clone().join("config.yaml");
    let f = File::open(path)?;
    let config: Config = serde_yaml::from_reader(f)?;
    let template_path = args.path.join(&config.template).canonicalize()?;
    let posts = init_from_path(args.path.join(&config.content_path).canonicalize()?)?;

    let mut tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    tera.register_function("get_title", get_title(config.title.clone()));
    tera.register_function(
        "get_description",
        get_description(config.description.clone()),
    );
    tera.register_function("get_host", get_host(config.domain.clone()));
    let mut context = Context::new();
    context.insert("domain", &config.domain);
    let result = tera.render("index.html", &context)?;
    println!("{}", result);
    Ok(())
}
