use super::*;
use jsonwebtoken::{Algorithm, decode_header};
use serde_json::{Value, json};
use shiro_reader_server::application::identity::crypto::errors::CryptoApplicationError;
use uuid::Uuid;

pub fn password_hashes_use_random_salts_and_verify_real_passwords() {
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

pub fn issued_tokens_have_valid_signatures_types_claims_and_distinct_ids() {
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
        let header = decode_header(token).unwrap();
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

pub fn token_types_cannot_be_interchanged_or_omitted() {
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

pub fn malformed_unsigned_or_tampered_tokens_are_rejected() {
    let crypto = crypto();
    let valid = access_token(&claims("user"));
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

pub fn signatures_and_algorithms_are_verified() {
    let crypto = crypto();
    let claims = claims("user");
    for token in [
        sign(
            &claims,
            Some("at+jwt"),
            Algorithm::HS256,
            b"another-test-key-at-least-32-bytes",
        ),
        sign(&claims, Some("at+jwt"), Algorithm::HS384, SECRET),
        sign(&claims, Some("at+jwt"), Algorithm::HS512, SECRET),
    ] {
        assert!(crypto.verify_access_token(&token).is_err());
    }
}

pub fn incorrect_issuer_and_audience_are_rejected_for_both_token_types() {
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

pub fn every_application_required_claim_must_be_present() {
    for field in ["exp", "nbf", "iss", "aud", "sub", "iat", "jti"] {
        let mut claims = claims("user");
        claims.as_object_mut().unwrap().remove(field);
        assert_rejected_for_both(&claims);
    }
}

pub fn required_claims_cannot_be_null_or_have_incorrect_types() {
    for field in ["exp", "nbf", "iss", "aud", "sub", "iat", "jti"] {
        for value in [Value::Null, json!({"unexpected": true})] {
            let mut claims = claims("user");
            claims[field] = value;
            assert_rejected_for_both(&claims);
        }
    }
}

pub fn expired_and_not_yet_valid_tokens_are_rejected_without_sleep() {
    let mut expired = claims("user");
    expired["exp"] = json!(now_sec() - 3600);
    assert_rejected_for_both(&expired);
    let mut future = claims("user");
    future["nbf"] = json!(now_sec() + 3600);
    assert_rejected_for_both(&future);
}

pub fn configured_clock_leeway_is_applied_to_exp_and_nbf() {
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

pub fn jwt_audience_can_be_an_array_containing_the_recipient() {
    // RFC 7519 §4.1.3: aud may be an array of StringOrURI values. A singleton
    // string is only the special case; both representations are valid JWTs.
    for audience in [json!([AUDIENCE]), json!(["another-api", AUDIENCE])] {
        let mut claims = claims("user");
        claims["aud"] = audience;
        let crypto = crypto();
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
}

pub fn hs256_does_not_issue_tokens_with_keys_shorter_than_256_bits() {
    for key in [b"".as_slice(), b"short", &[b'x'; 31]] {
        // Rejecting configuration at construction is also a valid boundary.
        if let Ok(service) = std::panic::catch_unwind(|| crypto_with(key, 0)) {
            assert!(service.issue_access_token("user".into()).is_err());
            assert!(service.issue_refresh_token("user".into()).is_err());
        }
    }
}

pub fn assert_rejected_for_both(claims: &Value) {
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

pub fn hs256_accepts_a_256_bit_key() {
    let crypto = crypto_with(&[b'x'; 32], 0);
    let access = crypto.issue_access_token("user".into()).unwrap();
    assert_eq!(crypto.verify_access_token(&access).unwrap(), "user");
}

pub fn hs256_does_not_verify_tokens_with_keys_shorter_than_256_bits() {
    for key in [b"".as_slice(), b"short", &[b'x'; 31]] {
        if let Ok(service) = std::panic::catch_unwind(|| crypto_with(key, 0)) {
            let payload = claims("user");
            let access = sign(&payload, Some("at+jwt"), Algorithm::HS256, key);
            let refresh = sign(&payload, Some("refresh+jwt"), Algorithm::HS256, key);
            assert!(service.verify_access_token(&access).is_err());
            assert!(service.verify_refresh_token(&refresh).is_err());
        }
    }
}

pub fn audience_arrays_require_only_string_members() {
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
