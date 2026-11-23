#[derive(Debug, thiserror::Error)]
pub enum CryptoApplicationError {
    #[error("Salt generation error")]
    SaltGeneration,

    #[error("Password hash parse error: {0}")]
    PasswordHashParse(String),

    #[error("Password hash error: {0}")]
    PasswordHash(String),

    #[error("Invalid JSON Web Token")]
    InvalidToken,

    #[error("Invalid JSON Web Token type")]
    InvalidTokenType,

    #[error("JSON Web Token internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
