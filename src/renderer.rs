use std::sync::Arc;

use rss::{ChannelBuilder, Item};
use tera::{Context, Tera};
use tracing::{debug, info};

use crate::{
    context::BuildContext,
    diagnostic::BarDiagnostic,
    fragment_services::FragmentServices,
    json_feed::{FeedItem, JsonFeedBuilder},
    render::{FragmentEngine, render_html},
    site::FeedType,
};

/// # Errors
/// Returns error if page rendering or feed generation fails.
///
/// # Panics
/// Panics if the `rendered_cache` mutex is poisoned.
#[allow(clippy::too_many_lines)]
pub fn render(
    ctx: &BuildContext,
    tera: &Tera,
    rendered_cache: &crate::render::RenderedContentCache,
) -> Result<(), BarDiagnostic> {
    info!("render dynamic pages and feeds");
    let mut feed_items: Vec<FeedItem> = vec![];
    let config = &ctx.config.config;
    let site = &ctx.site;
    let pages = &ctx.pages;

    let template_dir = ctx.config.path.join(&config.template);
    let template_dir = template_dir.canonicalize().map_err(|e| {
        BarDiagnostic::new(format!(
            "failed to canonicalize template path: {}",
            template_dir.display()
        ))
        .with_source(e.into())
    })?;

    let services = FragmentServices {
        site: site.clone(),
        config: Arc::new(config.clone()),
        project_path: Arc::new(ctx.config.path.clone()),
        pages: pages.clone(),
        syntax_set: ctx.syntax_set.clone(),
        rendered_cache: rendered_cache.clone(),
    };

    let engine = FragmentEngine::build(&template_dir, &ctx.theme, Some(&services))?;

    for pid in pages.keys() {
        if let Some(content_page) = pages.get(&pid) {
            let rendered = render_html(
                &content_page.ops,
                &content_page.source,
                &engine,
                &ctx.theme,
                &ctx.syntax_set,
                &pid,
            )
            .map_err(|e| {
                BarDiagnostic::new(format!("content rendering failed for \"{pid}\"")).with_source(e)
            })?;
            rendered_cache
                .lock()
                .expect("rendered cache poisoned")
                .insert(pid, rendered);
        }
    }

    while let Some(page) = site.next_unrendered_dynamic_page() {
        debug!("Rendering page: {}", page.path);
        let mut context = Context::new();
        context.insert("config", &config);
        context.insert("title", &page.title);
        context.insert("description", &page.description);
        context.insert("path", &page.path);
        context.insert("page_num", &page.page_num);
        let pid = page.path.trim_end_matches(".html");
        if let Some(rendered) = rendered_cache
            .lock()
            .expect("rendered cache poisoned")
            .get(pid)
        {
            context.insert("fragment_styles", &rendered.css);
            context.insert("rendered_body", &rendered.html);
        } else {
            context.insert("fragment_styles", "");
            context.insert("rendered_body", "");
        }
        let result = tera.render(&page.template, &context).map_err(|e| {
            let names = tera_error_names(&e);
            let inner: BarDiagnostic = e.into();
            let mut diag = BarDiagnostic::new(format!(
                "template rendering failed for \"{}\"",
                page.template
            ))
            .with_help(format!("while rendering page: {}", page.path));

            let template_path = template_dir.join(page.template.as_ref());
            if let Ok(content) = std::fs::read_to_string(&template_path) {
                diag = diag.with_source_code(page.template.to_string(), content.clone());
                for name in names.iter().take(5) {
                    if let Some(offset) = content.find(name.as_str()) {
                        diag = diag.with_label((offset, name.len()).into(), format!("'{name}'"));
                    }
                }
            }

            diag.with_source(inner)
        })?;
        site.set_page_content(&page.path, result.into());
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
                    language: config.language.clone(),
                }
                .build();

                feed.add_items(feed_items.clone());
                site.set_page_content(&page.path, feed.to_string().into());
            }
            FeedType::Atom => {
                let channel = ChannelBuilder::default()
                    .title(config.title.as_ref().to_string())
                    .link(config.domain.to_string())
                    .description(config.description.as_ref().to_string())
                    .language(Some(config.language.as_ref().into()))
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
                            .map(super::json_feed::FeedItem::to_rss_item)
                            .collect::<Vec<Item>>(),
                    )
                    .build();
                site.set_page_content(&page.path, channel.to_string().into());
            }
        }
    }
    info!("render dynamic pages and feeds complete");

    Ok(())
}

/// Walk a `tera::Error` chain and collect the structured names tera attaches
/// to each link — function, filter, test, template, and inheritance names.
/// `Msg`, `Json`, `Io`, and `Utf8Conversion` carry no structured name and are
/// skipped, so a template error that bottoms out in a `Msg` may produce zero
/// labels (the full message is still preserved on the source chain).
fn tera_error_names(err: &tera::Error) -> Vec<String> {
    use std::error::Error as _;

    let mut names: Vec<String> = Vec::new();
    let mut current: Option<&tera::Error> = Some(err);
    while let Some(e) = current {
        let candidate: Option<&str> = match &e.kind {
            tera::ErrorKind::CallFunction(name)
            | tera::ErrorKind::CallFilter(name)
            | tera::ErrorKind::CallTest(name)
            | tera::ErrorKind::FunctionNotFound(name)
            | tera::ErrorKind::FilterNotFound(name)
            | tera::ErrorKind::TestNotFound(name)
            | tera::ErrorKind::TemplateNotFound(name)
            | tera::ErrorKind::InvalidMacroDefinition(name) => Some(name.as_str()),
            tera::ErrorKind::MissingParent { parent, .. } => Some(parent.as_str()),
            tera::ErrorKind::CircularExtend { tpl, .. } => Some(tpl.as_str()),
            _ => None,
        };
        if let Some(name) = candidate
            && !names.iter().any(|n| n == name)
        {
            names.push(name.to_owned());
        }
        current = e
            .source()
            .and_then(|s| s.downcast_ref::<tera::Error>());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::tera_error_names;

    #[test]
    fn collects_call_function_name() {
        let err = tera::Error::call_function("get_image_url", tera::Error::msg("boom"));
        let names = tera_error_names(&err);
        assert_eq!(names, vec!["get_image_url".to_string()]);
    }

    #[test]
    fn walks_chain_and_dedupes() {
        let leaf = tera::Error::call_filter("upper", tera::Error::msg("inner"));
        let mid = tera::Error::call_function("get_image_url", leaf);
        let outer = tera::Error::call_function("get_image_url", mid);
        let names = tera_error_names(&outer);
        assert_eq!(
            names,
            vec!["get_image_url".to_string(), "upper".to_string()]
        );
    }

    #[test]
    fn ignores_msg_variant() {
        let err = tera::Error::msg("Variable 'foo' not found");
        let names = tera_error_names(&err);
        assert!(names.is_empty(), "got: {names:?}");
    }

    #[test]
    fn collects_template_not_found() {
        let err = tera::Error::template_not_found("missing.html");
        let names = tera_error_names(&err);
        assert_eq!(names, vec!["missing.html".to_string()]);
    }
}
