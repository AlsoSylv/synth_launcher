use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Full {
    pub game: Vec<Game>,
    pub mappings: Vec<Loader>,
    pub intermediary: Vec<Installer>,
    pub loader: Vec<Loader>,
    pub installer: Vec<Installer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Game {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Installer {
    pub url: Option<String>,
    pub maven: String,
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loader {
    pub separator: String,
    pub build: i64,
    pub maven: String,
    pub version: String,
    pub stable: bool,
    pub game_version: Option<String>,
}
