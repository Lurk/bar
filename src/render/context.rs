use std::collections::HashSet;

use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use yamd::op::{Content, Node, Op, OpKind};

use crate::diagnostic::BarDiagnostic;

use super::engine::{FragmentEngine, find_matching_end, fragment_template_name};
use super::{RenderCtx, render_node, render_ops_to_html};

pub(super) fn resolve_content<'a>(content: &'a Content, source: &'a str) -> &'a str {
    match content {
        Content::Span(range) => &source[range.clone()],
        Content::Materialized(s) => s.as_str(),
    }
}

pub(super) fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

fn heading_level(start_content: &str) -> u8 {
    let hashes = start_content.chars().take_while(|&c| c == '#').count();
    u8::try_from(hashes).unwrap_or(6).clamp(1, 6)
}

fn map_language(language: &str) -> String {
    match language.to_lowercase().as_str() {
        "js" | "javascript" => "JavaScript".to_owned(),
        "ts" | "typescript" | "tsx" | "jsx" => "TypeScriptReact".to_owned(),
        "rs" | "rust" => "Rust".to_owned(),
        "bash" | "sh" => "Bourne Again Shell (bash)".to_owned(),
        "yaml" | "yml" => "YAML".to_owned(),
        "md" | "yamd" => "Markdown".to_owned(),
        other => other.to_owned(),
    }
}

fn highlight_code(code: &str, language: &str, syntax_set: &SyntaxSet) -> String {
    let mapped = map_language(language);
    let syntax = syntax_set
        .find_syntax_by_name(&mapped)
        .or_else(|| syntax_set.find_syntax_by_extension(language))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut generator =
        ClassedHTMLGenerator::new_with_class_style(syntax, syntax_set, ClassStyle::Spaced);
    for line in LinesWithEndings::from(code) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .expect("syntect line parse should not fail");
    }
    generator.finalize()
}

pub(super) fn source_span_for_ops(ops: &[Op], start: usize, end: usize) -> Option<(usize, usize)> {
    let start_offset = match &ops[start].content {
        Content::Span(range) => range.start,
        Content::Materialized(_) => return None,
    };
    let end_offset = match &ops[end].content {
        Content::Span(range) => range.end,
        Content::Materialized(_) => return None,
    };
    Some((start_offset, end_offset.saturating_sub(start_offset)))
}

fn extract_inner_text(ops: &[Op], source: &str, start: usize, end: usize) -> String {
    let mut text = String::new();
    for op in &ops[start + 1..end] {
        if op.kind == OpKind::Value {
            text.push_str(resolve_content(&op.content, source));
        }
    }
    text
}

/// Render the `icon` fragment for `name`. Errors are surfaced rather than
/// swallowed so call sites can decide whether to propagate or fall back.
pub(super) fn render_icon(engine: &FragmentEngine, name: &str) -> Result<String, BarDiagnostic> {
    let mut icon_ctx = tera::Context::new();
    icon_ctx.insert("name", name);
    let template = fragment_template_name("icon");
    engine.tera.render(&template, &icon_ctx).map_err(|e| {
        BarDiagnostic::new(format!("failed to render icon '{name}'"))
            .with_source(BarDiagnostic::new(e.to_string()))
    })
}

