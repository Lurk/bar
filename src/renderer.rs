use std::sync::Arc;

use rss::{ChannelBuilder, Item};
use tera::{Context, Tera};
use tracing::{debug, info};

use crate::{
    error::BarErr,
    json_feed::{FeedItem, JsonFeedBuilder},
    pages::Pages,
    site::{FeedType, Site},
    CONFIG,
};

pub fn render(site: Arc<Site>, tera: &Tera, pages: &Pages) -> Result<(), BarErr> {
    info!("render dynamic pages and feeds");
    let mut feed_items: Vec<FeedItem> = vec![];
    let config = CONFIG.get().expect("Config to be initialized");

    while let Some(page) = site.next_unrendered_dynamic_page() {
        debug!("Rendering page: {}", page.path);
        let mut context = Context::new();
        context.insert("config", &config);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        context.insert("page_num", &page.page_num);
        let result = tera.render(&page.template, &context)?;
        site.set_page_content(page.path.clone(), result.into());
        if let Some(page) = pages.get(page.path.trim_end_matches(".html")) {
            feed_items.push(FeedItem::new(page, config.domain.as_ref()));
        }
    }

    feed_items.sort_by(|b, a| a.date_published.cmp(&b.date_published));

    while let Some(page) = site.next_unrendered_feed() {
        let icon = if site.get_page("/icon.png").is_some() {
            Some(config.domain.join("/icon.png")?)
        } else {
            None
        };

        let favicon = if site.get_page("/favicon.ico").is_some() {
            Some(config.domain.join("/favicon.ico")?)
        } else {
            None
        };

        match page.typ {
            FeedType::Json => {
                let feed_url = config.domain.join(&page.path)?;
                let mut feed = JsonFeedBuilder {
                    title: config.title.clone(),
                    home_page_url: config.domain.clone(),
                    feed_url,
                    icon,
                    favicon,
                    language: Arc::from("en"), // TODO: make this configurable
                }
                .build();

                feed.add_items(feed_items.clone());
                site.set_page_content(page.path.clone(), feed.to_string().into());
            }
            FeedType::Atom => {
                let channel = ChannelBuilder::default()
                    .title(config.title.as_ref().to_string())
                    .link(config.domain.to_string())
                    .description(config.description.as_ref().to_string())
                    .language(Some("en".into())) // TODO: make this configurable
                    .image(icon.map(|url| rss::Image {
                        url: url.to_string(),
                        title: config.title.as_ref().to_string(),
                        link: config.domain.to_string(),
                        width: None,
                        height: None,
                        description: None,
                    }))
                    .items(
                        feed_items
                            .iter()
                            .map(|item| item.to_rss_item())
                            .collect::<Vec<Item>>(),
                    )
                    .build();
                site.set_page_content(page.path.clone(), channel.to_string().into());
            }
        }
    }
    info!("render dynamic pages and feeds complete");

    Ok(())
}
