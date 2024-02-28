use serde::Serialize;
use std::collections::HashMap;

use crate::account::types::ProfileResult;
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
    token_response(client, device_code, client_id, "urn:ietf:params:oauth:grant-type:device_code").await
}

pub async fn refresh_token_response(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
) -> Result<types::AuthorizationTokenResponse, crate::Error> {
    token_response(client, refresh_token, client_id, "refresh_token").await
}

pub async fn token_response(
    client: &reqwest::Client,
    device_code: &str,
    client_id: &str,
    grant_type: &str,
) -> Result<types::AuthorizationTokenResponse, crate::Error> {
    Ok(client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", grant_type),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()
        .await?
        .json()
        .await?)
}


#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LiveAuthRequest {
    properties: Properties,
    relying_party: &'static str,
    token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Properties {
    auth_method: &'static str,
    site_name: &'static str,
    rps_ticket: String,
}

pub async fn xbox_response(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<types::XboxLiveAuthenticationResponse, crate::Error> {
    Ok(client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&LiveAuthRequest {
            properties: Properties {
                auth_method: "RPS",
                site_name: "user.auth.xboxlive.com",
                rps_ticket: format!("d={}", access_token),
            },
            relying_party: "http://auth.xboxlive.com",
            token_type: "JWT",
        })
        .send()
        .await?
        .json()
        .await?)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Security<'se> {
    properties: Props<'se>,
    relying_party: &'static str,
    token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Props<'se> {
    sandbox_id: &'static str,
    user_tokens: [&'se str; 1],
}

pub async fn xbox_security_token_response(
    client: &reqwest::Client,
    token: &str,
) -> Result<types::XboxLiveAuthenticationResponse, crate::Error> {
    Ok(client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        // TODO: Replace with struct
        /*
        json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [&token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        })
         */
        .json(&Security {
            properties: Props {
                sandbox_id: "RETAIL",
                user_tokens: [token]
            },
            relying_party: "rp://api.minecraftservices.com/",
            token_type: "JWT"
        })
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
    client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await?
        .json::<ProfileResult>()
        .await?
        .into()
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
