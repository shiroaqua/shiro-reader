#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UserDomainError {
    #[error("Invalid user ID format")]
    InvalidId,

    #[error("Username cannot be empty")]
    MissingUserName,

    #[error("Username can only contain letters and numbers")]
    InvalidUserNameFormat,

    #[error("Email cannot be empty")]
    MissingEmail,

    #[error("Invalid email format")]
    InvalidEmailFormat,

    #[error("Phone number cannot be empty")]
    MissingPhone,

    #[error("Invalid phone number format")]
    InvalidPhoneFormat,

    #[error("Extension must be a non-empty ASCII digit string")]
    InvalidPhoneExtension,

    #[error("Password hash must be a valid Argon2 PHC string")]
    InvalidPasswordHash,

    #[error("Unchanged user name")]
    NameUnchanged,
    
    #[error("Unchanged password hash")]
    PasswordHashUnchanged,
}