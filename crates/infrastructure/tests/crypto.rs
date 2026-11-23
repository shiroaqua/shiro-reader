use argon2::Params;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde_json::{Value, json};
use shiro_application::identity::crypto::errors::CryptoApplicationError;
use shiro_infrastructure::crypto::CryptoService;
use uuid::Uuid;

// Public test fixtures only; never use application credentials here.
const SECRET: &[u8] = b"shiro-reader-test-key-at-least-32-bytes-long";
const ISSUER: &str = "shiro-reader-test";
const AUDIENCE: &str = "shiro-reader-test-api";
const ACCESS_TTL: u64 = 3600;
const REFRESH_TTL: u64 = 86400;

fn now_sec() -> u64 {
    jiff::Timestamp::now().as_second().try_into().unwrap()
}

fn claims(user_id: &str) -> Value {
    let now = now_sec();
    json!({
        "iss": ISSUER, "aud": AUDIENCE, "sub": user_id,
        "iat": now, "nbf": now - 60, "exp": now + ACCESS_TTL,
        "jti": Uuid::new_v4().to_string(),
    })
}

fn sign(claims: &Value, typ: Option<&str>, algorithm: Algorithm, secret: &[u8]) -> String {
    let mut header = Header::new(algorithm);
    header.typ = typ.map(str::to_owned);
    encode(
        &header,
        claims,
        &EncodingKey::from_secret(secret).expect("Invalid jwt secret key format."),
    )
    .unwrap()
}

fn access_token(claims: &Value) -> String {
    sign(claims, Some("at+jwt"), Algorithm::HS256, SECRET)
}

fn crypto() -> CryptoService {
    crypto_with(SECRET, 0)
}

fn crypto_with(secret: &[u8], leeway: u64) -> CryptoService {
    CryptoService::new(
        Params::new(1024, 1, 1, Some(32)).unwrap(),
        secret,
        ISSUER.into(),
        AUDIENCE.into(),
        ACCESS_TTL,
        REFRESH_TTL,
        leeway,
    )
}

fn verified_claims(token: &str) -> Value {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    decode::<Value>(
        token,
        &DecodingKey::from_secret(SECRET).expect("Invalid jwt secret key format."),
        &validation,
    )
    .unwrap()
    .claims
}

fn assert_rejected_for_both(claims: &Value) {
    let crypto = crypto();
    assert!(
        crypto.verify_access_token(&access_token(claims)).is_err(),
        "accepted access claims: {claims}"
    );
    let refresh = sign(claims, Some("refresh+jwt"), Algorithm::HS256, SECRET);
    assert!(
        crypto.verify_refresh_token(&refresh).is_err(),
        "accepted refresh claims: {claims}"
    );
}

// 验证密码哈希使用随机盐值（Argon2id），且正确密码、错误密码与损坏 PHC 的校验行为正确。
#[test]
fn passwords_use_random_salts_and_verify_correctly() {
    let crypto = crypto();
    let first = crypto.hash_password("密码123").unwrap();
    let second = crypto.hash_password("密码123").unwrap();
    assert_ne!(first, second);
    assert!(first.starts_with("$argon2id$v=19$"));
    assert!(crypto.verify_password("密码123", &first).unwrap());
    assert!(crypto.verify_password("密码123", &second).unwrap());
    assert!(!crypto.verify_password("wrong password", &first).unwrap());
    assert!(crypto.verify_password("密码123", "broken PHC").is_err());
}

// 验证签发的访问令牌和刷新令牌具有正确签名、类型头、完整声明与唯一 jti。
#[test]
fn issued_tokens_have_valid_signatures_claims_types_and_unique_ids() {
    let crypto = crypto();
    let id = Uuid::new_v4().to_string();
    let before = now_sec();
    let access = crypto.issue_access_token(id.clone()).unwrap();
    let second = crypto.issue_access_token(id.clone()).unwrap();
    let refresh = crypto.issue_refresh_token(id.clone()).unwrap();
    let after = now_sec();
    let mut jtis = std::collections::HashSet::new();
    for (token, typ, ttl) in [
        (&access, "at+jwt", ACCESS_TTL),
        (&second, "at+jwt", ACCESS_TTL),
        (&refresh, "refresh+jwt", REFRESH_TTL),
    ] {
        let header = jsonwebtoken::decode_header(token).unwrap();
        assert_eq!(header.alg, Algorithm::HS256);
        assert_eq!(header.typ.as_deref(), Some(typ));
        let claims = verified_claims(token);
        assert_eq!(claims["iss"], ISSUER);
        assert_eq!(claims["aud"], AUDIENCE);
        assert_eq!(claims["sub"], id);
        let iat = claims["iat"].as_u64().unwrap();
        assert!((before..=after).contains(&iat));
        assert_eq!(claims["nbf"], iat);
        assert_eq!(claims["exp"], iat + ttl);
        let jti = claims["jti"].as_str().unwrap();
        Uuid::parse_str(jti).unwrap();
        assert!(jtis.insert(jti.to_owned()));
    }
    assert_eq!(crypto.verify_access_token(&access).unwrap(), id);
    assert_eq!(crypto.verify_refresh_token(&refresh).unwrap(), id);
}

