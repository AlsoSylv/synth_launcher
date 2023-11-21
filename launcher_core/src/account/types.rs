use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    verification_uri: String,
    expires_in: u32,
    interval: u64,
    message: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AuthorizationTokenResponse {
    token_type: String,
    scope: String,
    expires_in: u64,
    ext_expires_in: u32,
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RefreshTokenResponse {
    access_token: String,
    refresh_token: String,
    scope: String,
    expires_in: u32,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct XboxLiveAuthenticationResponse {
    issue_instant: String,
    not_after: String,
    token: String,
    display_claims: HashMap<String, Vec<HashMap<String, String>>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MinecraftAuthenticationResponse {
    username: String,
    access_token: String,
    token_type: String,
    expires_in: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MinecraftProfileResponse {
    id: String,
    name: String,
    skins: Vec<Skin>,
    capes: Vec<Cape>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Skin {
    id: String,
    state: String,
    url: String,
    variant: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Cape {
    id: String,
    state: String,
    url: String,
    alias: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Account {
    pub active: bool,
    pub expiry: u64,
    pub access_token: String,
    pub refresh_token: String,
    pub profile: Profile,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub skins: Vec<Skin>,
    pub capes: Vec<Cape>,
}
