#![allow(dead_code)]

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use uuid::Uuid;

// Public test fixtures only; never use application credentials here.
pub const SECRET: &[u8] = b"shiro-reader-test-key-at-least-32-bytes-long";
pub const ISSUER: &str = "shiro-reader-test";
pub const AUDIENCE: &str = "shiro-reader-test-api";
pub const ACCESS_TTL: u64 = 3600;
pub const REFRESH_TTL: u64 = 86400;

pub fn claims(user_id: &str) -> Value {
    let now = now_sec();
    json!({
        "iss": ISSUER, "aud": AUDIENCE, "sub": user_id,
        "iat": now, "nbf": now - 60, "exp": now + ACCESS_TTL,
        "jti": Uuid::new_v4().to_string(),
    })
}

pub fn sign(claims: &Value, typ: Option<&str>, algorithm: Algorithm, secret: &[u8]) -> String {
    let mut header = Header::new(algorithm);
    header.typ = typ.map(str::to_owned);
    encode(
        &header,
        claims,
        &EncodingKey::from_secret(secret).expect("Invalid jwt secret key format."),
    )
    .unwrap()
}

pub fn access_token(claims: &Value) -> String {
    sign(claims, Some("at+jwt"), Algorithm::HS256, SECRET)
}

#[derive(Clone, Copy, Debug)]
pub enum InvalidToken {
    Malformed,
    Expired,
    Future,
    WrongIssuer,
    WrongAudience,
    MissingExpiry,
    WrongSignature,
    WrongAlgorithm,
}

impl InvalidToken {
    pub const ALL: [Self; 8] = [
        Self::Malformed,
        Self::Expired,
        Self::Future,
        Self::WrongIssuer,
        Self::WrongAudience,
        Self::MissingExpiry,
        Self::WrongSignature,
        Self::WrongAlgorithm,
    ];
}

pub fn invalid_token(case: InvalidToken, refresh: bool) -> String {
    let mut payload = claims("user");
    let mut key = SECRET;
    let mut algorithm = Algorithm::HS256;
    match case {
        InvalidToken::Malformed => return "invalid-token".into(),
        InvalidToken::Expired => payload["exp"] = json!(now_sec() - 3600),
        InvalidToken::Future => payload["nbf"] = json!(now_sec() + 3600),
        InvalidToken::WrongIssuer => payload["iss"] = json!("another-issuer"),
        InvalidToken::WrongAudience => payload["aud"] = json!("another-api"),
        InvalidToken::MissingExpiry => {
            payload.as_object_mut().unwrap().remove("exp");
        }
        InvalidToken::WrongSignature => key = b"different-public-test-key-at-least-32-bytes",
        InvalidToken::WrongAlgorithm => algorithm = Algorithm::HS384,
    }
    sign(
        &payload,
        Some(if refresh { "refresh+jwt" } else { "at+jwt" }),
        algorithm,
        key,
    )
}

pub fn now_sec() -> u64 {
    jiff::Timestamp::now().as_second().try_into().unwrap()
}
