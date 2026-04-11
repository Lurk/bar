use std::{path::PathBuf, sync::Arc};

use syntect::parsing::SyntaxSet;

use crate::{config::Config, pages::Pages, site::Site};

pub struct BuildConfig {
    pub path: PathBuf,
    pub config: Config,
}

pub struct BuildContext {
    pub config: BuildConfig,
    pub pages: Arc<Pages>,
    pub site: Arc<Site>,
    pub syntax_set: Arc<SyntaxSet>,
}
