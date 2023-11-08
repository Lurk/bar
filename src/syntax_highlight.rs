use std::{collections::HashMap, sync::Arc};

use syntect::{
    dumps::from_uncompressed_data,
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use tera::{Function, Value};

use crate::{error::Errors, templating::get_arc_str_arg};

fn map_language(supported: &[Arc<str>], language: Arc<str>) -> Result<Arc<str>, tera::Error> {
    let language: Arc<str> = match language.to_lowercase().as_ref() {
        "js" | "javascript" => "JavaScript".into(),
        "ts" | "typescript" | "jsx" => "TypeScriptReact".into(),
        "rs" | "rust" => "Rust".into(),
        "bash" | "sh" => "Bourne Again Shell (bash)".into(),
        _ => language,
    };
    if supported.contains(&language) {
        Ok(language)
    } else {
        Err(tera::Error::from(format!(
            "Language {} is not supported\nSupported languages are {:?}",
            language, supported
        )))
    }
}

pub fn code(syntax_set: Arc<SyntaxSet>) -> impl Function + 'static {
    let supported = syntax_set
        .syntaxes()
        .iter()
        .map(|syntax| Arc::from(syntax.name.clone()))
        .collect::<Vec<Arc<str>>>();
    move |args: &HashMap<String, Value>| {
        let code = get_arc_str_arg(args, "code").unwrap();
        let language = map_language(&supported, get_arc_str_arg(args, "language").unwrap())?;
        let sr_rs = syntax_set.find_syntax_by_name(language.as_ref()).unwrap();
        let mut rs_html_generator =
            ClassedHTMLGenerator::new_with_class_style(sr_rs, &syntax_set, ClassStyle::Spaced);
        for line in LinesWithEndings::from(code.as_ref()) {
            rs_html_generator
                .parse_html_for_line_which_includes_newline(line)
                .unwrap();
        }
        let html_rs = rs_html_generator.finalize();
        Ok(tera::to_value(html_rs)?)
    }
}

pub fn init() -> Result<Arc<SyntaxSet>, Errors> {
    Ok(Arc::new(from_uncompressed_data(include_bytes!(
        "./syntaxes.bin"
    ))?))
}
