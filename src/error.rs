use std::{
    error::Error,
    fmt::{Debug, Display},
    io,
    path::StripPrefixError,
};

use itertools::Itertools;
use tokio::task::JoinError;
use url::ParseError;

pub struct BarErr {
    err: Errors,
    context: Vec<String>,
}

impl BarErr {
    pub fn new(err: Errors, context: Vec<String>) -> Self {
        Self { err, context }
    }
}

pub trait ContextExt<T> {
    fn with_context<V>(self, v: V) -> Result<T, BarErr>
    where
        V: FnOnce() -> String;
}

impl<T, E> ContextExt<T> for Result<T, E>
where
    E: Into<BarErr>,
{
    fn with_context<V>(self, v: V) -> Result<T, BarErr>
    where
        V: FnOnce() -> String,
    {
        self.map_err(|e| {
            let mut err: BarErr = e.into();
            err.context.push(v());
            err
        })
    }
}

#[derive(Debug)]
pub enum Errors {
    IO(io::Error),
    YamlParseError(serde_yaml::Error),
    JsonParseError(serde_json::Error),
    TerraError(tera::Error),
    OsStringError(std::ffi::OsString),
    BinErr(bincode::Error),
    StripPrefixError(StripPrefixError),
    ParseError(ParseError),
    Str(String),
    JoinError(JoinError),
    CandleCore(candle_core::Error),
    ReqwestError(reqwest::Error),
    HFAPIError(hf_hub::api::tokio::ApiError),
    Boxed(Box<dyn std::error::Error + Send + Sync + 'static>),
    ImageError(image::ImageError),
}

impl From<io::Error> for BarErr {
    fn from(err: io::Error) -> Self {
        BarErr {
            err: Errors::IO(err),
            context: vec![],
        }
    }
}

impl From<serde_yaml::Error> for BarErr {
    fn from(err: serde_yaml::Error) -> Self {
        BarErr {
            err: Errors::YamlParseError(err),
            context: vec![],
        }
    }
}

impl From<serde_json::Error> for BarErr {
    fn from(err: serde_json::Error) -> Self {
        BarErr {
            err: Errors::JsonParseError(err),
            context: vec![],
        }
    }
}

impl From<tera::Error> for BarErr {
    fn from(err: tera::Error) -> Self {
        BarErr {
            err: Errors::TerraError(err),
            context: vec![],
        }
    }
}

impl From<std::ffi::OsString> for BarErr {
    fn from(err: std::ffi::OsString) -> Self {
        BarErr {
            err: Errors::OsStringError(err),
            context: vec![],
        }
    }
}

impl From<bincode::Error> for BarErr {
    fn from(err: bincode::Error) -> Self {
        BarErr {
            err: Errors::BinErr(err),
            context: vec![],
        }
    }
}

impl From<StripPrefixError> for BarErr {
    fn from(err: StripPrefixError) -> Self {
        BarErr {
            err: Errors::StripPrefixError(err),
            context: vec![],
        }
    }
}

impl From<ParseError> for BarErr {
    fn from(err: ParseError) -> Self {
        BarErr {
            err: Errors::ParseError(err),
            context: vec![],
        }
    }
}

impl From<String> for BarErr {
    fn from(err: String) -> Self {
        BarErr {
            err: Errors::Str(err),
            context: vec![],
        }
    }
}

impl From<&str> for BarErr {
    fn from(err: &str) -> Self {
        BarErr {
            err: Errors::Str(err.to_string()),
            context: vec![],
        }
    }
}

impl From<JoinError> for BarErr {
    fn from(err: JoinError) -> Self {
        BarErr {
            err: Errors::JoinError(err),
            context: vec![],
        }
    }
}

impl From<candle_core::Error> for BarErr {
    fn from(err: candle_core::Error) -> Self {
        BarErr {
            err: Errors::CandleCore(err),
            context: vec![],
        }
    }
}

impl From<reqwest::Error> for BarErr {
    fn from(err: reqwest::Error) -> Self {
        BarErr {
            err: Errors::ReqwestError(err),
            context: vec![],
        }
    }
}

impl From<hf_hub::api::tokio::ApiError> for BarErr {
    fn from(err: hf_hub::api::tokio::ApiError) -> Self {
        BarErr {
            err: Errors::HFAPIError(err),
            context: vec![],
        }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync + 'static>> for BarErr {
    fn from(err: Box<dyn std::error::Error + Send + Sync + 'static>) -> Self {
        BarErr {
            err: Errors::Boxed(err),
            context: vec![],
        }
    }
}

impl From<image::ImageError> for BarErr {
    fn from(err: image::ImageError) -> Self {
        BarErr {
            err: Errors::ImageError(err),
            context: vec![],
        }
    }
}

fn recursive_terra_error(err: &(dyn Error + 'static)) -> String {
    if let Some(source) = &err.source() {
        format!("{}\n{}\n", err, recursive_terra_error(*source))
    } else {
        format!("{}\n", err)
    }
}

impl Display for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::IO(err) => f.write_str(err.to_string().as_str()),
            Errors::YamlParseError(err) => f.write_str(err.to_string().as_str()),
            Errors::JsonParseError(err) => f.write_str(err.to_string().as_str()),
            Errors::TerraError(err) => f.write_str(recursive_terra_error(err).as_str()),
            Errors::OsStringError(err) => f.write_str(format!("{err:#?}").as_str()),
            Errors::BinErr(err) => f.write_str(err.to_string().as_str()),
            Errors::StripPrefixError(err) => f.write_str(err.to_string().as_str()),
            Errors::ParseError(err) => f.write_str(err.to_string().as_str()),
            Errors::Str(err) => f.write_str(err.to_string().as_str()),
            Errors::JoinError(err) => f.write_str(err.to_string().as_str()),
            Errors::CandleCore(err) => f.write_str(err.to_string().as_str()),
            Errors::ReqwestError(err) => f.write_str(err.to_string().as_str()),
            Errors::HFAPIError(err) => f.write_str(err.to_string().as_str()),
            Errors::Boxed(err) => f.write_str(err.to_string().as_str()),
            Errors::ImageError(err) => f.write_str(err.to_string().as_str()),
        }
    }
}

