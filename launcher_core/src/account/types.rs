use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u64,
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub scope: String,
    pub expires_in: u64,
    pub ext_expires_in: u32,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub scope: String,
    pub expires_in: u64,
    pub ext_expires_in: u32,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct XboxLiveAuthenticationResponse {
    pub issue_instant: String,
    pub not_after: String,
    pub token: String,
    pub display_claims: HashMap<String, Vec<HashMap<String, String>>>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MinecraftAuthenticationResponse {
    pub username: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u32,
    pub roles: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ProductCheck {
    pub items: Vec<Item>,
    pub signature: String,
    pub key_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub name: String,
    pub signature: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub skins: Vec<Skin>,
    pub capes: Vec<Cape>,
    #[serde(rename = "profileActions")]
    pub profile_actions: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileError {
    pub path: String,
    pub error_type: String,
    pub error: String,
    pub error_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProfileResult {
    Ok(Profile),
    Err(ProfileError),
}

impl Display for ProfileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {}, Reason: {}",
            self.error, self.error_message
        )
    }
}

impl Error for ProfileError {}

impl From<ProfileResult> for Result<Profile, crate::Error> {
    fn from(value: ProfileResult) -> Self {
        match value {
            ProfileResult::Ok(p) => Ok(p),
            ProfileResult::Err(e) => Err(crate::Error::ProfileError(e)),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
    #[serde(rename = "textureKey")]
    pub texture_key: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    pub alias: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub active: bool,
    pub expiry: u64,
    pub access_token: String,
    pub profile: Profile,
}

impl From<RefreshTokenResponse> for AuthorizationTokenResponse {
    fn from(val: RefreshTokenResponse) -> Self {
        AuthorizationTokenResponse {
            expires_in: val.expires_in,
            refresh_token: val.refresh_token,
            access_token: val.access_token,
            ext_expires_in: val.ext_expires_in,
            scope: val.scope,
            token_type: val.token_type,
        }
    }
}
