use std::io;

#[derive(Debug)]
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
