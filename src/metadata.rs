use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Default, Clone, Deserialize, Eq)]
pub struct Metadata {
    pub title: String,
    pub date: DateTime<FixedOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Arc<str>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_draft: Option<bool>,
}
