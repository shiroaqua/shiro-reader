use std::str::FromStr;

use derive_more::{Display, From, AsRef, Deref};
use uuid::Uuid;

use crate::domain::library::bookshelf::folder::errors::FolderDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct FolderId(Uuid);

impl FolderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse_folder_id(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        Self::parse(value).map_err(|_| FolderDomainError::InvalidFolderId)
    }

    pub fn parse_parent_id(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        Self::parse(value).map_err(|_| FolderDomainError::InvalidParentFolderId)
    }
}

impl FolderId {
    fn parse(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(FolderDomainError::InvalidFolderId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| FolderDomainError::InvalidFolderId)?;
        Ok(Self(parsed))
    }
}

impl Default for FolderId {
    fn default() -> Self {
        Self::new()
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Display, From, AsRef, Deref)]
pub struct FolderName(String);

impl FolderName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(FolderDomainError::MissingFolderName);
        }

        if !raw.chars().all(|c| c.is_alphanumeric())  {
            return Err(FolderDomainError::InvalidFolderNameFormat);
        }

        Ok(Self(raw.to_string()))
    }
}

impl FromStr for FolderName {
    type Err = FolderDomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}