use std::collections::HashMap;

use crate::types::modern::Value;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u64,
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AuthorizationTokenResponse {
    pub token_type: String,
    pub scope: String,
    pub expires_in: u64,
    pub ext_expires_in: u32,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub scope: String,
    pub expires_in: u32,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct XboxLiveAuthenticationResponse {
    pub issue_instant: String,
    pub not_after: String,
    pub token: String,
    pub display_claims: HashMap<String, Vec<HashMap<String, String>>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MinecraftAuthenticationResponse {
    pub username: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCheck {
    pub items: Vec<Item>,
    pub signature: Value,
    pub key_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub signature: Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub skins: Vec<Skin>,
    pub capes: Vec<Cape>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Skin {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    pub alias: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Account {
    pub active: bool,
    pub expiry: u64,
    pub access_token: String,
    pub refresh_token: String,
    pub profile: Profile,
}
