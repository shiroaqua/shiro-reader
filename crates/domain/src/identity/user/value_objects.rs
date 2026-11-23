use std::fmt;

use argon2::{Params, password_hash::phc::PasswordHash};
use derive_more::{AsRef, Display, From, Into};
use uuid::Uuid;

use crate::identity::user::errors::UserDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Into)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let parsed = Uuid::parse_str(value.as_ref()).map_err(|_| UserDomainError::InvalidId)?;
        Ok(Self(parsed))
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct UserName(String);

impl UserName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();

        if raw.is_empty() {
            return Err(UserDomainError::MissingUserName);
        }

        // 允许 Unicode 字母和数字，包括中文。
        // 不允许空格、标点、下划线、emoji 等。
        if !raw.chars().all(char::is_alphanumeric) {
            return Err(UserDomainError::InvalidUserNameFormat);
        }

        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// 不派生 Display 或 From，避免意外输出密码哈希或绕过校验。
#[derive(Clone, PartialEq, Eq)]
pub struct UserPasswordHash(String);

impl UserPasswordHash {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();

        let phc = PasswordHash::new(raw).map_err(|_| UserDomainError::InvalidPasswordHash)?;

        if phc.algorithm.as_str() != "argon2id" {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        if !matches!(phc.version, Some(16) | Some(19)) {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        for required in ["m", "t", "p"] {
            if !phc.params.iter().any(|(name, _)| name.as_str() == required) {
                return Err(UserDomainError::InvalidPasswordHash);
            }
        }

        let salt = phc
            .salt
            .as_ref()
            .ok_or(UserDomainError::InvalidPasswordHash)?;
        let salt_bytes: &[u8] = salt.as_ref();
        if salt_bytes.len() < argon2::MIN_SALT_LEN {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        if phc.hash.is_none() {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        Params::try_from(&phc).map_err(|_| UserDomainError::InvalidPasswordHash)?;
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for UserPasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UserPasswordHash([REDACTED])")
    }
}