impl Display for BarErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let context = if self.context.is_empty() {
            "".to_string()
        } else {
            format!(
                "context:\n{}",
                self.context
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(pos, message)| format!("\t{}. {message}", pos + 1))
                    .join("\n")
            )
        };
        writeln!(f, "Error:\n\n{}\n\n{}", self.err, context)
    }
}

impl Debug for BarErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let context = if self.context.is_empty() {
            "".to_string()
        } else {
            format!(
                "context:\n{}",
                self.context
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(pos, message)| format!("\t{}. {message}", pos + 1))
                    .join("\n")
            )
        };

        writeln!(f, "Error:\n\n{}\n\n{}", self.err, context)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::{BarErr, ContextExt};

    use pretty_assertions::assert_eq;

    #[test]
    fn multiple_context_display() {
        let error_message =
            "Error:\n\nactual error\n\ncontext:\n\t2. second\n\t1. first\n".to_string();
        let err: Result<(), BarErr> = Err("actual error")
            .with_context(|| "first".to_string())
            .with_context(|| "second".to_string());

        if let Err(bar) = err {
            assert_eq!(bar.to_string(), error_message);
        };
    }

    #[test]
    fn multiple_context_debug() {
        let error_message =
            "Error:\n\nactual error\n\ncontext:\n\t2. second\n\t1. first\n".to_string();
        let err: Result<(), BarErr> = Err("actual error")
            .with_context(|| "first".to_string())
            .with_context(|| "second".to_string());

        if let Err(bar) = err {
            assert_eq!(format!("{bar:?}"), error_message);
        };
    }
}
