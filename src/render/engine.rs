use std::collections::{HashMap, HashSet};
use std::path::Path;

use yamd::op::{Node, Op, OpKind};

use crate::diagnostic::BarDiagnostic;
use crate::theme::Theme;

const FRAGMENT_DEFAULTS: &[(&str, &str, &str)] = &[
    (
        "anchor",
        include_str!("../defaults/fragments/anchor.html"),
        include_str!("../defaults/fragments/anchor.css"),
    ),
    (
        "bold",
        include_str!("../defaults/fragments/bold.html"),
        include_str!("../defaults/fragments/bold.css"),
    ),
    (
        "code",
        include_str!("../defaults/fragments/code.html"),
        include_str!("../defaults/fragments/code.css"),
    ),
    (
        "code_span",
        include_str!("../defaults/fragments/code_span.html"),
        include_str!("../defaults/fragments/code_span.css"),
    ),
    (
        "collapsible",
        include_str!("../defaults/fragments/collapsible.html"),
        include_str!("../defaults/fragments/collapsible.css"),
    ),
    (
        "embed",
        include_str!("../defaults/fragments/embed.html"),
        include_str!("../defaults/fragments/embed.css"),
    ),
    (
        "emphasis",
        include_str!("../defaults/fragments/emphasis.html"),
        include_str!("../defaults/fragments/emphasis.css"),
    ),
    (
        "heading",
        include_str!("../defaults/fragments/heading.html"),
        include_str!("../defaults/fragments/heading.css"),
    ),
    (
        "highlight",
        include_str!("../defaults/fragments/highlight.html"),
        include_str!("../defaults/fragments/highlight.css"),
    ),
    (
        "icon",
        include_str!("../defaults/fragments/icon.html"),
        include_str!("../defaults/fragments/icon.css"),
    ),
    (
        "image",
        include_str!("../defaults/fragments/image.html"),
        include_str!("../defaults/fragments/image.css"),
    ),
    (
        "images",
        include_str!("../defaults/fragments/images.html"),
        include_str!("../defaults/fragments/images.css"),
    ),
    (
        "italic",
        include_str!("../defaults/fragments/italic.html"),
        include_str!("../defaults/fragments/italic.css"),
    ),
    (
        "list_item",
        include_str!("../defaults/fragments/list_item.html"),
        include_str!("../defaults/fragments/list_item.css"),
    ),
    (
        "ordered_list",
        include_str!("../defaults/fragments/ordered_list.html"),
        include_str!("../defaults/fragments/ordered_list.css"),
    ),
    (
        "paragraph",
        include_str!("../defaults/fragments/paragraph.html"),
        include_str!("../defaults/fragments/paragraph.css"),
    ),
    (
        "strikethrough",
        include_str!("../defaults/fragments/strikethrough.html"),
        include_str!("../defaults/fragments/strikethrough.css"),
    ),
    (
        "thematic_break",
        include_str!("../defaults/fragments/thematic_break.html"),
        include_str!("../defaults/fragments/thematic_break.css"),
    ),
    (
        "unordered_list",
        include_str!("../defaults/fragments/unordered_list.html"),
        include_str!("../defaults/fragments/unordered_list.css"),
    ),
];

const FRAGMENT_TEMPLATE_PREFIX: &str = "__bar_fragment__";

pub(super) fn fragment_template_name(key: &str) -> String {
    format!("{FRAGMENT_TEMPLATE_PREFIX}{key}")
}

pub(super) fn fragment_key(node: &Node) -> &'static str {
    match node {
        Node::Anchor => "anchor",
        Node::Bold => "bold",
        Node::Code => "code",
        Node::CodeSpan => "code_span",
        Node::Collapsible => "collapsible",
        Node::Destination => "destination",
        Node::Document => "document",
        Node::Embed => "embed",
        Node::Emphasis => "emphasis",
        Node::Heading => "heading",
        Node::Highlight => "highlight",
        Node::Icon => "icon",
        Node::Image => "image",
        Node::Images => "images",
        Node::Italic => "italic",
        Node::ListItem => "list_item",
        Node::Metadata => "metadata",
        Node::Modifier => "modifier",
        Node::OrderedList => "ordered_list",
        Node::Paragraph => "paragraph",
        Node::Strikethrough => "strikethrough",
        Node::ThematicBreak => "thematic_break",
        Node::Title => "title",
        Node::UnorderedList => "unordered_list",
    }
}

