use std::fmt::{Debug, Display};

use miette::{
    Diagnostic, LabeledSpan, MietteHandler, NamedSource, ReportHandler, SourceCode, SourceSpan,
};

pub struct BarDiagnostic {
    message: String,
    source_error: Option<Box<BarDiagnostic>>,
    source_code: Option<NamedSource<String>>,
    labels: Vec<LabeledSpan>,
    help: Option<String>,
    related: Vec<BarDiagnostic>,
}

impl BarDiagnostic {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source_error: None,
            source_code: None,
            labels: vec![],
            help: None,
            related: vec![],
        }
    }

    /// Attach a sibling diagnostic. miette's default handler renders related
    /// diagnostics as their own sections (with their own snippet) below the
    /// main one — use this when a single error has multiple useful source
    /// locations to show (e.g. a template error that points back at a yamd
    /// metadata line).
    #[must_use]
    pub fn with_related(mut self, other: BarDiagnostic) -> Self {
        self.related.push(other);
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: BarDiagnostic) -> Self {
        self.source_error = Some(Box::new(source));
        self
    }

    #[must_use]
    pub fn with_source_code(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        let name: String = name.into();
        let content: String = content.into();
        self.source_code = Some(NamedSource::new(name, content));
        self
    }

    #[must_use]
    pub fn with_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels
            .push(LabeledSpan::new_with_span(Some(message.into()), span));
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl Display for BarDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Debug for BarDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handler = MietteHandler::new();
        handler.debug(self, f)
    }
}

impl std::error::Error for BarDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source_error
            .as_deref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

