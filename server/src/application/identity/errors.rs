use thiserror::Error;

use crate::application::identity::authentication::errors::AuthenticationApplicationError;
use crate::application::identity::crypto::errors::CryptoApplicationError;
use crate::application::identity::user::errors::UserApplicationError;
use crate::domain::identity::user::errors::UserDomainError;
use crate::infrastructure::db::sqlite::sqlite_error;

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
            UserDomainError::InvalidPasswordHash => Self::InvalidPasswordHash,
            UserDomainError::NameUnchanged => Self::NameUnchanged,
            UserDomainError::PasswordHashUnchanged => Self::PasswordHashUnchanged,
            UserDomainError::InvalidTimestamp => {
                Self::Storage(anyhow::anyhow!("invalid user timestamp"))
            }
        }
    }
}

impl From<UserDomainError> for IdentityApplicationError {
    fn from(err: UserDomainError) -> Self {
        Self::User(err.into())
    }
}

pub(crate) fn user_storage(error: toasty::Error) -> IdentityApplicationError {
    if error.is_record_not_found() {
        return UserApplicationError::NotFound.into();
    }
    if let Some(rusqlite::Error::SqliteFailure(code, Some(message))) = sqlite_error(&error) {
        if code.extended_code == 2067 && message.contains("users.name") {
            return UserApplicationError::NameConflict.into();
        }
    }
    UserApplicationError::Storage(error.into()).into()
}
