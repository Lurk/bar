use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use syntect::parsing::SyntaxSet;
use yamd::op::{Node, Op, OpKind};

use crate::diagnostic::BarDiagnostic;
use crate::theme::Theme;

mod context;
mod engine;

pub use engine::FragmentEngine;

use context::{build_fragment_context, html_escape, resolve_content, source_span_for_ops};
use engine::{collect_css, find_matching_end, fragment_key, fragment_template_name};

pub type RenderedContentCache = Arc<Mutex<HashMap<Arc<str>, RenderedContent>>>;

#[derive(Debug, Clone)]
pub struct RenderedContent {
    pub html: String,
    pub css: String,
}

#[derive(Clone, Copy)]
pub(super) struct RenderCtx<'a> {
    pub(super) theme: &'a Theme,
    pub(super) syntax_set: &'a SyntaxSet,
    pub(super) engine: &'a FragmentEngine,
    pub(super) source_name: &'a str,
}

pub(super) fn render_node(
    ops: &[Op],
    source: &str,
    node: &Node,
    start: usize,
    used_nodes: &mut HashSet<&'static str>,
    render_ctx: RenderCtx<'_>,
) -> Result<(String, usize), BarDiagnostic> {
    let engine = render_ctx.engine;
    let source_name = render_ctx.source_name;
    let key = fragment_key(node);
    let end = find_matching_end(ops, start, key);

    let wrap_with_yamd_context = |e: BarDiagnostic| -> BarDiagnostic {
        if let Some((offset, length)) = source_span_for_ops(ops, start, end) {
            BarDiagnostic::new(format!("error rendering '{key}' fragment"))
                .with_source_code(source_name.to_string(), source.to_string())
                .with_label(
                    (offset, length).into(),
                    format!("while rendering this {key}"),
                )
                .with_source(e)
        } else {
            e
        }
    };

    let ctx = build_fragment_context(ops, source, node, start, end, render_ctx, used_nodes)
        .map_err(&wrap_with_yamd_context)?;

    let template_name = fragment_template_name(key);
    let rendered = engine
        .tera
        .render(&template_name, &ctx)
        .map_err(|e| {
            let available: Vec<String> = ctx
                .clone()
                .into_json()
                .as_object()
                .map_or_else(Vec::new, |m| m.keys().cloned().collect());
            BarDiagnostic::new(format!("failed to render fragment template for '{key}'"))
                .with_help(format!("available variables: {}", available.join(", ")))
                .with_source(e.into())
        })
        .map_err(&wrap_with_yamd_context)?;

    used_nodes.insert(key);
    Ok((rendered, end + 1))
}

fn walk_ops(
    ops: &[Op],
    source: &str,
    render_ctx: RenderCtx<'_>,
    html: &mut String,
    used_nodes: &mut HashSet<&'static str>,
) -> Result<(), BarDiagnostic> {
    let mut i = 0;
    while i < ops.len() {
        match &ops[i].kind {
            OpKind::Start(node) => match node {
                Node::Document | Node::Title | Node::Destination | Node::Modifier => {
                    i += 1;
                    continue;
                }
                Node::Metadata => {
                    let end = find_matching_end(ops, i, "metadata");
                    i = end + 1;
                    continue;
                }
                _ => {
                    let (rendered, next_i) =
                        render_node(ops, source, node, i, used_nodes, render_ctx)?;
                    html.push_str(&rendered);
                    i = next_i;
                    continue;
                }
            },
            OpKind::End(_) => {
                i += 1;
                continue;
            }
            OpKind::Value => {
                let text = resolve_content(&ops[i].content, source);
                html.push_str(&html_escape(text));
            }
        }
        i += 1;
    }
    Ok(())
}

pub(super) fn render_ops_to_html(
    ops: &[Op],
    source: &str,
    render_ctx: RenderCtx<'_>,
    used_nodes: &mut HashSet<&'static str>,
) -> Result<String, BarDiagnostic> {
    let mut html = String::new();
    walk_ops(ops, source, render_ctx, &mut html, used_nodes)?;
    Ok(html)
}

