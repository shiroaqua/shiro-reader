use std::fmt;

use argon2::{Params, password_hash::phc::PasswordHash};
use derive_more::{AsRef, Display, From};
use email_address::EmailAddress;
use phonenumber::{Mode, country};
use uuid::Uuid;

use crate::domain::identity::user::errors::*;

// ==================== UserId ====================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef)]
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

// ==================== Email ====================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef)]
pub struct Email(String);

impl Email {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();

        if raw.is_empty() {
            return Err(UserDomainError::MissingEmail);
        }

        if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(UserDomainError::InvalidEmailFormat);
        }

        if !EmailAddress::is_valid(raw) {
            return Err(UserDomainError::InvalidEmailFormat);
        }

        // 不将整个邮箱转为小写：local-part 理论上可以区分大小写。
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ==================== Phone ====================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Phone {
    /// 国际标准号码，例如：+442079460018。
    /// 包含国家号，以及该号码计划中适用的区号和用户号码。
    e164: String,

    /// 分机号，例如：0012。使用字符串保留前导零。
    extension: Option<String>,
}

impl Phone {
    /// 解析国际格式号码，要求显式提供 +国家号。
    ///
    /// 示例：+44 20 7946 0018
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        Self::parse_with_region(value, None, None)
    }

    /// 解析国际格式或本地格式号码，并可指定分机号。
    ///
    /// default_region:
    /// - None：必须使用 +国家号格式。
    /// - Some(country::GB)：允许英国本地号码，如 020 7946 0018。
    ///
    /// extension:
    /// - 单独传入，不写在主号码中。
    pub fn parse_with_region(
        value: impl AsRef<str>,
        default_region: Option<country::Id>,
        extension: Option<&str>,
    ) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();

        if raw.is_empty() {
            return Err(UserDomainError::MissingPhone);
        }

        if raw.trim() != raw {
            return Err(UserDomainError::InvalidPhoneFormat);
        }

        // 显式限制输入字符，避免解析库宽松提取号码或丢弃分机。
        // 允许常见排版：空格、括号、连字符、点。
        // 不接受主号码中的 ext、x、# 等分机标记。
        if !raw
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '(' | ')' | '-' | '.'))
        {
            return Err(UserDomainError::InvalidPhoneFormat);
        }

        if raw.contains('+') && (!raw.starts_with('+') || raw.matches('+').count() != 1) {
            return Err(UserDomainError::InvalidPhoneFormat);
        }

        if default_region.is_none() && !raw.starts_with('+') {
            return Err(UserDomainError::InvalidPhoneFormat);
        }

        let extension = match extension {
            Some(ext) => {
                if ext.is_empty() || !ext.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(UserDomainError::InvalidPhoneExtension);
                }

                Some(ext.to_owned())
            }
            None => None,
        };

        let number = phonenumber::parse(default_region, raw)
            .map_err(|_| UserDomainError::InvalidPhoneFormat)?;

        if !phonenumber::is_valid(&number) {
            return Err(UserDomainError::InvalidPhoneFormat);
        }

        let e164 = number.format().mode(Mode::E164).to_string();

        Ok(Self { e164, extension })
    }

    pub fn as_e164(&self) -> &str {
        &self.e164
    }

    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }
}

impl fmt::Display for Phone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.extension {
            Some(extension) => write!(f, "{} ext. {}", self.e164, extension),
            None => f.write_str(&self.e164),
        }
    }
}

// ==================== UserPasswordHash ====================

// 不派生 Display、Debug 或 From，避免意外输出密码哈希或绕过校验。
#[derive(Clone, PartialEq, Eq)]
pub struct UserPasswordHash(String);

impl UserPasswordHash {
    
    pub fn parse(value: impl AsRef<str>) -> Result<Self, UserDomainError> {
        let raw = value.as_ref();
   
        // 解析 PHC 字符串，校验编码格式。
        let phc = PasswordHash::new(raw)
            .map_err(|_| UserDomainError::InvalidPasswordHash)?;
   
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
            if !phc
                .params
                .iter()
                .any(|(name, _)| name.as_str() == required)
            {
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
        Params::try_from(&phc)
            .map_err(|_| UserDomainError::InvalidPasswordHash)?;
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
