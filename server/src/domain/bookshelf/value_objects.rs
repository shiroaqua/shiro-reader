use std::str::FromStr;

use derive_more::{Display, From, AsRef, Deref};
use uuid::Uuid;

use crate::domain::bookshelf::errors::DirectoryDomainError;

pub const ROOT_DIRECTORY_ID: &str = "00000000-0000-0000-0000-000000000000";
pub const ROOT_DIRECTORY_NAME: &str = "$ROOT";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct DirectoryId(String);

impl DirectoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn root() -> Self {
        Self(ROOT_DIRECTORY_ID.to_owned())
    }

    pub fn parse_directory_id(value: impl AsRef<str>) -> Result<Self, DirectoryDomainError> {
        Self::parse(value).map_err(|_| DirectoryDomainError::InvalidDirectoryId)
    }

    pub fn parse_parent_id(value: impl AsRef<str>) -> Result<Self, DirectoryDomainError> {
        Self::parse(value).map_err(|_| DirectoryDomainError::InvalidParentId)
    }

    pub fn is_root(&self) -> bool {
        self.0 == ROOT_DIRECTORY_ID
    }
}

impl DirectoryId {
    fn parse(value: impl AsRef<str>) -> Result<Self, DirectoryDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(DirectoryDomainError::DirectoryIdRequired);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| DirectoryDomainError::InvalidDirectoryId)?;
        Ok(Self(parsed.to_string()))
    }
}

impl Default for DirectoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for DirectoryId {
    type Err = DirectoryDomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}



#[derive(Debug, Clone, PartialEq, Eq, Display, From, AsRef, Deref)]
pub struct DirectoryName(String);

impl DirectoryName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DirectoryDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(DirectoryDomainError::DirectoryNameRequired);
        }

        if raw == ROOT_DIRECTORY_NAME {
            return Err(DirectoryDomainError::DirectoryNameReserved);
        }

        if !raw.chars().all(|c| c.is_alphanumeric())  {
            return Err(DirectoryDomainError::DirectoryNameInvalidFormat);
        }

        Ok(Self(raw.to_string()))
    }

    pub fn root() -> Self {
        Self(ROOT_DIRECTORY_NAME.to_owned())
    }
}

impl FromStr for DirectoryName {
    type Err = DirectoryDomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}