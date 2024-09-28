use std::{
    fmt::{Display, Formatter},
    sync::Arc,
};

use cloudinary::transformation::{crop_mode::CropMode, gravity::Gravity, Image, Transformations};
use rss::{Category, Guid, Item, ItemBuilder};
use serde::Serialize;
use url::Url;

use crate::pages::Page;

#[derive(Serialize, Debug, Clone)]
pub struct FeedItem {
    id: Arc<str>,
    title: Arc<str>,
    content_text: Arc<str>,
    url: Url,
    image: Option<Url>,
    pub date_published: Arc<str>,
    tags: Vec<Arc<str>>,
}

impl FeedItem {
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
            content_text: page.metadata.preview.clone().unwrap_or("".into()).into(),

            url,
            date_published: page.metadata.date.format("%+").to_string().into(),
            tags: page
                .metadata
                .tags
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|tag| tag.into())
                .collect(),
        }
    }

    pub fn to_rss_item(&self) -> Item {
        ItemBuilder::default()
            .title(Some(self.title.as_ref().into()))
            .link(Some(self.url.clone().into()))
            .description(Some(self.content_text.as_ref().into()))
            .pub_date(Some(self.date_published.as_ref().into()))
            .guid(Some(Guid {
                value: self.url.clone().into(),
                permalink: true,
            }))
            .categories(
                self.tags
                    .iter()
                    .map(|tag| Category {
                        name: tag.as_ref().into(),
                        domain: None,
                    })
                    .collect::<Vec<Category>>(),
            )
            .build()
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
    items: Vec<FeedItem>,
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

    pub fn add_items(&mut self, items: Vec<FeedItem>) {
        self.items = items;
    }
}

impl Display for JsonFeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}
