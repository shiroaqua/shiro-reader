use std::fmt;

use argon2::{Params, password_hash::phc::PasswordHash};
use derive_more::{AsRef, Display, From};
use uuid::Uuid;

use crate::domain::identity::user::errors::*;

// ==================== UserId ====================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, toasty::Embed)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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

// ==================== UserName ====================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, toasty::Embed)]
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

// ==================== UserPasswordHash ====================

// 不派生 Display、Debug 或 From，避免意外输出密码哈希或绕过校验。
#[derive(Clone, PartialEq, Eq, toasty::Embed)]
pub struct UserPasswordHash(String);

impl UserPasswordHash {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();

        // 解析 PHC 字符串，校验编码格式。
        let phc = PasswordHash::new(raw).map_err(|_| UserDomainError::InvalidPasswordHash)?;

        // 只接受 Argon2id。
        if phc.algorithm.as_str() != "argon2id" {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        // 要求显式提供受支持的版本。
        if !matches!(phc.version, Some(16) | Some(19)) {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        // 必须显式包含内存成本、时间成本、并行度参数。
        for required in ["m", "t", "p"] {
            if !phc.params.iter().any(|(name, _)| name.as_str() == required) {
                return Err(UserDomainError::InvalidPasswordHash);
            }
        }

        // 必须包含盐值
        let salt = phc
            .salt
            .as_ref()
            .ok_or(UserDomainError::InvalidPasswordHash)?;
        let salt_bytes: &[u8] = salt.as_ref();
        if salt_bytes.len() < argon2::MIN_SALT_LEN {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        // 必须包含哈希输出，而不能只有算法参数和盐值。
        if phc.hash.is_none() {
            return Err(UserDomainError::InvalidPasswordHash);
        }

        // 校验 Argon2 参数的合法性，包括参数名、数值范围、
        // 内存成本与并行度之间的约束，以及哈希输出长度。
        Params::try_from(&phc).map_err(|_| UserDomainError::InvalidPasswordHash)?;
        Ok(Self(raw.to_owned()))
    }

    /// 显式获取编码，用于持久化或验证密码。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for UserPasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UserPasswordHash([REDACTED])")
    }
}