/// Render yamd ops into HTML using a pre-built [`FragmentEngine`].
///
/// # Errors
/// Returns an error if any fragment template fails to render against its yamd input.
pub fn render_html(
    ops: &[Op],
    source: &str,
    engine: &FragmentEngine,
    theme: &Theme,
    syntax_set: &SyntaxSet,
    source_name: &str,
) -> Result<RenderedContent, BarDiagnostic> {
    let render_ctx = RenderCtx {
        theme,
        syntax_set,
        engine,
        source_name,
    };
    let mut used_nodes: HashSet<&'static str> = HashSet::new();
    let mut html = String::with_capacity(source.len() * 2);
    walk_ops(ops, source, render_ctx, &mut html, &mut used_nodes)?;
    let css = collect_css(engine, &used_nodes);
    Ok(RenderedContent { html, css })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use syntect::parsing::SyntaxSet;
    use yamd::op;

    use crate::syntax_highlight::init;
    use crate::theme::Theme;

    use super::{FragmentEngine, RenderedContent, render_html};

    const TEST_THEME_TOML: &str = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test theme"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = true
heading_anchors = true
"#;

    fn test_theme() -> Theme {
        Theme::parse(TEST_THEME_TOML).expect("test theme should parse")
    }

    fn test_syntax_set() -> Arc<SyntaxSet> {
        init().expect("syntax set should init")
    }

    fn render(source: &str) -> String {
        render_full(source).html
    }

    fn render_full(source: &str) -> RenderedContent {
        let ops = op::parse(source);
        let theme = test_theme();
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(Path::new("/tmp"), &theme, None).expect("engine");
        render_html(&ops, source, &engine, &theme, &ss, "test").expect("render should succeed")
    }

    #[test]
    fn renders_paragraph() {
        let html = render("hello world");
        assert!(html.contains("<p>hello world</p>"), "got: {html}");
    }

    #[test]
    fn renders_heading() {
        let html = render("# My Title");
        assert!(html.contains("<h1>"), "got: {html}");
        assert!(html.contains("</h1>"), "got: {html}");
        assert!(
            html.contains(r##"<a id="My Title" href="#My Title" class="second">#</a>"##),
            "heading should contain a clickable hash anchor with literal text id, got: {html}"
        );
        assert!(html.contains(">My Title</h1>"), "got: {html}");
    }

    #[test]
    fn renders_heading_with_anchor() {
        let html = render("# intro [link](/x) end");
        assert!(html.contains("<h1>"), "got: {html}");
        assert!(
            html.contains(r#"<a href="/x">link</a>"#),
            "anchor inside heading must be rendered, got: {html}"
        );
        assert!(
            !html.contains(">/x<") && !html.contains(" /x "),
            "anchor destination must not leak into heading text, got: {html}"
        );
        assert!(
            html.contains(
                r##"<a id="intro link end" href="#intro link end" class="second">#</a>"##
            ),
            "id and href should be the literal heading text (anchor label included), got: {html}"
        );
    }

    #[test]
    fn renders_heading_with_leading_anchor() {
        let html = render("# [link](/x) tail");
        assert!(
            html.contains(r#"<a href="/x">link</a>"#),
            "leading anchor must be rendered, got: {html}"
        );
        assert!(
            html.contains("tail</h1>"),
            "trailing text must follow the anchor, got: {html}"
        );
    }

    #[test]
    fn renders_heading_without_anchors() {
        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "# Title";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(Path::new("/tmp"), &theme, None).expect("engine");
        let html = render_html(&ops, source, &engine, &theme, &ss, "test")
            .expect("ok")
            .html;
        assert!(html.contains("<h1>Title</h1>"), "got: {html}");
        assert!(!html.contains("id="), "got: {html}");
    }

    #[test]
    fn renders_bold() {
        let html = render("**bold**");
        assert!(html.contains("<b>bold</b>"), "got: {html}");
    }

    #[test]
    fn renders_italic() {
        let html = render("_italic_");
        assert!(html.contains("<i>italic</i>"), "got: {html}");
    }

    #[test]
    fn renders_emphasis() {
        let html = render("*emphasis*");
        assert!(html.contains("<em>emphasis</em>"), "got: {html}");
    }

    #[test]
    fn renders_thematic_break() {
        let html = render("-----");
        assert!(html.contains("<hr/>"), "got: {html}");
    }

    #[test]
    fn renders_inline_code() {
        let html = render("`code`");
        assert!(html.contains("<code>code</code>"), "got: {html}");
    }

    #[test]
    fn renders_image() {
        let html = render("![alt](/photo.jpg)");
        assert!(html.contains("<img"), "got: {html}");
        assert!(html.contains("alt"), "got: {html}");
        assert!(html.contains("/photo.jpg"), "got: {html}");
    }

    #[test]
    fn renders_code_block() {
        let source = "```rust\nlet x = 1;\n```";
        let html = render(source);
        assert!(html.contains("<pre"), "got: {html}");
        assert!(html.contains("<code"), "got: {html}");
    }

    #[test]
    fn renders_unordered_list() {
        let html = render("- item one\n- item two");
        assert!(html.contains("<ul>"), "got: {html}");
        assert!(
            html.contains("<li>item one"),
            "list item should not be wrapped in <p>, got: {html}"
        );
        assert!(
            !html.contains("<li><p>"),
            "list item should not contain <p>, got: {html}"
        );
    }

    #[test]
    fn renders_anchor() {
        let html = render("[text](http://example.com)");
        assert!(html.contains("http://example.com"), "got: {html}");
        assert!(html.contains("text"), "got: {html}");
    }

    #[test]
    fn renders_strikethrough() {
        let html = render("~~strike~~");
        assert!(html.contains("<s>strike</s>"), "got: {html}");
    }

    #[test]
    fn renders_ordered_list() {
        let html = render("+ first\n+ second");
        assert!(html.contains("<ol>"), "got: {html}");
        assert!(
            html.contains("<li>first"),
            "list item should not be wrapped in <p>, got: {html}"
        );
        assert!(
            !html.contains("<li><p>"),
            "list item should not contain <p>, got: {html}"
        );
    }

    #[test]
    fn renders_highlight_with_title_and_icon() {
        let source = "!! title text\n! lightbulb\nparagraph\n!!";
        let html = render(source);
        assert!(
            html.contains("hicon"),
            "should have icon wrapper, got: {html}"
        );
        assert!(
            html.contains("class=\"body\""),
            "should have body wrapper, got: {html}"
        );
        assert!(
            html.contains("title text"),
            "should contain title, got: {html}"
        );
    }

    #[test]
    fn collapsible_id_is_slugified_from_title() {
        let html = render("{% Hello, World!\n\nbody\n%}");
        assert!(
            html.contains("<input type=\"checkbox\" id=\"hello--world-\""),
            "id should be slugified, got: {html}"
        );
        assert!(
            html.contains("<label for=\"hello--world-\">"),
            "for should match id, got: {html}"
        );
    }

    #[test]
    fn collapsible_with_no_alphanumeric_title_uses_position_fallback() {
        let html = render("{% !!!\n\nbody\n%}");
        assert!(
            !html.contains("id=\"\""),
            "empty id is invalid, got: {html}"
        );
        assert!(
            !html.contains("for=\"\""),
            "empty for is invalid, got: {html}"
        );
        assert!(
            html.contains("id=\"collapsible-"),
            "should fall back to positional id, got: {html}"
        );
    }

    #[test]
    fn renders_nested_collapsible() {
        let source = "{% outer\n\nbetween\n\n{% inner\n\ninside\n%}\n%}";
        let html = render(source);
        assert!(
            html.contains("class=\"collapsible\""),
            "should use collapsible class, got: {html}"
        );
        assert!(
            html.contains("<label for=\"outer\">"),
            "outer label, got: {html}"
        );
        assert!(
            html.contains("<label for=\"inner\">"),
            "inner label, got: {html}"
        );
        assert!(
            html.contains("between"),
            "content between collapsibles, got: {html}"
        );
        assert!(html.contains("inside"), "inner content, got: {html}");
    }

    #[test]
    fn renders_embed_youtube() {
        let html = render("{{youtube|https://www.youtube.com/embed/abc123}}");
        assert!(
            html.contains(r#"src="https://www.youtube.com/embed/abc123""#),
            "got: {html}"
        );
        assert!(html.contains(r#"class="youtube""#), "got: {html}");
        assert!(html.contains("allowfullscreen"), "got: {html}");
    }

    // The gallery is a :target slideshow: each slide div is hidden, the hash-targeted one
    // (or the first) is shown. The `picture` fragment wraps each slide image in its OWN
    // nested `.image` div, so the slide-selection rules MUST use direct-child combinators
    // (`.ig > .frame > .image > div`). A descendant rule like `.ig .image div { display: none }`
    // would also match the nested picture wrapper and hide the visible slide's image — the
    // exact regression a theme hit when this fragment changed shape.
    #[test]
    fn gallery_slideshow_css_uses_direct_child_combinator() {
        let css = include_str!("../defaults/fragments/images.css");
        assert!(
            css.contains(".ig > .frame > .image > div:target"),
            "slideshow must select slides via a direct-child `> div:target`: {css}"
        );
        assert!(
            !css.contains("\n.ig .image "),
            "a descendant `.ig .image ...` rule also matches nested picture wrappers and breaks slides: {css}"
        );
    }

    #[test]
    fn renders_images_gallery() {
        use crate::render::context::html_escape;
        use crate::render::engine::fragment_template_name;
        use std::collections::HashMap;
        use tera::{Context, Tera, Value};

        let mut tera = Tera::default();
        tera.set_escape_fn(html_escape);
        // images.html includes picture.html per slide, so both must be registered.
        let picture_name = fragment_template_name("picture");
        tera.add_raw_template(
            &picture_name,
            include_str!("../defaults/fragments/picture.html"),
        )
        .expect("picture template parses");
        let name = fragment_template_name("images");
        tera.add_raw_template(&name, include_str!("../defaults/fragments/images.html"))
            .expect("images template parses");
        tera.register_function("get_srcset", |_a: &HashMap<String, Value>| {
            Ok(Value::String(
                "/images/h-352x198.jpg 352w, /images/h-1008x567.jpg 1008w".to_owned(),
            ))
        });
        tera.register_function("get_image_url", |args: &HashMap<String, Value>| {
            let src = args
                .get("src")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            Ok(Value::String(src))
        });
        // crc32 is used in images.html to produce stable slide anchor ids.
        tera.register_filter("crc32", |value: &Value, _: &HashMap<String, Value>| {
            let s = value
                .as_str()
                .ok_or_else(|| tera::Error::msg("crc32 stub requires a string"))?;
            Ok(Value::String(s.to_owned()))
        });

        let images: Vec<serde_json::Value> = vec![
            serde_json::json!({"src": "/a.jpg", "alt": "image a"}),
            serde_json::json!({"src": "/b.jpg", "alt": "image b"}),
        ];

        let mut ctx = Context::new();
        ctx.insert("images", &images);
        ctx.insert("has_services", &true);
        ctx.insert("lazy_images", &true);
        ctx.insert("image_sizes", "(min-width: 1008px) 1008px, 100vw");

        let html = tera.render(&name, &ctx).expect("render");

        // Exactly one gallery-level fullscreen button (no per-slide leakage).
        assert_eq!(
            html.matches("class=\"fullscreen\"").count(),
            1,
            "expected exactly one gallery-level fullscreen button, got: {html}"
        );
        // CSS is not inlined into markup.
        assert!(
            !html.contains("scroll-snap"),
            "CSS must not appear in markup, got: {html}"
        );
        // 2 fixture images → 2 slide <img> (via picture include) + 2 thumbnail <img> = 4 total.
        // An exact count catches a regression where a slide drops its <img>.
        assert_eq!(html.matches("<img").count(), 4, "got: {html}");
        // Gallery container is present.
        assert!(html.contains("class=\"ig\""), "got: {html}");
    }

    #[test]
    fn escapes_html_in_text() {
        let html = render("<script>alert('xss')</script>");
        assert!(!html.contains("<script>"), "got: {html}");
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
    }

    #[test]
    fn escapes_single_quote_in_text() {
        let html = render("don't");
        assert!(!html.contains("don't"), "got: {html}");
        assert!(html.contains("don&#x27;t"), "got: {html}");
    }

    #[test]
    fn escapes_anchor_text_html() {
        let html = render("[<script>x</script>](http://e.com)");
        assert!(
            !html.contains("<script>x</script>"),
            "anchor text must not pass raw <script>, got: {html}"
        );
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
    }

    #[test]
    fn escapes_anchor_href_quotes() {
        let html = render(r#"[click](http://e.com"onclick=alert(1))"#);
        assert!(
            !html.contains(r#"http://e.com"onclick"#),
            "anchor href must escape attribute breakout, got: {html}"
        );
        assert!(html.contains("&quot;"), "got: {html}");
    }

    #[test]
    fn escapes_image_alt_html() {
        let html = render("![<script>x</script>](/p.jpg)");
        assert!(
            !html.contains("<script>x</script>"),
            "image alt must not pass raw <script>, got: {html}"
        );
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
    }

    #[test]
    fn escapes_embed_args_attribute() {
        let html = render(r#"{{custom|"><script>alert(1)</script>}}"#);
        assert!(
            !html.contains("<script>alert(1)</script>"),
            "embed args must escape attribute breakout, got: {html}"
        );
        assert!(html.contains("&quot;"), "got: {html}");
    }

    #[test]
    fn embed_unknown_kind_renders_as_iframe() {
        let html = render("{{iframe|/stages/?s=001}}");
        assert!(
            html.contains(r#"<iframe class="iframe""#),
            "unknown embed kind must render as iframe element, got: {html}"
        );
        assert!(
            html.contains(r#"src="/stages/?s=001""#),
            "iframe src must contain url, got: {html}"
        );
        assert!(
            !html.contains(r#"<div class="embed iframe""#),
            "must not fall back to empty div, got: {html}"
        );
    }

    #[test]
    fn escapes_icon_name() {
        let html = render("!! note\n! <script>alert(1)</script>\nbody\n!!");
        assert!(
            !html.contains("<script>alert(1)</script>"),
            "icon name must not pass raw <script>, got: {html}"
        );
    }

    #[test]
    fn embed_renders_gpx_icons_unescaped() {
        use std::collections::HashMap;
        use tera::{Context, Tera, Value};

        use crate::render::context::html_escape;
        use crate::render::engine::fragment_template_name;

        let mut tera = Tera::default();
        tera.set_escape_fn(html_escape);

        // embed.html includes picture.html for the GPX branch, so both must be registered.
        let picture_name = fragment_template_name("picture");
        tera.add_raw_template(
            &picture_name,
            include_str!("../defaults/fragments/picture.html"),
        )
        .expect("picture template should parse");
        let template_name = fragment_template_name("embed");
        tera.add_raw_template(
            &template_name,
            include_str!("../defaults/fragments/embed.html"),
        )
        .expect("embed template should parse");

        tera.register_function("render_gpx", |_args: &HashMap<String, Value>| {
            Ok(Value::String("/foo.png".to_owned()))
        });
        tera.register_function("get_gpx_srcset", |_args: &HashMap<String, Value>| {
            Ok(Value::String(
                "/foo-352w.png 352w, /foo-1008w.png 1008w".to_owned(),
            ))
        });
        tera.register_function("get_gpx_stats", |_args: &HashMap<String, Value>| {
            Ok(serde_json::json!({
                "total_ascent_m": 100,
                "distance_km": 25.0,
            }))
        });
        tera.register_function("add_static_file", |_args: &HashMap<String, Value>| {
            Ok(Value::String("/public/gpx/foo.gpx".to_owned()))
        });
        tera.register_filter("crc32", |value: &Value, _: &HashMap<String, Value>| {
            let s = value
                .as_str()
                .ok_or_else(|| tera::Error::msg("crc32 stub requires a string"))?;
            Ok(Value::String(s.to_owned()))
        });

        let mut ctx = Context::new();
        ctx.insert("kind", "gpx");
        ctx.insert("args", "/test.gpx");
        ctx.insert("has_services", &true);
        ctx.insert("lazy_images", &true);
        ctx.insert("image_sizes", "(min-width: 1008px) 1008px, 100vw");
        ctx.insert(
            "icon_elevation",
            "<span class=\"icon icon-elevation\">e</span>",
        );
        ctx.insert(
            "icon_distance",
            "<span class=\"icon icon-distance\">d</span>",
        );
        ctx.insert(
            "icon_download",
            "<span class=\"icon icon-download\">o</span>",
        );

        let html = tera
            .render(&template_name, &ctx)
            .expect("render should succeed");

        assert!(
            html.contains(r#"<span class="icon icon-elevation">e</span>"#),
            "icon_elevation must render raw, got: {html}"
        );
        assert!(
            html.contains(r#"<span class="icon icon-distance">d</span>"#),
            "icon_distance must render raw, got: {html}"
        );
        assert!(
            html.contains(r#"<span class="icon icon-download">o</span>"#),
            "icon_download must render raw, got: {html}"
        );
        assert!(
            !html.contains("&lt;span class=&quot;icon icon-"),
            "icon HTML must not be entity-escaped, got: {html}"
        );
    }

    #[test]
    fn embed_gpx_map_uses_picture_fragment() {
        use std::collections::HashMap;
        use tera::{Context, Tera, Value};

        use crate::render::context::html_escape;
        use crate::render::engine::fragment_template_name;

        let mut tera = Tera::default();
        tera.set_escape_fn(html_escape);
        // embed.html includes picture.html, so both must be registered.
        let picture_name = fragment_template_name("picture");
        tera.add_raw_template(
            &picture_name,
            include_str!("../defaults/fragments/picture.html"),
        )
        .expect("picture template should parse");
        let name = fragment_template_name("embed");
        tera.add_raw_template(&name, include_str!("../defaults/fragments/embed.html"))
            .expect("embed template should parse");
        tera.register_function("render_gpx", |_a: &HashMap<String, Value>| {
            Ok(Value::String("/foo.png".to_owned()))
        });
        tera.register_function("get_gpx_srcset", |_a: &HashMap<String, Value>| {
            Ok(Value::String(
                "/foo-352w.png 352w, /foo-1008w.png 1008w".to_owned(),
            ))
        });
        tera.register_function("get_gpx_stats", |_a: &HashMap<String, Value>| {
            Ok(serde_json::json!({ "total_ascent_m": 100, "distance_km": 25.0 }))
        });
        tera.register_function("add_static_file", |_a: &HashMap<String, Value>| {
            Ok(Value::String("/public/gpx/foo.gpx".to_owned()))
        });
        tera.register_filter("crc32", |v: &Value, _: &HashMap<String, Value>| {
            Ok(Value::String(v.as_str().unwrap_or_default().to_owned()))
        });

        let mut ctx = Context::new();
        ctx.insert("kind", "gpx");
        ctx.insert("args", "/test.gpx");
        ctx.insert("has_services", &true);
        ctx.insert("lazy_images", &true);
        ctx.insert("image_sizes", "(min-width: 1008px) 1008px, 100vw");
        ctx.insert("icon_elevation", "");
        ctx.insert("icon_distance", "");
        ctx.insert("icon_download", "");

        let html = tera.render(&name, &ctx).expect("render should succeed");
        // The GPX branch now delegates to the shared picture fragment, which
        // wraps the image in its own <div class="image">. The outer gpx-map div
        // lets embed.css target .gpx-map .image for sizing.
        assert!(html.contains("srcset="), "got: {html}");
        assert!(html.contains("requestFullscreen()"), "got: {html}");
        assert!(!html.contains("<picture"), "got: {html}");
    }

    #[test]
    fn fragment_override_for_image() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        std::fs::write(
            fragments_dir.join("image.html"),
            "<figure><img src=\"{{ src }}\" alt=\"{{ alt }}\"/></figure>",
        )
        .expect("write template");

        std::fs::write(fragments_dir.join("image.css"), ".figure { margin: 0; }\n")
            .expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test theme"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = true
heading_anchors = true
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "![alt text](/photo.jpg)";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test").expect("render");
        assert!(result.html.contains("<figure>"), "got: {}", result.html);
        assert!(result.html.contains("alt text"), "got: {}", result.html);
    }

    #[test]
    fn fragment_override_for_heading() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        std::fs::write(
            fragments_dir.join("heading.html"),
            "<h{{ level }} class=\"custom\" id=\"{{ anchor_id }}\">{{ text }}</h{{ level }}>",
        )
        .expect("write template");

        std::fs::write(fragments_dir.join("heading.css"), "").expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = true
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "# Hello";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test").expect("render");
        assert!(
            result.html.contains("class=\"custom\""),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn fragment_override_missing_template_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");
        // A syntactically broken template triggers an error during build. Tera
        // catches it during the initial glob load before the per-fragment pass.
        std::fs::write(
            fragments_dir.join("image.html"),
            "{% if unclosed_block %}oops",
        )
        .expect("write broken template");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let err = FragmentEngine::build(dir.path(), &theme, None)
            .err()
            .expect("engine build should fail with broken override template");
        let rendered = format!("{err:?}");
        // The glob pre-loads every *.html in the fragments dir, so a
        // syntactically broken override is caught at load time (Tera::new),
        // before the per-fragment convention branch runs.
        assert!(
            rendered.contains("failed to load fragment templates from"),
            "got: {rendered}"
        );
    }

    #[test]
    fn fragment_override_for_paragraph_uses_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        std::fs::write(
            fragments_dir.join("paragraph.html"),
            "<div class=\"para\">{{ content }}</div>",
        )
        .expect("write template");

        std::fs::write(fragments_dir.join("paragraph.css"), "").expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "hello world";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test").expect("render");
        assert!(
            result.html.contains("class=\"para\""),
            "got: {}",
            result.html
        );
        assert!(result.html.contains("hello world"), "got: {}", result.html);
    }

    #[test]
    fn css_not_empty_for_heading() {
        let result = render_full("# Title");
        assert!(!result.css.is_empty(), "CSS should not be empty");
        assert!(
            result.css.contains("margin-top"),
            "heading default CSS: {}",
            result.css
        );
    }

    #[test]
    fn css_uses_fragment_override() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        std::fs::write(
            fragments_dir.join("image.html"),
            "<img src=\"{{ src | safe }}\" alt=\"{{ alt }}\"/>",
        )
        .expect("write template");

        let custom_css = ".custom-image { border: 2px solid red; }\n";
        std::fs::write(fragments_dir.join("image.css"), custom_css).expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test theme"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = true
heading_anchors = true
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "![alt](/photo.jpg)";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test").expect("render");
        assert!(
            result.css.contains(".custom-image"),
            "CSS should include fragment override, got: {}",
            result.css
        );
    }

    #[test]
    fn renders_full_document() {
        let source = "# Hello World\n\nThis is a **paragraph** with *emphasis* and a [link](http://example.com).\n\n![An image](/photo.jpg)\n\n```rust\nfn main() {}\n```\n\n- list item one\n- list item two\n\n-----";
        let html = render(source);
        assert!(html.contains("<h1"), "heading");
        assert!(html.contains("<p>"), "paragraph");
        assert!(html.contains("<b>"), "bold");
        assert!(html.contains("<em>"), "emphasis");
        assert!(html.contains("<a href"), "anchor");
        assert!(html.contains("<img"), "image");
        assert!(html.contains("<pre"), "code block");
        assert!(html.contains("<ul>"), "list");
        assert!(html.contains("<hr/>"), "thematic break");
    }

    #[test]
    fn render_error_includes_yamd_source_context() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        // Template references a variable that doesn't exist — triggers render error
        std::fs::write(
            fragments_dir.join("paragraph.html"),
            "{{ undefined_var | length }}",
        )
        .expect("write template");

        std::fs::write(fragments_dir.join("paragraph.css"), "").expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "hello world";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test.yamd");
        assert!(result.is_err(), "should fail on bad template");

        let err = result.unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("test.yamd"),
            "error should reference yamd source file, got: {rendered}"
        );
        assert!(
            rendered.contains("hello world"),
            "error should include yamd source snippet, got: {rendered}"
        );
    }

    #[test]
    fn render_error_for_failing_embed_shows_location_chain_and_snippet() {
        use std::collections::HashMap;
        use tera::Value;

        use crate::diagnostic::BarDiagnostic;

        let dir = tempfile::tempdir().expect("tempdir");
        let theme = test_theme();
        let ss = test_syntax_set();
        let mut engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        // The default embed template only takes the gpx branch when has_services
        // is true, so flip it on for this test.
        engine.has_services = true;

        // Stub render_gpx to return a chained Tera error mimicking the real
        // shape: a CallFunction error wrapping a deeper IO-style message.
        engine
            .tera
            .register_function("render_gpx", |_args: &HashMap<String, Value>| {
                Err(tera::Error::call_function(
                    "render_gpx",
                    tera::Error::msg("No such file or directory (os error 2): /bad/path.gpx"),
                ))
            });

        let source = "intro paragraph\n\n{{gpx|/bad/path.gpx}}\n\ntrailing text";
        let ops = yamd::op::parse(source);

        // Mimic renderer.rs:64 — render_html's error gets wrapped in an outer
        // "content rendering failed for ..." diagnostic before reaching miette.
        let inner = render_html(
            &ops,
            source,
            &engine,
            &theme,
            &ss,
            "content/post/sample.yamd",
        )
        .expect_err("render should fail when render_gpx errors");
        let outer =
            BarDiagnostic::new("content rendering failed for \"/post/sample\"").with_source(inner);

        let rendered = format!("{outer:?}");

        assert!(
            rendered.contains("content/post/sample.yamd"),
            "snippet header should include the yamd source name, got:\n{rendered}"
        );
        assert!(
            rendered.contains("{{gpx|"),
            "rendered output should include the offending embed snippet, got:\n{rendered}"
        );
        assert!(
            rendered.contains("/bad/path.gpx"),
            "cause chain should preserve the bad path string, got:\n{rendered}"
        );
        assert!(
            rendered.contains("No such file") || rendered.contains("not found"),
            "cause chain should preserve the underlying IO message, got:\n{rendered}"
        );
    }

    #[test]
    fn tera_error_chain_is_preserved_on_into_bar_diagnostic() {
        use std::error::Error as _;

        use crate::diagnostic::BarDiagnostic;

        let leaf = tera::Error::msg("deepest cause");
        let mid = tera::Error::call_function("render_gpx", leaf);
        let top = tera::Error::call_function("__bar_fragment__embed.html", mid);

        let diag: BarDiagnostic = top.into();

        let mut messages = vec![diag.to_string()];
        let mut current: Option<&dyn std::error::Error> = diag.source();
        while let Some(e) = current {
            messages.push(e.to_string());
            current = e.source();
        }

        assert!(
            messages.iter().any(|m| m.contains("deepest cause")),
            "expected deepest message in chain, got: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("render_gpx")),
            "expected mid-chain function name, got: {messages:?}"
        );
    }

    #[test]
    fn css_includes_all_used_node_defaults() {
        let source = "# Title\n\nhello\n\n-----";
        let ops = op::parse(source);
        let theme = test_theme();
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(Path::new("/tmp"), &theme, None).expect("engine");
        let result = render_html(&ops, source, &engine, &theme, &ss, "test").unwrap();
        assert!(result.css.contains("h1"), "heading css: {}", result.css);
        assert!(
            result.css.contains("hr"),
            "thematic break css: {}",
            result.css
        );
    }

    #[test]
    fn css_includes_nested_node_defaults_inside_collapsible() {
        let result = render_full("{% wrapper\n\n# nested heading\n\n%}");
        assert!(
            result.css.contains("margin-top"),
            "heading css must be collected when nested in a collapsible: {}",
            result.css
        );
    }

    fn render_with_anchor_override(source: &str, marker_css: &str) -> RenderedContent {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");
        std::fs::write(
            fragments_dir.join("anchor.html"),
            "<a href=\"{{ href }}\">{{ text }}</a>",
        )
        .expect("write anchor template");
        std::fs::write(fragments_dir.join("anchor.css"), marker_css).expect("write anchor css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        render_html(&ops, source, &engine, &theme, &ss, "test").expect("render")
    }

    #[test]
    fn css_includes_overridden_anchor_inside_list_item() {
        let marker = ".my-anchor { color: red; }";
        let result = render_with_anchor_override("- [link](/x)", marker);
        assert!(
            result.css.contains(".my-anchor"),
            "anchor css must be collected when nested in a list item: {}",
            result.css
        );
    }

    #[test]
    fn css_includes_overridden_anchor_inside_highlight() {
        let marker = ".my-anchor { color: red; }";
        let result = render_with_anchor_override("!! note\n! lightbulb\n[link](/x)\n!!", marker);
        assert!(
            result.css.contains(".my-anchor"),
            "anchor css must be collected when nested in a highlight: {}",
            result.css
        );
    }

    #[test]
    fn docs_match_default_fragments() {
        let docs = std::fs::read_to_string("docs/templating/fragments.md")
            .expect("fragments.md should exist");

        let fragment_files = [
            "image",
            "code",
            "heading",
            "anchor",
            "embed",
            "collapsible",
            "highlight",
            "images",
            "paragraph",
            "unordered_list",
            "ordered_list",
            "list_item",
            "thematic_break",
            "icon",
            "picture",
        ];

        for name in fragment_files {
            let css = std::fs::read_to_string(format!("src/defaults/fragments/{name}.css"))
                .unwrap_or_default();
            let css = css.trim();
            if !css.is_empty() {
                assert!(
                    docs.contains(css),
                    "docs/templating/fragments.md is out of sync with src/defaults/fragments/{name}.css\n\
                     Expected to find:\n{css}"
                );
            }
        }
    }

    #[test]
    fn image_context_exposes_sizes() {
        // The non-services <img> path ignores image_sizes, so assert via a
        // fragment override that echoes it.
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");
        std::fs::write(
            fragments_dir.join("image.html"),
            "<img data-sizes=\"{{ image_sizes }}\" src=\"{{ src }}\">",
        )
        .expect("write template");
        std::fs::write(fragments_dir.join("image.css"), "").expect("write css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "![alt](/photo.jpg)";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");
        let html = render_html(&ops, source, &engine, &theme, &ss, "test")
            .expect("render")
            .html;
        assert!(
            html.contains("data-sizes=\"(display-mode: fullscreen) 100vw, (min-width:"),
            "got: {html}"
        );
    }

    #[test]
    fn image_fragment_uses_srcset_with_services() {
        use crate::render::context::html_escape;
        use crate::render::engine::fragment_template_name;
        use std::collections::HashMap;
        use tera::{Context, Tera, Value};

        let mut tera = Tera::default();
        tera.set_escape_fn(html_escape);
        // image.html includes picture, so register both.
        let picture_name = fragment_template_name("picture");
        tera.add_raw_template(
            &picture_name,
            include_str!("../defaults/fragments/picture.html"),
        )
        .expect("picture template parses");
        let name = fragment_template_name("image");
        tera.add_raw_template(&name, include_str!("../defaults/fragments/image.html"))
            .expect("image template parses");
        tera.register_function("get_srcset", |_a: &HashMap<String, Value>| {
            Ok(Value::String(
                "/images/h-352x198.jpg 352w, /images/h-1008x567.jpg 1008w".to_owned(),
            ))
        });

        let mut ctx = Context::new();
        ctx.insert("src", "/photo.jpg");
        ctx.insert("alt", "a cat");
        ctx.insert("has_services", &true);
        ctx.insert("lazy_images", &true);
        ctx.insert("image_sizes", "(min-width: 1008px) 1008px, 100vw");

        let html = tera.render(&name, &ctx).expect("render");
        assert!(
            html.contains("srcset=\"/images/h-352x198.jpg 352w"),
            "got: {html}"
        );
        assert!(html.contains("sizes=\""), "got: {html}");
        assert!(html.contains("requestFullscreen()"), "got: {html}");
    }

    #[test]
    fn image_fragment_plain_img_without_services() {
        use crate::render::context::html_escape;
        use crate::render::engine::fragment_template_name;
        use tera::{Context, Tera};

        let mut tera = Tera::default();
        tera.set_escape_fn(html_escape);
        // image.html includes picture, so register both.
        let picture_name = fragment_template_name("picture");
        tera.add_raw_template(
            &picture_name,
            include_str!("../defaults/fragments/picture.html"),
        )
        .expect("picture template parses");
        let name = fragment_template_name("image");
        tera.add_raw_template(&name, include_str!("../defaults/fragments/image.html"))
            .expect("image template parses");
        // No get_srcset registered: must take the else branch (no-services path).

        let mut ctx = Context::new();
        ctx.insert("src", "/photo.jpg");
        ctx.insert("alt", "a cat");
        ctx.insert("has_services", &false);
        ctx.insert("lazy_images", &false);
        ctx.insert("image_sizes", "(min-width: 1008px) 1008px, 100vw");
        let html = tera.render(&name, &ctx).expect("render");
        assert!(html.contains(r#"<img src="/photo.jpg""#), "got: {html}");
        assert!(!html.contains("srcset"), "got: {html}");
        assert!(!html.contains("fullscreen"), "got: {html}");
    }

    #[test]
    fn engine_build_propagates_template_parse_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("broken.html"), "{% if missing_endif %}oops")
            .expect("write broken template");

        let theme = test_theme();
        let err = FragmentEngine::build(dir.path(), &theme, None)
            .err()
            .expect("engine build should fail when a globbed template has a syntax error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("broken.html") || rendered.contains("template"),
            "error should surface the underlying tera failure, got: {rendered}"
        );
    }

    #[test]
    fn render_propagates_icon_template_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fragments_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&fragments_dir).expect("mkdir");

        std::fs::write(
            fragments_dir.join("icon.html"),
            "{{ undefined_var | length }}",
        )
        .expect("write icon template");
        std::fs::write(fragments_dir.join("icon.css"), "").expect("write icon css");

        let theme_toml = r#"
[theme]
name = "test"
version = "1.0.0"
description = "Test"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(theme_toml).expect("parse");
        let source = "{% inner\n\nbody\n%}";
        let ops = op::parse(source);
        let ss = test_syntax_set();
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        let result = render_html(&ops, source, &engine, &theme, &ss, "test.yamd");
        assert!(
            result.is_err(),
            "render should propagate icon render error, got: {:?}",
            result.map(|r| r.html)
        );
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("icon"),
            "error should mention the icon fragment, got: {err}"
        );
    }

    const MINIMAL_THEME_TOML: &str = r#"
[theme]
name = "t"
version = "1.0.0"
description = "d"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = true
heading_anchors = true
"#;

    #[test]
    fn picture_renders_and_image_pulls_its_css() {
        use crate::render::engine::{collect_css, fragment_template_name};

        let dir = tempfile::tempdir().expect("tempdir");
        let theme = Theme::parse(MINIMAL_THEME_TOML).expect("parse");
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        let mut c = tera::Context::new();
        c.insert("has_services", &true);
        c.insert("src", "/a.jpg");
        c.insert("srcset", "/a.jpg 352w");
        c.insert("alt", "x");
        c.insert("image_sizes", "100vw");
        c.insert("lazy_images", &true);
        let html = engine
            .tera
            .render(&fragment_template_name("picture"), &c)
            .expect("render");
        assert!(html.contains("class=\"fullscreen\""), "got: {html}");
        assert!(html.contains("requestFullscreen()"), "got: {html}");

        let mut used = std::collections::HashSet::new();
        used.insert("Image");
        let css = collect_css(&engine, &used);
        assert!(
            css.contains(".image .fullscreen"),
            "picture css missing: {css}"
        );
    }

    #[test]
    fn picture_fallback_src_is_smallest_variant() {
        use crate::render::engine::fragment_template_name;

        let dir = tempfile::tempdir().expect("tempdir");
        let theme = Theme::parse(MINIMAL_THEME_TOML).expect("parse");
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        let mut c = tera::Context::new();
        c.insert("has_services", &true);
        // The raw original must never be the fallback src: it is not published.
        c.insert("src", "/photo.jpg");
        c.insert(
            "srcset",
            "/images/h-352x198.jpg 352w, /images/h-1008x567.jpg 1008w",
        );
        c.insert("alt", "x");
        c.insert("image_sizes", "100vw");
        c.insert("lazy_images", &true);
        let html = engine
            .tera
            .render(&fragment_template_name("picture"), &c)
            .expect("render");
        // Fallback src is the smallest (first) srcset candidate, a published variant.
        assert!(
            html.contains(r#"src="/images/h-352x198.jpg""#),
            "got: {html}"
        );
        assert!(
            !html.contains(r#"src="/photo.jpg""#),
            "raw unpublished original must not be the fallback src: {html}"
        );
    }

    #[test]
    fn picture_passthrough_srcset_is_usable_fallback() {
        use crate::render::engine::fragment_template_name;

        let dir = tempfile::tempdir().expect("tempdir");
        let theme = Theme::parse(MINIMAL_THEME_TOML).expect("parse");
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        let mut c = tera::Context::new();
        c.insert("has_services", &true);
        c.insert("src", "/icon.svg");
        // Passthrough sources (svg, non-cloudinary remote) yield a single
        // descriptor-less candidate; the fallback must still resolve to it.
        c.insert("srcset", "/icon.svg");
        c.insert("alt", "x");
        c.insert("image_sizes", "100vw");
        c.insert("lazy_images", &false);
        let html = engine
            .tera
            .render(&fragment_template_name("picture"), &c)
            .expect("render");
        assert!(html.contains(r#"src="/icon.svg""#), "got: {html}");
    }

    #[test]
    fn gallery_slides_carry_lazy_loading_when_enabled() {
        use std::collections::HashMap;
        use tera::Value;

        // Build an engine with stub service functions so has_services=true and
        // images.html takes the slider branch (the path where lazy_images must
        // be forwarded from the Node::Images context arm).
        let dir = tempfile::tempdir().expect("tempdir");
        let theme = test_theme(); // lazy_images = true
        let ss = test_syntax_set();
        let mut engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        // Promote to has_services so images.html renders slides via picture.
        engine.has_services = true;
        engine
            .tera
            .register_function("get_srcset", |_: &HashMap<String, Value>| {
                Ok(Value::String(
                    "/a-352w.jpg 352w, /a-1008w.jpg 1008w".to_owned(),
                ))
            });
        engine
            .tera
            .register_function("get_image_url", |args: &HashMap<String, Value>| {
                let src = args
                    .get("src")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                Ok(Value::String(src))
            });
        engine
            .tera
            .register_filter("crc32", |v: &Value, _: &HashMap<String, Value>| {
                Ok(Value::String(v.as_str().unwrap_or_default().to_owned()))
            });

        // Two images on consecutive lines → yamd parses them as Node::Images.
        let source = "![img a](/a.jpg)\n![img b](/b.jpg)";
        let ops = yamd::op::parse(source);
        let html = render_html(&ops, source, &engine, &theme, &ss, "test")
            .expect("render should succeed")
            .html;

        // The two slide <img> elements must carry loading="lazy" when
        // theme.render.lazy_images is true. The thumbnail row always hardcodes
        // loading="lazy" on its <img> tags, so with 2 images we expect at least
        // 4 occurrences total (2 slides + 2 thumbnails). Without the fix the
        // Node::Images context arm omits lazy_images, picture.html receives no
        // value for it, and the slide <img> tags skip the attribute — leaving
        // only 2 occurrences (thumbnails only).
        let lazy_count = html.matches(r#"loading="lazy""#).count();
        assert!(
            lazy_count >= 4,
            "expected >=4 lazy attrs (2 slides + 2 thumbnails) but got {lazy_count}; \
             gallery slides must carry loading=\"lazy\" when lazy_images=true, got: {html}"
        );
        assert!(
            html.contains("class=\"ig\""),
            "gallery container missing, got: {html}"
        );
    }

    #[test]
    fn fragment_override_resolved_by_convention() {
        use crate::render::engine::fragment_template_name;

        let dir = tempfile::tempdir().expect("tempdir");
        let frag_dir = dir.path().join("fragments");
        std::fs::create_dir_all(&frag_dir).unwrap();
        std::fs::write(
            frag_dir.join("paragraph.html"),
            "<p class=\"custom\">{{ content | safe }}</p>",
        )
        .unwrap();
        std::fs::write(frag_dir.join("paragraph.css"), ".custom{color:red}").unwrap();

        let theme = Theme::parse(MINIMAL_THEME_TOML).expect("parse");
        let engine = FragmentEngine::build(dir.path(), &theme, None).expect("engine");

        assert_eq!(
            engine.css.get("paragraph").map(String::as_str),
            Some(".custom{color:red}")
        );
        let rendered = engine
            .tera
            .render(&fragment_template_name("paragraph"), &{
                let mut c = tera::Context::new();
                c.insert("content", "hi");
                c
            })
            .expect("render");
        assert!(rendered.contains("class=\"custom\""), "got: {rendered}");
    }
}
