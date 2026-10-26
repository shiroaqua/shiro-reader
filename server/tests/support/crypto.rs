#![allow(dead_code)]

use argon2::Params;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde_json::{Value, json};
use shiro_reader_server::application::identity::crypto::service::CryptoService;
use uuid::Uuid;

// Public test fixtures only; never use application credentials here.
pub const SECRET: &[u8] = b"shiro-reader-test-key-at-least-32-bytes-long";
pub const ISSUER: &str = "shiro-reader-test";
pub const AUDIENCE: &str = "shiro-reader-test-api";
pub const ACCESS_TTL: u64 = 3600;
pub const REFRESH_TTL: u64 = 86400;

pub fn crypto() -> CryptoService {
    crypto_with(SECRET, 0)
}

pub fn crypto_with(secret: &[u8], leeway: u64) -> CryptoService {
    CryptoService::new(
        Params::new(1024, 1, 1, Some(32)).unwrap(),
        secret, ISSUER.into(), AUDIENCE.into(), ACCESS_TTL, REFRESH_TTL, leeway,
    )
}

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
    encode(&header, claims, &EncodingKey::from_secret(secret)).unwrap()
}

pub fn access_token(claims: &Value) -> String {
    sign(claims, Some("at+jwt"), Algorithm::HS256, SECRET)
}

pub fn verified_claims(token: &str) -> Value {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    decode::<Value>(token, &DecodingKey::from_secret(SECRET), &validation).unwrap().claims
}

#[path = "crypto_checks.rs"]
pub mod checks;

#[derive(Clone, Copy, Debug)]
pub enum InvalidToken { Malformed, Expired, Future, WrongIssuer, WrongAudience, MissingExpiry, WrongSignature, WrongAlgorithm }

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
        InvalidToken::MissingExpiry => { payload.as_object_mut().unwrap().remove("exp"); }
        InvalidToken::WrongSignature => key = b"different-public-test-key-at-least-32-bytes",
        InvalidToken::WrongAlgorithm => algorithm = Algorithm::HS384,
    }
    sign(&payload, Some(if refresh { "refresh+jwt" } else { "at+jwt" }), algorithm, key)
}

fn now_sec() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}