// 验证签名密钥、算法限制与畸形/无签名/篡改载荷令牌均被正确校验并拒绝。
#[test]
fn signatures_algorithms_and_token_structure_are_verified() {
    let crypto = crypto();
    let payload = claims("user");
    for token in [
        sign(
            &payload,
            Some("at+jwt"),
            Algorithm::HS256,
            b"another-test-key-at-least-32-bytes",
        ),
        sign(&payload, Some("at+jwt"), Algorithm::HS384, SECRET),
        sign(&payload, Some("at+jwt"), Algorithm::HS512, SECRET),
    ] {
        assert!(crypto.verify_access_token(&token).is_err());
    }

    let valid = access_token(&payload);
    let pieces: Vec<_> = valid.split('.').collect();
    // Replace a signed payload with another syntactically valid payload; keep
    // the original signature, so this is a signature check, not just bad JSON.
    let other = access_token(&claims("attacker"));
    let other_payload = other.split('.').nth(1).unwrap();
    let tampered = format!("{}.{other_payload}.{}", pieces[0], pieces[2]);
    let unsigned = "eyJhbGciOiJub25lIiwidHlwIjoiYXQrand0In0.e30.";
    for token in ["", "garbage", "a.b.c", unsigned, &tampered] {
        assert!(crypto.verify_access_token(token).is_err());
        assert!(crypto.verify_refresh_token(token).is_err());
    }
}

// 验证访问令牌和刷新令牌的 typ 不可省略或互换。
#[test]
fn access_and_refresh_types_cannot_be_interchanged_or_omitted() {
    let crypto = crypto();
    let claims = claims("user");
    for typ in [None, Some("JWT"), Some("refresh+jwt")] {
        let token = sign(&claims, typ, Algorithm::HS256, SECRET);
        assert!(
            matches!(
                crypto.verify_access_token(&token),
                Err(CryptoApplicationError::InvalidTokenType)
            ),
            "typ={typ:?}"
        );
    }
    for typ in [None, Some("JWT"), Some("at+jwt")] {
        let token = sign(&claims, typ, Algorithm::HS256, SECRET);
        assert!(
            matches!(
                crypto.verify_refresh_token(&token),
                Err(CryptoApplicationError::InvalidTokenType)
            ),
            "typ={typ:?}"
        );
    }
}

// 验证不匹配的签发者和受众对两类令牌均被拒绝。
#[test]
fn issuer_and_audience_must_match() {
    for (field, value) in [
        ("iss", json!("other")),
        ("aud", json!("other")),
        ("aud", json!(["other", "also-other"])),
        ("aud", json!([])),
    ] {
        let mut claims = claims("user");
        claims[field] = value;
        assert_rejected_for_both(&claims);
    }
}

// 验证 aud 支持包含目标受众的字符串数组，并拒绝非字符串元素。
#[test]
fn audience_accepts_string_arrays_but_rejects_other_member_types() {
    // RFC 7519 §4.1.3: aud may be an array of StringOrURI values.
    let crypto = crypto();
    for audience in [json!([AUDIENCE]), json!(["another-api", AUDIENCE])] {
        let mut claims = claims("user");
        claims["aud"] = audience;
        let access = access_token(&claims);
        assert_eq!(
            crypto
                .verify_access_token(&access)
                .expect("RFC 7519 audience array must be accepted"),
            "user"
        );
        let refresh = sign(&claims, Some("refresh+jwt"), Algorithm::HS256, SECRET);
        assert_eq!(crypto.verify_refresh_token(&refresh).unwrap(), "user");
    }

    for audience in [
        json!([AUDIENCE, 42]),
        json!([AUDIENCE, null]),
        json!([AUDIENCE, {}]),
    ] {
        let mut claims = claims("user");
        claims["aud"] = audience;
        assert_rejected_for_both(&claims);
    }
}

// 验证应用要求的全部必填 JWT 声明不可缺失。
#[test]
fn required_claims_cannot_be_missing() {
    for field in ["exp", "nbf", "iss", "aud", "sub", "iat", "jti"] {
        let mut claims = claims("user");
        claims.as_object_mut().unwrap().remove(field);
        assert_rejected_for_both(&claims);
    }
}

// 验证 exp 和 nbf 的校验遵循配置的时钟偏差宽限。
#[test]
fn expiration_and_not_before_respect_configured_clock_leeway() {
    let mut expired = claims("user");
    expired["exp"] = json!(now_sec() - 3600);
    assert_rejected_for_both(&expired);
    let mut future = claims("user");
    future["nbf"] = json!(now_sec() + 3600);
    assert_rejected_for_both(&future);

    let crypto = crypto_with(SECRET, 300);
    for (field, time, expected) in [
        ("exp", now_sec() - 30, true),
        ("exp", now_sec() - 3600, false),
        ("nbf", now_sec() + 30, true),
        ("nbf", now_sec() + 3600, false),
    ] {
        let mut claims = claims("user");
        claims[field] = json!(time);
        assert_eq!(
            crypto.verify_access_token(&access_token(&claims)).is_ok(),
            expected,
            "{field}={time}"
        );
    }
}

// 验证 HS256 构造时拒绝短于 256 位的密钥，并接受 256 位密钥。
#[test]
fn hs256_requires_at_least_a_256_bit_key() {
    for key in [b"".as_slice(), b"short", &[b'x'; 31]] {
        assert!(
            std::panic::catch_unwind(|| crypto_with(key, 0)).is_err(),
            "key length {} should be rejected",
            key.len()
        );
    }
    let crypto = crypto_with(&[b'x'; 32], 0);
    let access = crypto.issue_access_token("user".into()).unwrap();
    assert_eq!(crypto.verify_access_token(&access).unwrap(), "user");
}
