use std::{path::PathBuf, sync::Arc};

use syntect::parsing::SyntaxSet;
use tera::Tera;

use crate::{config::Config, pages::Pages, render::RenderedContentCache, site::Site};

pub struct FragmentServices {
    pub site: Arc<Site>,
    pub config: Arc<Config>,
    pub project_path: Arc<PathBuf>,
    pub pages: Arc<Pages>,
    pub syntax_set: Arc<SyntaxSet>,
    pub rendered_cache: RenderedContentCache,
}

impl FragmentServices {
    pub fn register(&self, tera: &mut Tera) {
        crate::templating::register_functions(
            tera,
            self.site.clone(),
            self.config.clone(),
            self.project_path.clone(),
            &self.pages,
            self.rendered_cache.clone(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GpxEmbeddingConfig, YamdProcessors};
    use crate::site::Site;
    use std::path::PathBuf;

    #[test]
    fn register_adds_functions_to_tera() {
        let site = Arc::new(Site::new(PathBuf::from("/tmp")));
        let config = Arc::new(Config {
            dist_path: PathBuf::from("./dist"),
            content_path: PathBuf::from("./content"),
            static_source_path: PathBuf::from("./public"),
            static_files_extensions: vec![],
            template: PathBuf::from("./template"),
            domain: Arc::from(url::Url::parse("https://test.com").unwrap()),
            title: Arc::from("test"),
            description: Arc::from("test"),
            language: Arc::from("en"),
            template_config: std::collections::HashMap::new(),
            yamd_processors: YamdProcessors {
                convert_cloudinary_embed: false,
                generate_alt_text: None,
            },
            gpx_embedding: GpxEmbeddingConfig::default(),
        });
        let pages = Arc::new(Pages::new());
        let syntax_set = crate::syntax_highlight::init().unwrap();
        let rendered_cache = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let services = FragmentServices {
            site,
            config,
            project_path: Arc::new(PathBuf::from("/tmp")),
            pages,
            syntax_set,
            rendered_cache,
        };
        let mut tera = tera::Tera::default();
        services.register(&mut tera);
        tera.add_raw_template("test", "{{ get_gpx_stats(input='/nonexistent.gpx') }}")
            .unwrap();
        let result = tera.render("test", &tera::Context::new());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            !err.contains("is not defined"),
            "function should be registered, got: {err}"
        );
    }
}
