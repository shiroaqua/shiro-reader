#[derive(Debug, thiserror::Error)]
pub enum AuthenticationApplicationError {
    #[error("Invalid credentials error")]
    InvalidCredentials,

    #[error("Missing bearer token")]
    MissingBearerToken,

    #[error("User not found")]
    NotFound
}
