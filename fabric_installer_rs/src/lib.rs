use types::Full;
use crate::types::{Game, Loader};

pub mod types;

pub const BASE: &str = "https://meta.fabricmc.net";

pub async fn full(client: &reqwest::Client) -> reqwest::Result<Full> {
    client.get(format!("{BASE}/v2/versions")).send().await?.json().await
}

pub async fn game_versions(client: &reqwest::Client) -> reqwest::Result<Vec<Game>> {
    client.get(format!("{BASE}/v2/versions/game")).send().await?.json().await
}

pub async fn game_yarn_versions(client: &reqwest::Client) -> reqwest::Result<Vec<Game>> {
    client.get(format!("{BASE}/v2/versions/game/yarn")).send().await?.json().await
}

pub async fn game_intermediary_versions(client: &reqwest::Client) -> reqwest::Result<Vec<Game>> {
    client.get(format!("{BASE}/v2/versions/game/intermediary")).send().await?.json().await
}

pub async fn intermediary_versions(client: &reqwest::Client) -> reqwest::Result<Vec<Loader>> {
    client.get(format!("{BASE}/v2/versions/intermediary")).send().await?.json().await
}

pub async fn intermediary_versions_for_game_version(client: &reqwest::Client, game_version: &str) -> reqwest::Result<Vec<Loader>> {
    client.get(format!("{BASE}/v2/versions/intermediary/{game_version}")).send().await?.json().await
}