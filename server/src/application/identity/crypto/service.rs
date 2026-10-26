use argon2::{
    Algorithm, Argon2, Params, PasswordVerifier, Version,
    password_hash::{
        Error as PasswordHashError, PasswordHasher, phc::PasswordHash, try_generate_salt,
    },
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::time::now_sec;

use super::errors::CryptoApplicationError;

pub struct CryptoService {
    argon2_params: Params,
    jwt_parms: JwtParms,
}
pub struct JwtParms {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    audience: String,
    validation: Validation,
    access_ttl_secs: u64,
    refresh_ttl_secs: u64,
}

impl CryptoService {
    pub fn new(
        argon2_params: Params,
        jwt_secret: &[u8],
        jwt_issuer: String,
        jwt_audience: String,
        jwt_access_ttl_secs: u64,
        jwt_refresh_ttl_secs: u64,
        jwt_vaildation_leeway: u64,
    ) -> Self {
        if jwt_secret.len() < 32 {
            panic!("HS256 keys must contain at least 32 bytes")
        }


        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.leeway = jwt_vaildation_leeway;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.set_issuer(&[jwt_issuer.as_str()]);
        validation.set_audience(&[jwt_audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud", "sub", "iat", "jti"]);

        Self {
            argon2_params: argon2_params,
            jwt_parms: JwtParms {
                encoding_key: EncodingKey::from_secret(jwt_secret),
                decoding_key: DecodingKey::from_secret(jwt_secret),
                issuer: jwt_issuer,
                audience: jwt_audience,
                validation: validation,
                access_ttl_secs: jwt_access_ttl_secs,
                refresh_ttl_secs: jwt_refresh_ttl_secs,
            },
        }
    }

    pub fn hash_password(&self, plaintext: &str) -> Result<String, CryptoApplicationError> {
        let salt = try_generate_salt().map_err(|_| CryptoApplicationError::SaltGeneration)?;
        let password_hash = self
            .argon2id()
            .hash_password_with_salt(plaintext.as_bytes(), &salt)?
            .to_string();
        Ok(password_hash)
    }
    pub fn verify_password(
        &self,
        plaintext: &str,
        stored_hash: &str,
    ) -> Result<bool, CryptoApplicationError> {
        let parsed_hash = PasswordHash::new(stored_hash)?;
        match self
            .argon2id()
            .verify_password(plaintext.as_bytes(), &parsed_hash)
        {
            Ok(()) => Ok(true),
            Err(PasswordHashError::PasswordInvalid) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn issue_access_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        let now = now_sec();
        let claims = Claims {
            iss: self.jwt_parms.issuer.clone(),
            sub: user_id,
            aud: Audience::Single(self.jwt_parms.audience.clone()),
            iat: now,
            nbf: now,
            exp: now + self.jwt_parms.access_ttl_secs,
            jti: Uuid::new_v4().to_string(),
        };
        let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
        header.typ = Some("at+jwt".to_owned());

        Ok(encode(&header, &claims, &self.jwt_parms.encoding_key)?)
    }

    pub fn issue_refresh_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        let now = now_sec();
        let claims = Claims {
            iss: self.jwt_parms.issuer.clone(),
            sub: user_id.to_string(),
            aud: Audience::Single(self.jwt_parms.audience.clone()),
            iat: now,
            nbf: now,
            exp: now + self.jwt_parms.refresh_ttl_secs,
            jti: Uuid::new_v4().to_string(),
        };
        let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
        header.typ = Some("refresh+jwt".to_owned());
        Ok(encode(&header, &claims, &self.jwt_parms.encoding_key)?)
    }

    pub fn verify_access_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        let token_data = decode::<Claims>(
            token,
            &self.jwt_parms.decoding_key,
            &self.jwt_parms.validation,
        )?;
        if !token_data.header.typ.is_some_and(|f| f == "at+jwt") {
            return Err(CryptoApplicationError::InvalidTokenType);
        }

        Ok(token_data.claims.sub)
    }

    pub fn verify_refresh_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        let token_data = decode::<Claims>(
            token,
            &self.jwt_parms.decoding_key,
            &self.jwt_parms.validation,
        )?;
        if !token_data.header.typ.is_some_and(|f| f == "refresh+jwt") {
            return Err(CryptoApplicationError::InvalidTokenType);
        }

        Ok(token_data.claims.sub)
    }

    fn argon2id(&self) -> Argon2<'static> {
        Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            self.argon2_params.clone(),
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    // JWT 标准 Claims
    iss: String, // Issuer：签发者，通常是认证服务器
    sub: String, // Subject：主题，通常是用户 ID
    aud: Audience, // Audience：接收方/受众，比如某个服务或 API
    exp: u64,    // Expiration Time：过期时间戳，超过后 token 无效
    nbf: u64,    // Not Before：生效时间戳，在此之前 token 无效
    iat: u64,    // Issued At：签发时间戳
    jti: String, // JWT ID：JWT 唯一标识，常用于防重放
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}

