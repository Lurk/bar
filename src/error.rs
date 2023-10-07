use std::{
    fmt::{Debug, Display},
    io,
};

pub enum Errors {
    FileNotFound(io::Error),
    ConfigFileNotValid(serde_yaml::Error),
    TerraError(tera::Error),
}

impl From<io::Error> for Errors {
    fn from(err: io::Error) -> Self {
        Errors::FileNotFound(err)
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

impl Display for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::FileNotFound(err) => write!(f, "File not found:\n {}", err),
            Errors::ConfigFileNotValid(err) => write!(f, "Config file not valid:\n {}", err),
            Errors::TerraError(err) => write!(f, "Terra error:\n {}", err),
        }
    }
}

impl Debug for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::FileNotFound(err) => write!(f, "File not found:\n {}", err),
            Errors::ConfigFileNotValid(err) => write!(f, "Config file not valid:\n {}", err),
            Errors::TerraError(err) => write!(f, "Terra error:\n {}", err),
        }
    }
}
