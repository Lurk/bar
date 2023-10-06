pub mod error;
pub mod posts;

use clap::Parser;
use error::Errors;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use tera::{Context, Tera};

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
}

struct Page {
    path: String,
    content: String,
    template: String,
}

fn main() -> Result<(), Errors> {
    let args = Args::parse();
    let path = args.path.clone().join("config.yaml");
    let f = File::open(path)?;
    let config: Config = serde_yaml::from_reader(f)?;
    let template_path = args.path.join(config.template).canonicalize()?;
    println!("{:?}", template_path);
    let posts = init_from_path(args.path.join(config.content_path).canonicalize()?)?;

    let tera = Tera::new(format!("{}/**/*.html", template_path.to_str().unwrap()).as_str())?;
    let mut context = Context::new();
    let result = tera.render("index.html", &context)?;
    println!("{}", result);
    Ok(())
}
