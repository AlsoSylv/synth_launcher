use launcher_core::account;
use std::fmt::Display;

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    Tokio(tokio::io::Error),
    SerdeJson(serde_json::Error),
    Profile(account::types::ProfileError),
    TomlDe(toml::de::Error),
}

impl From<launcher_core::Error> for Error {
    fn from(value: launcher_core::Error) -> Self {
        match value {
            launcher_core::Error::Reqwest(e) => Error::Reqwest(e),
            launcher_core::Error::Tokio(e) => Error::Tokio(e),
            launcher_core::Error::SerdeJson(e) => Error::SerdeJson(e),
            launcher_core::Error::ProfileError(e) => Error::Profile(e),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::Reqwest(value)
    }
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Error::Tokio(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::SerdeJson(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::TomlDe(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str: &dyn Display = match self {
            Error::Reqwest(err) => err,
            Error::Tokio(err) => err,
            Error::SerdeJson(err) => err,
            Error::Profile(err) => err,
            Error::TomlDe(err) => err,
        };
        write!(f, "{}", str)
    }
}
