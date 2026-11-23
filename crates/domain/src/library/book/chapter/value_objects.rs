use derive_more::{AsRef, Display, Into};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::errors::ChapterDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChapterId(Uuid);

impl ChapterId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, ChapterDomainError> {
        Uuid::parse_str(value.as_ref())
            .map(Self)
            .map_err(|_| ChapterDomainError::InvalidId)
    }
}

impl Default for ChapterId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into, Serialize)]
#[serde(transparent)]
pub struct ChapterTitle(String);

impl ChapterTitle {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ChapterDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() || raw.trim() != raw {
            return Err(ChapterDomainError::InvalidTitle);
        }

        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ChapterTitle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChapterLocation {
    Pdf {
        /// One-based page number, inclusive.
        start_page: u32,
        /// One-based page number, inclusive.
        end_page: u32,
    },
    Epub {
        /// Resource path relative to the EPUB package root.
        href: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cfi: Option<String>,
    },
}

impl ChapterLocation {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ChapterDomainError> {
        let location = serde_json::from_str::<Self>(value.as_ref())
            .map_err(|_| ChapterDomainError::InvalidLocation)?;
        if !location.is_valid() {
            return Err(ChapterDomainError::InvalidLocation);
        }
        Ok(location)
    }

    pub(super) fn is_valid(&self) -> bool {
        match self {
            Self::Pdf {
                start_page,
                end_page,
            } => *start_page > 0 && start_page <= end_page,
            Self::Epub { href, cfi } => {
                !href.trim().is_empty() && cfi.as_ref().is_none_or(|value| !value.is_empty())
            }
        }
    }
}
