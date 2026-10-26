#[derive(Debug, thiserror::Error)]
pub enum CryptoApplicationError {
    #[error("Salt generation error")]
    SaltGeneration,

    #[error("Password hash parse error: {0}")]
    PasswordHashParse(#[from] argon2::password_hash::phc::Error),

    #[error("Password hash error: {0}")]
    PasswordHash(#[from] argon2::password_hash::Error),

    #[error("JSON Web Token error: {0}")]
    JsonWebToken(#[from] jsonwebtoken::errors::Error),

    #[error("Invalid JSON Web Token type")]
    InvalidTokenType,
}
