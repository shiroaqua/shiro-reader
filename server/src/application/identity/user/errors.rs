#[derive(Debug, thiserror::Error)]
pub enum UserApplicationError {
    #[error("Invalid user ID format")]
    InvalidId,

    #[error("Username cannot be empty")]
    MissingUserName,

    #[error("Username can only contain letters and numbers")]
    InvalidUserNameFormat,

    #[error("Password hash must be a valid Argon2 PHC string")]
    InvalidPasswordHash,

    #[error("User not found")]
    NotFound,

    #[error("User name conflict")]
    NameConflict,

    #[error("User version conflict")]
    VersionConflict,

    #[error("Unchanged user name")]
    NameUnchanged,

    #[error("Unchanged password hash")]
    PasswordHashUnchanged,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

