use std::sync::Arc;

use cloudinary::transformation::{crop_mode::CropMode, gravity::Gravity, Image, Transformations};
use serde::Serialize;
use url::Url;

use crate::pages::Page;

#[derive(Serialize, Debug, Clone)]
pub struct JsonFeedItem {
    id: Arc<str>,
    title: Arc<str>,
    content_text: Arc<str>,
    url: Url,
    image: Option<Url>,
    date_published: Arc<str>,
    tags: Vec<Arc<str>>,
}

impl JsonFeedItem {
    pub fn new(page: &Page, base_url: &Url) -> Self {
        let mut url = base_url.clone();
        url.set_path(format!("{}.html", &page.pid).as_str());
        let image =
            page.get_image(base_url)
                .map(|src| match Image::try_from(src.clone()) {
                    Ok(image) => {
                        let result = image.clone().add_transformation(Transformations::Crop(
                            CropMode::Fill {
                                width: 800,
                                height: 600,
                                gravity: Some(Gravity::AutoClassic),
                            },
                        ));
                        result.build()
                    }
                    Err(_) => src,
                });

        Self {
            id: page.pid.clone(),
            title: page.get_title().into(),
            image,
            content_text: page
                .content
                .metadata
                .clone()
                .preview
                .unwrap_or("".into())
                .into(),

            url,
            date_published: page
                .content
                .metadata
                .date
                .unwrap()
                .format("%+")
                .to_string()
                .into(),
            tags: page
                .content
                .metadata
                .tags
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|tag| tag.into())
                .collect(),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct JsonFeed {
    version: Arc<str>,
    title: Arc<str>,
    home_page_url: Arc<Url>,
    feed_url: Url,
    icon: Option<Url>,
    favicon: Option<Url>,
    language: Arc<str>,
    items: Vec<JsonFeedItem>,
}

pub struct JsonFeedBuilder {
    pub title: Arc<str>,
    pub home_page_url: Arc<Url>,
    pub feed_url: Url,
    pub icon: Option<Url>,
    pub favicon: Option<Url>,
    pub language: Arc<str>,
}

impl JsonFeedBuilder {
    pub fn build(self) -> JsonFeed {
        JsonFeed::new(
            self.title,
            self.home_page_url,
            self.feed_url,
            self.icon,
            self.favicon,
            self.language,
        )
    }
}

impl JsonFeed {
    pub fn new(
        title: Arc<str>,
        home_page_url: Arc<Url>,
        feed_url: Url,
        icon: Option<Url>,
        favicon: Option<Url>,
        language: Arc<str>,
    ) -> Self {
        Self {
            version: "https://jsonfeed.org/version/1.1".into(),
            title,
            home_page_url,
            feed_url,
            icon,
            favicon,
            language,
            items: vec![],
        }
    }

    pub fn add_items(&mut self, mut items: Vec<JsonFeedItem>) {
        items.sort_by(|b, a| a.date_published.cmp(&b.date_published));
        self.items = items;
    }

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
