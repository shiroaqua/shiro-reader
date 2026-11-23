use argon2::{
    Algorithm, Argon2, Params, PasswordVerifier, Version,
    password_hash::{
        Error as PasswordHashError, PasswordHasher, phc::PasswordHash, try_generate_salt,
    },
};
use jiff::Timestamp;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use shiro_application::identity::{crypto::errors::CryptoApplicationError, ports::IdentityCrypto};
use uuid::Uuid;

pub struct CryptoService {
    argon2_params: Params,
    jwt_params: JwtParams,
}

struct JwtParams {
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
        jwt_validation_leeway: u64,
    ) -> Self {
        assert!(
            jwt_secret.len() >= 32,
            "HS256 keys must contain at least 32 bytes"
        );

        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.leeway = jwt_validation_leeway;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.set_issuer(&[jwt_issuer.as_str()]);
        validation.set_audience(&[jwt_audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud", "sub", "iat", "jti"]);

        Self {
            argon2_params,
            jwt_params: JwtParams {
                encoding_key: EncodingKey::from_secret(jwt_secret)
                    .expect("Invalid jwt secret key format."),
                decoding_key: DecodingKey::from_secret(jwt_secret)
                    .expect("Invalid jwt secret key format."),
                issuer: jwt_issuer,
                audience: jwt_audience,
                validation,
                access_ttl_secs: jwt_access_ttl_secs,
                refresh_ttl_secs: jwt_refresh_ttl_secs,
            },
        }
    }

    pub fn hash_password(&self, plaintext: &str) -> Result<String, CryptoApplicationError> {
        let salt = try_generate_salt().map_err(|_| CryptoApplicationError::SaltGeneration)?;
        let password_hash = self
            .argon2id()
            .hash_password_with_salt(plaintext.as_bytes(), &salt)
            .map_err(|err| CryptoApplicationError::PasswordHash(err.to_string()))?
            .to_string();
        Ok(password_hash)
    }

    pub fn verify_password(
        &self,
        plaintext: &str,
        stored_hash: &str,
    ) -> Result<bool, CryptoApplicationError> {
        let parsed_hash = PasswordHash::new(stored_hash)
            .map_err(|err| CryptoApplicationError::PasswordHashParse(err.to_string()))?;
        match self
            .argon2id()
            .verify_password(plaintext.as_bytes(), &parsed_hash)
        {
            Ok(()) => Ok(true),
            Err(PasswordHashError::PasswordInvalid) => Ok(false),
            Err(error) => Err(CryptoApplicationError::PasswordHash(error.to_string())),
        }
    }

    pub fn issue_access_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        self.issue_token(user_id, self.jwt_params.access_ttl_secs, "at+jwt")
    }

    pub fn issue_refresh_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        self.issue_token(user_id, self.jwt_params.refresh_ttl_secs, "refresh+jwt")
    }

    pub fn verify_access_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        self.verify_token(token, "at+jwt")
    }

    pub fn verify_refresh_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        self.verify_token(token, "refresh+jwt")
    }

    fn issue_token(
        &self,
        user_id: String,
        ttl_secs: u64,
        typ: &str,
    ) -> Result<String, CryptoApplicationError> {
        let now: u64 = Timestamp::now()
            .as_second()
            .try_into()
            .expect("system clock is before UNIX_EPOCH");
        let claims = Claims {
            iss: self.jwt_params.issuer.clone(),
            sub: user_id,
            aud: Audience::Single(self.jwt_params.audience.clone()),
            iat: now,
            nbf: now,
            exp: now + ttl_secs,
            jti: Uuid::new_v4().to_string(),
        };
        let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
        header.typ = Some(typ.to_owned());

        encode(&header, &claims, &self.jwt_params.encoding_key)
            .map_err(|err| CryptoApplicationError::Internal(err.into()))
    }

    fn verify_token(
        &self,
        token: &str,
        expected_typ: &str,
    ) -> Result<String, CryptoApplicationError> {
        let token_data = decode::<Claims>(
            token,
            &self.jwt_params.decoding_key,
            &self.jwt_params.validation,
        )
        .map_err(map_jwt_verify_error)?;

        if token_data.header.typ.as_deref() != Some(expected_typ) {
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

impl IdentityCrypto for CryptoService {
    fn hash_password(&self, plaintext: &str) -> Result<String, CryptoApplicationError> {
        Self::hash_password(self, plaintext)
    }

    fn verify_password(
        &self,
        plaintext: &str,
        stored_hash: &str,
    ) -> Result<bool, CryptoApplicationError> {
        Self::verify_password(self, plaintext, stored_hash)
    }

    fn issue_access_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        Self::issue_access_token(self, user_id)
    }

    fn issue_refresh_token(&self, user_id: String) -> Result<String, CryptoApplicationError> {
        Self::issue_refresh_token(self, user_id)
    }

    fn verify_access_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        Self::verify_access_token(self, token)
    }

    fn verify_refresh_token(&self, token: &str) -> Result<String, CryptoApplicationError> {
        Self::verify_refresh_token(self, token)
    }
}

fn map_jwt_verify_error(error: jsonwebtoken::errors::Error) -> CryptoApplicationError {
    use jsonwebtoken::errors::ErrorKind;

    match error.kind() {
        ErrorKind::InvalidToken
        | ErrorKind::InvalidSignature
        | ErrorKind::InvalidAlgorithmName
        | ErrorKind::MissingRequiredClaim(_)
        | ErrorKind::InvalidClaimFormat(_)
        | ErrorKind::ExpiredSignature
        | ErrorKind::InvalidIssuer
        | ErrorKind::InvalidAudience
        | ErrorKind::InvalidSubject
        | ErrorKind::ImmatureSignature
        | ErrorKind::InvalidAlgorithm
        | ErrorKind::Base64(_)
        | ErrorKind::Json(_)
        | ErrorKind::Utf8(_) => CryptoApplicationError::InvalidToken,
        _ => CryptoApplicationError::Internal(error.into()),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    aud: Audience,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}
