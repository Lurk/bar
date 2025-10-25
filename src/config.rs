use std::{collections::HashMap, fs::File, path::PathBuf, sync::Arc};

use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use url::Url;

use crate::error::{BarErr, ContextExt};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateConfigValue {
    VecOfStrings(Vec<String>),
    MapOfStringToString(LinkedHashMap<String, String>),
    MapOfStringToMapOfStringToString(LinkedHashMap<String, LinkedHashMap<String, String>>),
    String(String),
    Bool(bool),
    Usize(usize),
}

fn default_extension() -> Vec<String> {
    vec![
        "css".to_string(),
        "js".to_string(),
        "png".to_string(),
        "jpg".to_string(),
        "jpeg".to_string(),
        "gif".to_string(),
        "svg".to_string(),
        "webmanifest".to_string(),
        "ico".to_string(),
        "txt".to_string(),
    ]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YamdProcessors {
    /// converts cloudinary embed to image gallery
    #[serde(default)]
    pub convert_cloudinary_embed: bool,
    /// generate alt text for images
    #[serde(default)]
    pub generate_alt_text: Option<AltTextGenerator>,
}

fn default_prompt() -> Arc<str> {
    "Describe this image in detail".to_string().into()
}

fn default_temperature() -> f64 {
    0.1
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AltTextGenerator {
    /// Prompt for the alt text generator.
    /// Default: "Describe this image in detail".to_string()
    #[serde(default = "default_prompt")]
    pub prompt: Arc<str>,
    /// The temperature for the alt text generator.
    /// Default: 0.1
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GpxEmbeddingConfig {
    /// Base URLs for tile servers.
    /// Example: ["https://a.tile.openstreetmap.org", "https://b.tile.openstreetmap.org"]
    /// Default: ["https://tile.openstreetmap.org"]
    /// The URLs should support {z}/{x}/{y} pattern.
    pub base: Vec<Arc<str>>,
    /// Optional copyright notice to be displayed on the map.
    /// Path to png.
    /// Default: None
    pub attribution_png: Option<PathBuf>,
}

impl Default for GpxEmbeddingConfig {
    fn default() -> Self {
        GpxEmbeddingConfig {
            base: vec![Arc::from("https://tile.openstreetmap.org")],
            attribution_png: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub dist_path: PathBuf,
    pub content_path: PathBuf,
    /// The path to the directory with static files. Relative to the config file.
    pub static_source_path: PathBuf,
    /// White list of file extensions to be copied to the dist directory.
    /// The extensions should not include the dot.
    /// Default: ["css", "js", "png", "jpg", "jpeg", "gif", "svg", "webmanifest", "ico", "txt"]
    #[serde(default = "default_extension")]
    pub static_files_extensions: Vec<String>,
    pub template: PathBuf,
    pub domain: Arc<Url>,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub template_config: HashMap<Arc<str>, TemplateConfigValue>,
    /// pre render yamd transformations
    pub yamd_processors: YamdProcessors,
    /// gpx embedding configuration
    #[serde(default)]
    pub gpx_embedding: GpxEmbeddingConfig,
}

impl TryFrom<&PathBuf> for Config {
    type Error = BarErr;
    fn try_from(value: &PathBuf) -> Result<Self, BarErr> {
        let config_path = value.join("config.yaml");
        info!("initializing config");
        debug!("reading config at: {:?}", config_path);
        let f =
            File::open(&config_path).with_context(|| format!("config file: {:?}", &config_path))?;
        Ok(serde_yaml::from_reader(f)?)
    }
}
