pub mod error;

use syntect::{
    dumps::from_uncompressed_data,
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

use crate::error::Error;

pub struct Highlighted {
    pub html: String,
    /// `false` when the language was not found and the code fell back to plain text.
    pub recognized: bool,
}

pub struct Highlighter {
    syntax_set: SyntaxSet,
}

enum Alias {
    PlainText,
    Name(&'static str),
}

fn alias(language: &str) -> Option<Alias> {
    match language.to_lowercase().as_str() {
        "" | "plain" | "text" | "txt" => Some(Alias::PlainText),
        // TSX grammar is a superset of TS, so it also covers JSX in .ts snippets.
        "ts" | "typescript" | "tsx" | "jsx" => Some(Alias::Name("TypeScriptReact")),
        "bash" | "sh" => Some(Alias::Name("Bourne Again Shell (bash)")),
        "md" | "yamd" => Some(Alias::Name("Markdown")),
        _ => None,
    }
}

impl Highlighter {
    /// # Errors
    /// Returns error if the embedded syntax set cannot be deserialized.
    pub fn new() -> Result<Self, Error> {
        let syntax_set =
            from_uncompressed_data(include_bytes!(concat!(env!("OUT_DIR"), "/syntaxes.bin")))
                .map_err(|e| Error::Load(e.to_string()))?;
        Ok(Self { syntax_set })
    }

    fn find_syntax(&self, language: &str) -> Option<&SyntaxReference> {
        match alias(language) {
            Some(Alias::PlainText) => Some(self.syntax_set.find_syntax_plain_text()),
            Some(Alias::Name(name)) => self.syntax_set.find_syntax_by_name(name),
            None => self
                .syntax_set
                .find_syntax_by_name(language)
                .or_else(|| self.syntax_set.find_syntax_by_token(language)),
        }
    }

    /// Renders `code` as HTML with scope names as CSS classes.
    ///
    /// # Errors
    /// Returns error if syntect fails to parse a line.
    pub fn highlight(&self, code: &str, language: &str) -> Result<Highlighted, Error> {
        let found = self.find_syntax(language);
        let recognized = found.is_some();
        let syntax = found.unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut generator = ClassedHTMLGenerator::new_with_class_style(
            syntax,
            &self.syntax_set,
            ClassStyle::Spaced,
        );
        for line in LinesWithEndings::from(code) {
            generator.parse_html_for_line_which_includes_newline(line)?;
        }
        Ok(Highlighted {
            html: generator.finalize(),
            recognized,
        })
    }
}
