use thiserror::Error;

use crate::application::identity::authentication::errors::AuthenticationApplicationError;
use crate::application::identity::crypto::errors::CryptoApplicationError;
use crate::application::identity::user::errors::UserApplicationError;
use crate::domain::identity::user::errors::UserDomainError;
use crate::infrastructure::repositories::errors::UserRepositoryError;

#[derive(Debug, Error)]
pub enum IdentityApplicationError {
    #[error("User error: {0}")]
    User(#[from] UserApplicationError),

    #[error("Authentication error: {0}")]
    Authentication(#[from] AuthenticationApplicationError),
    
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoApplicationError),
}

impl From<UserDomainError> for UserApplicationError {
    fn from(err: UserDomainError) -> Self {
        match err {
            UserDomainError::InvalidId => Self::InvalidId,
            UserDomainError::MissingUserName => Self::MissingUserName,
            UserDomainError::InvalidUserNameFormat => Self::InvalidUserNameFormat,
            UserDomainError::MissingEmail => Self::MissingEmail,
            UserDomainError::InvalidEmailFormat => Self::InvalidEmailFormat,
            UserDomainError::MissingPhone => Self::MissingPhone,
            UserDomainError::InvalidPhoneFormat => Self::InvalidPhoneFormat,
            UserDomainError::InvalidPhoneExtension => Self::InvalidPhoneExtension,
            UserDomainError::InvalidPasswordHash => Self::InvalidPasswordHash,
            UserDomainError::NameUnchanged => Self::NameUnchanged,
            UserDomainError::PasswordHashUnchanged => Self::PasswordHashUnchanged,
        }
    }
}

impl From<UserDomainError> for IdentityApplicationError {
    fn from(err: UserDomainError) -> Self {
        Self::User(err.into())
    }
}

impl From<UserRepositoryError> for IdentityApplicationError {
    fn from(err: UserRepositoryError) -> Self {
        match err {
            UserRepositoryError::UserNotFound => Self::User(UserApplicationError::NotFound),
            UserRepositoryError::UserNameConflict => Self::User(UserApplicationError::NameConflict),
            UserRepositoryError::UserVersionConflict => Self::User(UserApplicationError::VersionConflict),
            UserRepositoryError::Storage(error) => Self::User(UserApplicationError::Storage(error)),
        }
    }
}