pub(super) fn find_matching_end(ops: &[Op], start: usize, node_key: &str) -> usize {
    let mut depth = 1u32;
    let mut i = start + 1;
    while i < ops.len() {
        match &ops[i].kind {
            OpKind::Start(n) if fragment_key(n) == node_key => depth += 1,
            OpKind::End(n) if fragment_key(n) == node_key => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    ops.len() - 1
}

pub(super) fn collect_css(engine: &FragmentEngine, used_nodes: &HashSet<&str>) -> String {
    let mut css = String::new();
    for node_name in used_nodes {
        let node_key = node_name.to_lowercase();
        if let Some(content) = engine.css.get(node_key.as_str()) {
            css.push_str(content);
            if !content.is_empty() && !content.ends_with('\n') {
                css.push('\n');
            }
        }
    }
    css
}

/// Pre-built fragment renderer. Builds Tera once with every fragment template
/// (defaults plus theme overrides) registered under stable names, and loads
/// every fragment's CSS into memory. Render calls reuse this engine instead
/// of touching disk per page.
pub struct FragmentEngine {
    pub(super) tera: tera::Tera,
    pub(super) css: HashMap<String, String>,
    pub(super) has_services: bool,
}

impl FragmentEngine {
    /// # Errors
    /// Returns an error if a theme-declared fragment template or CSS file
    /// cannot be read, or if any fragment template fails to parse.
    pub fn build(
        template_dir: &Path,
        theme: &Theme,
        services: Option<&crate::fragment_services::FragmentServices>,
    ) -> Result<Self, BarDiagnostic> {
        let glob = template_dir.join("**").join("*.html").display().to_string();
        let mut tera = tera::Tera::new(&glob).map_err(|e| {
            BarDiagnostic::new(format!("failed to load fragment templates from {glob}"))
                .with_source(BarDiagnostic::new(e.to_string()))
        })?;
        if let Some(svc) = services {
            svc.register(&mut tera);
        }

        let mut css: HashMap<String, String> = HashMap::new();
        for &(key, default_template, default_css) in FRAGMENT_DEFAULTS {
            let name = fragment_template_name(key);

            if let Some(fragment) = theme.render.fragments.get(key) {
                let template_path = template_dir.join(&fragment.template);
                let template_content = std::fs::read_to_string(&template_path).map_err(|e| {
                    BarDiagnostic::new(format!("failed to read fragment template for '{key}'"))
                        .with_help(format!("expected file at: {}", template_path.display()))
                        .with_source(e.into())
                })?;
                tera.add_raw_template(&name, &template_content)
                    .map_err(|e| {
                        BarDiagnostic::new(format!("syntax error in fragment template for '{key}'"))
                            .with_source_code(template_path.display().to_string(), template_content)
                            .with_source(BarDiagnostic::new(e.to_string()))
                    })?;

                let css_path = template_dir.join(&fragment.css);
                let css_content = std::fs::read_to_string(&css_path).map_err(|e| {
                    BarDiagnostic::new(format!("failed to read fragment css for '{key}'"))
                        .with_help(format!("expected file at: {}", css_path.display()))
                        .with_source(e.into())
                })?;
                css.insert(key.to_string(), css_content);
            } else {
                tera.add_raw_template(&name, default_template)
                    .map_err(|e| {
                        BarDiagnostic::new(format!(
                            "syntax error in default fragment template for '{key}'"
                        ))
                        .with_source(BarDiagnostic::new(e.to_string()))
                    })?;
                css.insert(key.to_string(), default_css.to_string());
            }
        }

        Ok(Self {
            tera,
            css,
            has_services: services.is_some(),
        })
    }
}
