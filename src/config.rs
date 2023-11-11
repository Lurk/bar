use std::{collections::HashMap, fs::File, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{ContextExt, Errors};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub dist_path: PathBuf,
    pub content_path: PathBuf,
    pub template: PathBuf,
    pub domain: Arc<Url>,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub template_config: HashMap<Arc<str>, TemplateConfigValue>,
}

impl TryFrom<PathBuf> for Config {
    type Error = Errors;
    fn try_from(value: PathBuf) -> Result<Self, Errors> {
        let config_path = value.join("config.yaml");
        let f =
            File::open(&config_path).with_context(format!("config file: {:?}", &config_path))?;
        Ok(serde_yaml::from_reader(f)?)
    }
}

impl Config {
    pub fn get(&self, key: Arc<str>) -> Option<&TemplateConfigValue> {
        self.template_config.get(&key)
    }
}