#[allow(clippy::too_many_lines)]
pub(super) fn build_fragment_context(
    ops: &[Op],
    source: &str,
    node: &Node,
    start: usize,
    end: usize,
    render_ctx: RenderCtx<'_>,
    used_nodes: &mut HashSet<&'static str>,
) -> Result<tera::Context, BarDiagnostic> {
    let RenderCtx {
        theme,
        syntax_set,
        engine,
        ..
    } = render_ctx;
    let has_services = engine.has_services;
    let mut ctx = tera::Context::new();

    match node {
        Node::Image => {
            let mut alt = String::new();
            let mut src = String::new();
            let mut in_title = false;
            let mut in_dest = false;
            for op in &ops[start + 1..end] {
                match &op.kind {
                    OpKind::Start(Node::Title) => in_title = true,
                    OpKind::End(Node::Title) => in_title = false,
                    OpKind::Start(Node::Destination) => in_dest = true,
                    OpKind::End(Node::Destination) => in_dest = false,
                    OpKind::Value => {
                        let text = resolve_content(&op.content, source);
                        if in_title {
                            alt.push_str(text);
                        } else if in_dest {
                            src.push_str(text);
                        }
                    }
                    _ => {}
                }
            }
            ctx.insert("src", &src);
            ctx.insert("alt", &alt);
            ctx.insert("lazy_images", &theme.render.lazy_images);
            ctx.insert("has_services", &has_services);
        }
        Node::Code => {
            let mut language = String::new();
            let mut content = String::new();
            let mut in_modifier = false;
            for op in &ops[start + 1..end] {
                match &op.kind {
                    OpKind::Start(Node::Modifier) => in_modifier = true,
                    OpKind::End(Node::Modifier) => in_modifier = false,
                    OpKind::Value => {
                        let text = resolve_content(&op.content, source);
                        if in_modifier {
                            language.push_str(text);
                        } else {
                            content.push_str(text);
                        }
                    }
                    _ => {}
                }
            }
            let highlighted = highlight_code(&content, &language, syntax_set);
            ctx.insert("language", &language);
            ctx.insert("content", &content);
            ctx.insert("highlighted", &highlighted);
            ctx.insert(
                "code_class",
                &theme.render.code_class.as_deref().unwrap_or("code"),
            );
        }
        Node::Heading => {
            let text_content = resolve_content(&ops[start].content, source);
            let level = heading_level(text_content);
            let text = extract_inner_text(ops, source, start, end);
            let slug = slugify(&text);
            ctx.insert("level", &level);
            ctx.insert("text", &text);
            ctx.insert("anchor_id", &slug);
            ctx.insert("heading_anchors", &theme.render.heading_anchors);
        }
        Node::Anchor => {
            let mut title = String::new();
            let mut dest = String::new();
            let mut in_title = false;
            let mut in_dest = false;
            for op in &ops[start + 1..end] {
                match &op.kind {
                    OpKind::Start(Node::Title) => in_title = true,
                    OpKind::End(Node::Title) => in_title = false,
                    OpKind::Start(Node::Destination) => in_dest = true,
                    OpKind::End(Node::Destination) => in_dest = false,
                    OpKind::Value => {
                        let text = resolve_content(&op.content, source);
                        if in_title {
                            title.push_str(text);
                        } else if in_dest {
                            dest.push_str(text);
                        }
                    }
                    _ => {}
                }
            }
            ctx.insert("href", &dest);
            ctx.insert("text", &title);
        }
        Node::Embed => {
            let mut values: Vec<String> = Vec::new();
            for op in &ops[start + 1..end] {
                if op.kind == OpKind::Value {
                    values.push(resolve_content(&op.content, source).to_owned());
                }
            }
            let kind = values.first().map_or("", String::as_str);
            let args = values.get(2).map_or("", String::as_str);
            ctx.insert("kind", kind);
            ctx.insert("args", args);
            ctx.insert("has_services", &has_services);

            if kind == "gpx" {
                for (var_name, icon_name) in [
                    ("icon_elevation", "lower-right-triangle"),
                    ("icon_distance", "distance"),
                    ("icon_download", "download"),
                ] {
                    let html = render_icon(engine, icon_name)?;
                    ctx.insert(var_name, &html);
                }
            }
        }
        Node::Icon => {
            let name = extract_inner_text(ops, source, start, end);
            ctx.insert("name", &name);
        }
        Node::Collapsible => {
            let inner_ops = &ops[start + 1..end];
            let mut title = String::new();
            let mut pos = 0;

            if pos < inner_ops.len() && matches!(inner_ops[pos].kind, OpKind::Start(Node::Modifier))
            {
                let mod_end = find_matching_end(inner_ops, pos, "modifier");
                title = extract_inner_text(inner_ops, source, pos, mod_end);
                pos = mod_end + 1;
            }

            let slug = slugify(&title);
            let id = if slug.chars().any(char::is_alphanumeric) {
                slug
            } else {
                format!("collapsible-{start}")
            };

            let toggle_icon = render_icon(engine, "play")?;

            let body_html = render_ops_to_html(&inner_ops[pos..], source, render_ctx, used_nodes)?;
            ctx.insert("id", &id);
            ctx.insert("title", &title);
            ctx.insert("toggle_icon", &toggle_icon);
            ctx.insert("content", &body_html);
        }
        Node::Highlight => {
            let inner_ops = &ops[start + 1..end];
            let mut icon_html = String::new();
            let mut title = String::new();
            let mut pos = 0;

            if pos < inner_ops.len() && matches!(inner_ops[pos].kind, OpKind::Start(Node::Modifier))
            {
                let mod_end = find_matching_end(inner_ops, pos, "modifier");
                title = extract_inner_text(inner_ops, source, pos, mod_end);
                pos = mod_end + 1;
            }

            if pos < inner_ops.len() && matches!(inner_ops[pos].kind, OpKind::Start(Node::Icon)) {
                let icon_end = find_matching_end(inner_ops, pos, "icon");
                let icon_name = extract_inner_text(inner_ops, source, pos, icon_end);
                icon_html = render_icon(engine, &icon_name)?;
                pos = icon_end + 1;
            }

            let body_html = render_ops_to_html(&inner_ops[pos..], source, render_ctx, used_nodes)?;
            ctx.insert("icon", &icon_html);
            ctx.insert("title", &title);
            ctx.insert("content", &body_html);
        }
        Node::Images => {
            let inner = render_ops_to_html(&ops[start + 1..end], source, render_ctx, used_nodes)?;
            ctx.insert("content", &inner);

            let mut images: Vec<serde_json::Value> = Vec::new();
            let inner_ops = &ops[start + 1..end];
            let mut j = 0;
            while j < inner_ops.len() {
                if matches!(inner_ops[j].kind, OpKind::Start(Node::Image)) {
                    let img_end = find_matching_end(inner_ops, j, "image");
                    let mut src = String::new();
                    let mut alt = String::new();
                    let mut in_title = false;
                    let mut in_dest = false;
                    for op in &inner_ops[j + 1..img_end] {
                        match &op.kind {
                            OpKind::Start(Node::Title) => in_title = true,
                            OpKind::End(Node::Title) => in_title = false,
                            OpKind::Start(Node::Destination) => in_dest = true,
                            OpKind::End(Node::Destination) => in_dest = false,
                            OpKind::Value => {
                                let text = resolve_content(&op.content, source);
                                if in_title {
                                    alt.push_str(text);
                                } else if in_dest {
                                    src.push_str(text);
                                }
                            }
                            _ => {}
                        }
                    }
                    images.push(serde_json::json!({"src": src, "alt": alt}));
                    j = img_end + 1;
                } else {
                    j += 1;
                }
            }
            ctx.insert("images", &images);
            ctx.insert("has_services", &has_services);
        }
        Node::ListItem => {
            let inner_ops = &ops[start + 1..end];
            let mut html = String::new();
            let mut j = 0;
            while j < inner_ops.len() {
                match &inner_ops[j].kind {
                    OpKind::Start(Node::Paragraph) | OpKind::End(_) => {
                        j += 1;
                    }
                    OpKind::Start(inner_node) => {
                        let (rendered, next_j) =
                            render_node(inner_ops, source, inner_node, j, used_nodes, render_ctx)?;
                        html.push_str(&rendered);
                        j = next_j;
                    }
                    OpKind::Value => {
                        html.push_str(&html_escape(resolve_content(&inner_ops[j].content, source)));
                        j += 1;
                    }
                }
            }
            ctx.insert("content", &html);
        }
        _ => {
            let inner = render_ops_to_html(&ops[start + 1..end], source, render_ctx, used_nodes)?;
            ctx.insert("content", &inner);
        }
    }

    Ok(ctx)
}
