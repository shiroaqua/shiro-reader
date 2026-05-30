use derive_more::{AsRef, Deref, Display, From};
use uuid::Uuid;

use crate::domain::library::bookshelf::folder::errors::FolderDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct FolderId(Uuid);

impl FolderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn root() -> Self {
        Self(Uuid::nil())
    }

    pub fn parse_folder_id(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        let id = Self::parse(value)?;
        if id.is_nil() {
            Err(FolderDomainError::InvalidId)
        } else {
            Ok(id)
        }
    }

    pub fn parse_parent_id(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        Self::parse(value).map_err(|e| {
            if e == FolderDomainError::InvalidId {
                FolderDomainError::InvalidParentId
            } else {
                e
            }
        })
    }

    pub fn is_root(&self) -> bool {
        self.is_nil()
    }
}

impl FolderId {
    fn parse(value: impl AsRef<str>) -> Result<Self, FolderDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(FolderDomainError::MissingId);
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

        if !raw.chars().all(|c| c.is_alphanumeric()) {
            return Err(FolderDomainError::InvalidNameFormat);
        }

        Ok(Self(raw.to_string()))
    }
}
