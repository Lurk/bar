use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::diagnostic::BarDiagnostic;

#[derive(Debug, Deserialize)]
pub struct Theme {
    pub theme: ThemeMeta,
    pub render: RenderConfig,
}

#[derive(Debug, Deserialize)]
pub struct ThemeMeta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub compatible_bar_versions: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RenderConfig {
    pub lazy_images: bool,
    pub heading_anchors: bool,
    pub code_class: Option<String>,
    #[serde(default)]
    pub fragments: HashMap<String, FragmentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct FragmentConfig {
    pub template: PathBuf,
    pub css: PathBuf,
}

impl Theme {
    /// # Errors
    /// Returns an error if `content` is not valid TOML or is missing required fields.
    pub fn parse(content: &str) -> Result<Self, BarDiagnostic> {
        let theme: Theme = toml::from_str(content)?;
        Ok(theme)
    }

    /// # Errors
    /// Returns an error if `bar_version` is incompatible with the theme's version requirement,
    /// or if any declared fragment template or CSS file does not exist under `template_dir`.
    pub fn validate(&self, bar_version: &str, template_dir: &Path) -> Result<(), BarDiagnostic> {
        let req = VersionReq::parse(&self.theme.compatible_bar_versions)?;
        let version = Version::parse(bar_version)?;

        if !req.matches(&version) {
            return Err(BarDiagnostic::new(format!(
                "theme '{}' is incompatible with bar {bar_version}: requires {}",
                self.theme.name, self.theme.compatible_bar_versions
            )));
        }

        for (name, fragment) in &self.render.fragments {
            let template_path = template_dir.join(&fragment.template);
            if !template_path.exists() {
                return Err(
                    BarDiagnostic::new(format!("fragment '{name}' template not found"))
                        .with_help(format!("expected file at: {}", template_path.display())),
                );
            }

            let css_path = template_dir.join(&fragment.css);
            if !css_path.exists() {
                return Err(
                    BarDiagnostic::new(format!("fragment '{name}' css not found"))
                        .with_help(format!("expected file at: {}", css_path.display())),
                );
            }
        }

        Ok(())
    }

    /// # Errors
    /// Returns an error if the file cannot be read or contains invalid TOML.
    pub fn load(path: &Path) -> Result<Self, BarDiagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            BarDiagnostic::new(format!("failed to read theme config: {}", path.display()))
                .with_source(e.into())
        })?;

        let theme: Theme = toml::from_str(&content).map_err(|e| {
            let mut diag = BarDiagnostic::new("invalid theme configuration")
                .with_source_code(path.display().to_string(), content.clone());
            if let Some(span) = e.span() {
                diag = diag.with_label(span.into(), e.message().to_string());
            }
            diag
        })?;

        Ok(theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_THEME: &str = r#"
[theme]
name = "my-theme"
version = "1.0.0"
description = "A test theme"
compatible_bar_versions = ">=0.1.0"
tags = ["blog", "minimal"]

[render]
lazy_images = true
heading_anchors = false
"#;

    #[test]
    fn parse_minimal_theme() {
        let theme = Theme::parse(MINIMAL_THEME).expect("should parse");
        assert_eq!(theme.theme.name, "my-theme");
        assert_eq!(theme.theme.version, "1.0.0");
        assert_eq!(theme.theme.description, "A test theme");
        assert_eq!(theme.theme.compatible_bar_versions, ">=0.1.0");
        assert_eq!(theme.theme.tags, vec!["blog", "minimal"]);
        assert!(theme.render.lazy_images);
        assert!(!theme.render.heading_anchors);
        assert!(theme.render.code_class.is_none());
        assert!(theme.render.fragments.is_empty());
    }

    #[test]
    fn parse_theme_with_fragments() {
        let content = r#"
[theme]
name = "fancy"
version = "2.0.0"
description = "Fancy theme"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = false
heading_anchors = true
code_class = "highlight"

[render.fragments.gallery]
template = "fragments/gallery.html"
css = "fragments/gallery.css"
"#;
        let theme = Theme::parse(content).expect("should parse");
        assert_eq!(theme.render.code_class.as_deref(), Some("highlight"));
        let gallery = theme
            .render
            .fragments
            .get("gallery")
            .expect("gallery fragment");
        assert_eq!(gallery.template, PathBuf::from("fragments/gallery.html"));
        assert_eq!(gallery.css, PathBuf::from("fragments/gallery.css"));
    }

    #[test]
    fn parse_invalid_toml_returns_error() {
        let result = Theme::parse("this is not valid toml ][");
        assert!(result.is_err());
    }

    #[test]
    fn validate_incompatible_version() {
        let content = r#"
[theme]
name = "future-theme"
version = "1.0.0"
description = "Requires far-future bar"
compatible_bar_versions = ">=99.0.0"
tags = []

[render]
lazy_images = false
heading_anchors = false
"#;
        let theme = Theme::parse(content).expect("should parse");
        let result = theme.validate("0.1.0", Path::new("/tmp"));
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("incompatible"),
            "error message: {err}"
        );
    }

    #[test]
    fn validate_compatible_version() {
        let theme = Theme::parse(MINIMAL_THEME).expect("should parse");
        let result = theme.validate("0.1.0", Path::new("/tmp"));
        assert!(result.is_ok());
    }

    #[test]
    fn load_invalid_toml_shows_source() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("bar_test_invalid_theme");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "[theme]\nname = ").unwrap();

        let result = Theme::load(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("theme.toml") || msg.contains("invalid"),
            "got: {msg}"
        );

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_from_path_missing_file() {
        let result = Theme::load(Path::new("/nonexistent/path/theme.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("failed to read theme config"),
            "error message: {err}"
        );
    }
}