impl Diagnostic for BarDiagnostic {
    fn source_code(&self) -> Option<&dyn SourceCode> {
        if let Some(s) = self.source_code.as_ref() {
            return Some(s as &dyn SourceCode);
        }
        self.source_error
            .as_deref()
            .and_then(Diagnostic::source_code)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if !self.labels.is_empty() {
            return Some(Box::new(self.labels.iter().cloned()));
        }
        self.source_error.as_deref().and_then(Diagnostic::labels)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.help
            .as_deref()
            .map(|h| Box::new(h) as Box<dyn Display + 'a>)
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        if self.related.is_empty() {
            None
        } else {
            Some(Box::new(self.related.iter().map(|d| d as &dyn Diagnostic)))
        }
    }
}

/// Walk an `std::error::Error` source chain and produce a `BarDiagnostic`
/// where each link in the chain becomes a nested source. Without this, the
/// miette renderer only sees the top-level message and the underlying cause
/// (the actually useful bit) is dropped.
fn chain_diagnostic(err: &dyn std::error::Error) -> BarDiagnostic {
    let mut diag = BarDiagnostic::new(err.to_string());
    if let Some(source) = err.source() {
        diag = diag.with_source(chain_diagnostic(source));
    }
    diag
}

pub trait ContextExt<T> {
    /// # Errors
    /// Returns error if the underlying result is an error, with added context.
    fn with_context<V>(self, v: V) -> Result<T, BarDiagnostic>
    where
        V: FnOnce() -> String;
}

impl<T, E> ContextExt<T> for Result<T, E>
where
    E: Into<BarDiagnostic>,
{
    fn with_context<V>(self, v: V) -> Result<T, BarDiagnostic>
    where
        V: FnOnce() -> String,
    {
        self.map_err(|e| BarDiagnostic::new(v()).with_source(e.into()))
    }
}

impl From<std::io::Error> for BarDiagnostic {
    fn from(err: std::io::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<serde_yaml::Error> for BarDiagnostic {
    fn from(err: serde_yaml::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<serde_json::Error> for BarDiagnostic {
    fn from(err: serde_json::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<tera::Error> for BarDiagnostic {
    fn from(err: tera::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<std::ffi::OsString> for BarDiagnostic {
    fn from(err: std::ffi::OsString) -> Self {
        BarDiagnostic::new(err.to_string_lossy().into_owned())
    }
}

impl From<bincode::Error> for BarDiagnostic {
    fn from(err: bincode::Error) -> Self {
        chain_diagnostic(err.as_ref())
    }
}

impl From<std::path::StripPrefixError> for BarDiagnostic {
    fn from(err: std::path::StripPrefixError) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<url::ParseError> for BarDiagnostic {
    fn from(err: url::ParseError) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<String> for BarDiagnostic {
    fn from(err: String) -> Self {
        BarDiagnostic::new(err)
    }
}

impl From<&str> for BarDiagnostic {
    fn from(err: &str) -> Self {
        BarDiagnostic::new(err)
    }
}

impl From<tokio::task::JoinError> for BarDiagnostic {
    fn from(err: tokio::task::JoinError) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<reqwest::Error> for BarDiagnostic {
    fn from(err: reqwest::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<Box<dyn std::error::Error + Send + Sync + 'static>> for BarDiagnostic {
    fn from(err: Box<dyn std::error::Error + Send + Sync + 'static>) -> Self {
        chain_diagnostic(err.as_ref())
    }
}

impl From<gpxtools::GPXError> for BarDiagnostic {
    fn from(err: gpxtools::GPXError) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<toml::de::Error> for BarDiagnostic {
    fn from(err: toml::de::Error) -> Self {
        chain_diagnostic(&err)
    }
}

impl From<semver::Error> for BarDiagnostic {
    fn from(err: semver::Error) -> Self {
        chain_diagnostic(&err)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io;

    use miette::Report;

    use super::{BarDiagnostic, ContextExt};

    #[test]
    fn simple_error_displays_message() {
        let err = BarDiagnostic::new("something went wrong");
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn error_with_context_chains() {
        let inner = BarDiagnostic::new("inner error");
        let outer = BarDiagnostic::new("outer error").with_source(inner);

        assert_eq!(outer.to_string(), "outer error");

        let source = outer.source().expect("should have source");
        assert_eq!(source.to_string(), "inner error");
    }

    #[test]
    fn error_with_source_snippet() {
        let content = "fn main() {\n    let x = 1;\n}\n";
        let err = BarDiagnostic::new("unexpected token")
            .with_source_code("main.rs", content)
            .with_label((12usize, 3usize).into(), "here");

        let report = Report::new(err);
        let rendered = format!("{report:?}");
        assert!(rendered.contains("main.rs"), "rendered: {rendered}");
        assert!(
            rendered.contains("unexpected token"),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let bar: BarDiagnostic = io_err.into();
        assert!(bar.to_string().contains("file not found"));
    }

    #[test]
    fn from_string() {
        let bar: BarDiagnostic = String::from("string error").into();
        assert_eq!(bar.to_string(), "string error");
    }

    #[test]
    fn from_str() {
        let bar: BarDiagnostic = "str error".into();
        assert_eq!(bar.to_string(), "str error");
    }

    #[test]
    fn chained_external_error_preserves_source() {
        use std::error::Error as _;
        use std::fmt;

        #[derive(Debug)]
        struct Inner;
        impl fmt::Display for Inner {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("variable 'foo' not found")
            }
        }
        impl std::error::Error for Inner {}

        #[derive(Debug)]
        struct Outer(Inner);
        impl fmt::Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Failed to render 'index.html'")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let outer = Outer(Inner);
        let diag = super::chain_diagnostic(&outer);
        assert_eq!(diag.to_string(), "Failed to render 'index.html'");
        let source = diag.source().expect("inner cause should be preserved");
        assert_eq!(source.to_string(), "variable 'foo' not found");
    }

    #[test]
    fn context_ext_adds_context() {
        let result: Result<(), io::Error> = Err(io::Error::other("low level"));
        let result = result.with_context(|| "high level context".to_string());

        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "high level context");

        let source = err.source().expect("should have source");
        assert_eq!(source.to_string(), "low level");
    }

    #[test]
    fn outer_wrap_falls_through_to_inner_source_code_and_labels() {
        use miette::Diagnostic;

        let inner = BarDiagnostic::new("inner")
            .with_source_code("file.txt", "hello world")
            .with_label((0usize, 5usize).into(), "here");
        let outer = BarDiagnostic::new("outer").with_source(inner);

        assert!(
            outer.source_code().is_some(),
            "outer should fall through to inner source_code"
        );
        let labels: Vec<_> = outer.labels().expect("outer should have labels").collect();
        assert_eq!(labels.len(), 1, "outer should fall through to inner label");
        assert_eq!(labels[0].label(), Some("here"));
    }

    #[test]
    fn outer_with_own_source_code_does_not_fall_through() {
        use miette::Diagnostic;

        let inner = BarDiagnostic::new("inner")
            .with_source_code("inner.txt", "inner content")
            .with_label((0usize, 5usize).into(), "inner-label");
        let outer = BarDiagnostic::new("outer")
            .with_source_code("outer.txt", "outer content")
            .with_label((0usize, 5usize).into(), "outer-label")
            .with_source(inner);

        let labels: Vec<_> = outer.labels().expect("outer should have labels").collect();
        assert_eq!(
            labels.len(),
            1,
            "outer must keep its own labels, not fall through"
        );
        assert_eq!(labels[0].label(), Some("outer-label"));
    }
}
