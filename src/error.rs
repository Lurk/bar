use std::{
    fmt::{Debug, Display},
    io,
};

pub struct Context<V, E>(V, E);

pub trait ContextExt<T, E> {
    fn with_context<V>(self, v: V) -> Result<T, Context<V, E>>;
}

impl<T, E> ContextExt<T, E> for Result<T, E> {
    fn with_context<V>(self, v: V) -> Result<T, Context<V, E>> {
        self.map_err(|e| Context(v, e))
    }
}

pub enum Errors {
    FileNotFound(String, io::Error),
    ConfigFileNotValid(serde_yaml::Error),
    TerraError(tera::Error),
    OsStringError(std::ffi::OsString),
    BinErr(bincode::Error),
}

impl From<io::Error> for Errors {
    fn from(err: io::Error) -> Self {
        Errors::FileNotFound("".to_string(), err)
    }
}

impl From<serde_yaml::Error> for Errors {
    fn from(err: serde_yaml::Error) -> Self {
        Errors::ConfigFileNotValid(err)
    }
}

impl From<tera::Error> for Errors {
    fn from(err: tera::Error) -> Self {
        Errors::TerraError(err)
    }
}

impl From<Context<String, io::Error>> for Errors {
    fn from(err: Context<String, io::Error>) -> Self {
        Errors::FileNotFound(err.0, err.1)
    }
}

impl From<std::ffi::OsString> for Errors {
    fn from(err: std::ffi::OsString) -> Self {
        Errors::OsStringError(err)
    }
}

impl From<bincode::Error> for Errors {
    fn from(err: bincode::Error) -> Self {
        Errors::BinErr(err)
    }
}

impl Display for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::FileNotFound(context, err) => {
                write!(f, "File {} not found:\n {}", context, err)
            }
            Errors::ConfigFileNotValid(err) => write!(f, "Config file not valid:\n {}", err),
            Errors::TerraError(err) => write!(f, "Terra error:\n {}", err),
            Errors::OsStringError(err) => write!(f, "OsString error:\n {:?}", err),
            Errors::BinErr(_) => todo!(),
        }
    }
}

impl Debug for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::FileNotFound(context, err) => {
                write!(f, "File {} not found:\n {:?}", context, err)
            }
            Errors::ConfigFileNotValid(err) => write!(f, "Config file not valid:\n {:#?}", err),
            Errors::TerraError(err) => write!(f, "Terra error:\n {:#?}", err),
            Errors::OsStringError(err) => write!(f, "OsString error:\n {:#?}", err),
            Errors::BinErr(_) => todo!(),
        }
    }
}
