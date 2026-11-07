use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshAccessTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshAccessTokenResponse {
    pub access_token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoutRequest {
    pub token: String,
}
