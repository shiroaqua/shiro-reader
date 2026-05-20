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
        Self::parse(value).map_err(|_| FolderDomainError::InvalidId)
    }

    pub fn parse_parent_id(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        Self::parse(value).map_err(|_| FolderDomainError::InvalidParentId)
    }
}

impl FolderId {
    fn parse(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(FolderDomainError::InvalidId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| FolderDomainError::InvalidId)?;
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
            return Err(FolderDomainError::MissingName);
        }

        if !raw.chars().all(|c| c.is_alphanumeric())  {
            return Err(FolderDomainError::InvalidNameFormat);
        }

        Ok(Self(raw.to_string()))
    }
}
