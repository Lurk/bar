use std::path::Path;

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
    pub image: ImageConfig,
}

const DEFAULT_IMAGE_SIZES: &str =
    "(display-mode: fullscreen) 100vw, (min-width: 1008px) 1008px, 100vw";
const DEFAULT_WIDTHS: [usize; 6] = [352, 704, 1008, 1568, 2016, 3840];

#[derive(Debug, Deserialize, Default)]
pub struct ImageConfig {
    pub sizes: Option<String>,
    pub widths: Option<Vec<usize>>,
}

impl ImageConfig {
    /// The responsive `sizes` attribute, falling back to the content-column default.
    #[must_use]
    pub fn sizes(&self) -> &str {
        self.sizes.as_deref().unwrap_or(DEFAULT_IMAGE_SIZES)
    }

    /// The srcset candidate widths, falling back to the default ladder.
    #[must_use]
    pub fn widths(&self) -> Vec<usize> {
        self.widths
            .clone()
            .unwrap_or_else(|| DEFAULT_WIDTHS.to_vec())
    }
}

impl Theme {
    /// # Errors
    /// Returns an error if `content` is not valid TOML or is missing required fields.
    pub fn parse(content: &str) -> Result<Self, BarDiagnostic> {
        let theme: Theme = toml::from_str(content)?;
        Ok(theme)
    }

    /// # Errors
    /// Returns an error if `bar_version` is incompatible with the theme's version requirement.
    pub fn validate(&self, bar_version: &str, _template_dir: &Path) -> Result<(), BarDiagnostic> {
        let req = VersionReq::parse(&self.theme.compatible_bar_versions)?;
        let version = Version::parse(bar_version)?;

        if !req.matches(&version) {
            return Err(BarDiagnostic::new(format!(
                "theme '{}' is incompatible with bar {bar_version}: requires {}",
                self.theme.name, self.theme.compatible_bar_versions
            )));
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

    #[test]
    fn image_config_defaults_when_absent() {
        let theme = Theme::parse(MINIMAL_THEME).expect("should parse");
        assert_eq!(
            theme.render.image.sizes(),
            "(display-mode: fullscreen) 100vw, (min-width: 1008px) 1008px, 100vw"
        );
        assert_eq!(
            theme.render.image.widths(),
            vec![352, 704, 1008, 1568, 2016, 3840]
        );
    }

    #[test]
    fn image_config_overrides() {
        let content = r#"
[theme]
name = "x"
version = "1.0.0"
description = "d"
compatible_bar_versions = ">=0.1.0"
tags = []

[render]
lazy_images = true
heading_anchors = false

[render.image]
sizes = "100vw"
widths = [100, 200]
"#;
        let theme = Theme::parse(content).expect("should parse");
        assert_eq!(theme.render.image.sizes(), "100vw");
        assert_eq!(theme.render.image.widths(), vec![100, 200]);
    }
}
