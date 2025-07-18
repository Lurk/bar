use std::{collections::HashMap, fs::File, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use url::Url;

use crate::error::{BarErr, ContextExt};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateConfigValue {
    VecOfStrings(Vec<String>),
    String(String),
    Bool(bool),
    Usize(usize),
}

impl TemplateConfigValue {
    pub fn as_vec_of_strings(&self) -> Option<&Vec<String>> {
        match self {
            TemplateConfigValue::VecOfStrings(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&String> {
        match self {
            TemplateConfigValue::String(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        match self {
            TemplateConfigValue::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_usize(&self) -> Option<&usize> {
        match self {
            TemplateConfigValue::Usize(v) => Some(v),
            _ => None,
        }
    }
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

impl Config {
    pub fn get(&self, key: Arc<str>) -> Option<&TemplateConfigValue> {
        self.template_config.get(&key)
    }
}
