use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub verbose: Verbosity,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(name = "build", about = "Build BAR project. [default]")]
    Build(BuildArgs),
    #[command(
        name = "article",
        about = "Create a new article in the current directory."
    )]
    Article(ArticleArgs),
}

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Path to the project directory.
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

#[derive(Parser, Debug)]
pub struct ArticleArgs {
    /// Title of the article will be used as the file name.
    #[clap()]
    pub title: String,
    /// By default BAR will fail if article with the same title already exists. Use this flag to
    /// overwrite exiting one.
    #[clap(short, long, action)]
    pub force: bool,
}
