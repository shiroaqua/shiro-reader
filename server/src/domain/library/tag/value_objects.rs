use std::sync::LazyLock;

use derive_more::{AsRef, Display};
use jiff::Timestamp;
use regex::Regex;
use uuid::Uuid;

use super::errors::TagDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookTagStatus {
    Active,
    Deleted { deleted_at: Timestamp },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, toasty::Embed)]
pub struct TagId(Uuid);

impl TagId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, TagDomainError> {
        Uuid::parse_str(value.as_ref())
            .map(Self)
            .map_err(|_| TagDomainError::InvalidId)
    }
}

impl Default for TagId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, AsRef, toasty::Embed)]
pub struct TagName(String);

impl TagName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TagDomainError> {
        static NAME_REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\A[\p{L}\p{N}.]+\z").expect("valid tag name regex"));

        let value = value.as_ref();
        if value.is_empty() {
            return Err(TagDomainError::MissingName);
        }
        // 不修剪空白；除用于翻译 key 分段的点号外，符号、标点和空白均不允许。
        if !NAME_REGEX.is_match(value) {
            return Err(TagDomainError::InvalidNameFormat);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, AsRef, toasty::Embed)]
pub struct TagColor(String);

impl TagColor {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TagDomainError> {
        let value = value.as_ref();
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
            return Err(TagDomainError::InvalidColor);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Debug, Clone)]
pub struct TagValueRule {
    allows_value: bool,
    value_regex: Option<String>,
    compiled_regex: Option<Regex>,
}

impl TagValueRule {
    pub fn new(allows_value: bool, value_regex: Option<String>) -> Result<Self, TagDomainError> {
        if !allows_value && value_regex.is_some() {
            return Err(TagDomainError::ValueRegexNotAllowed);
        }

        let value_regex = value_regex.filter(|pattern| !pattern.is_empty());
        // 正则限制匹配整个标签值；空正则表示不限制非空字符串的内容。
        let compiled_regex = value_regex
            .as_ref()
            .map(|pattern| {
                // 先校验原始表达式，避免不配对的括号逃逸完整匹配的分组。
                Regex::new(pattern)?;
                Regex::new(&format!(r"\A(?:{pattern})\z")).or_else(|_| {
                    // (?x) 模式末尾的注释需要换行结束，才能闭合外围分组。
                    Regex::new(&format!("\\A(?:{pattern}\n)\\z"))
                })
            })
            .transpose()
            .map_err(|_| TagDomainError::InvalidValueRegex)?;

        Ok(Self {
            allows_value,
            value_regex,
            compiled_regex,
        })
    }

    pub fn allows_value(&self) -> bool {
        self.allows_value
    }

    pub fn value_regex(&self) -> Option<&str> {
        self.value_regex.as_deref()
    }

    pub fn validate(&self, value: Option<&str>) -> Result<(), TagDomainError> {
        if !self.allows_value {
            return if value.is_some() {
                Err(TagDomainError::ValueNotAllowed)
            } else {
                Ok(())
            };
        }

        let value = value
            .filter(|value| !value.is_empty())
            .ok_or(TagDomainError::ValueRequired)?;
        if self
            .compiled_regex
            .as_ref()
            .is_some_and(|regex| !regex.is_match(value))
        {
            return Err(TagDomainError::ValueRegexMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagDeleteStrategy {
    Cascade,
    Promote,
}
