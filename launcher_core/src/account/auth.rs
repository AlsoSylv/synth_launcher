use std::collections::HashMap;

use serde_json::json;

use super::types;

pub async fn device_response(
    client: &reqwest::Client,
    client_id: &str,
) -> Result<types::DeviceCodeResponse, crate::Error> {
    Ok(client
        .get("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client_id),
            ("response_type", "code"),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await?
        .json()
        .await?)
}

pub async fn authorization_token_response(
    client: &reqwest::Client,
    device_code: &str,
    client_id: &str,
) -> Result<types::AuthorizationTokenResponse, crate::Error> {
    Ok(client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()
        .await?
        .json()
        .await?)
}

pub async fn refresh_token_response(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
) -> Result<types::RefreshTokenResponse, crate::Error> {
    Ok(client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?
        .json()
        .await?)
}

pub async fn xbox_response(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<types::XboxLiveAuthenticationResponse, crate::Error> {
    Ok(client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
                "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": &format!("d={}", access_token)
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .json()
        .await?)
}

pub async fn xbox_security_token_response(
    client: &reqwest::Client,
    token: &str,
) -> Result<types::XboxLiveAuthenticationResponse, crate::Error> {
    Ok(client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        // TODO: Replace with struct
        .json(&json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [&token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .json()
        .await?)
}

pub async fn minecraft_response(
    display_claims: &HashMap<String, Vec<HashMap<String, String>>>,
    token: &str,
    client: &reqwest::Client,
) -> Result<types::MinecraftAuthenticationResponse, crate::Error> {
    Ok(client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({
            "identityToken": format!("XBL3.0 x={};{}", &display_claims["xui"][0]["uhs"], token)
        }))
        .send()
        .await?
        .json()
        .await?)
}

pub async fn minecraft_profile_response(
    access_token: &str,
    client: &reqwest::Client,
) -> Result<types::Profile, crate::Error> {
    Ok(client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await?
        .json()
        .await?)
}

pub async fn minecraft_ownership_response(
    access_token: &str,
    client: &reqwest::Client,
) -> Result<types::ProductCheck, crate::Error> {
    Ok(client
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(access_token)
        .send()
        .await?
        .json()
        .await?)
}
