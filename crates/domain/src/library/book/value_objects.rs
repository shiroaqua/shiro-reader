use bitflags::bitflags;
use derive_more::{AsRef, Display, From, Into};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::book::errors::BookDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Into, Serialize, Deserialize)]
pub struct BookId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookTitle(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Into)]
pub struct BookHash(blake3::Hash);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookStatus {
    Active,
    Deleted { deleted_at: Timestamp },
}

impl BookHash {
    pub const fn new(hash: blake3::Hash) -> Self {
        Self(hash)
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(blake3::Hash::from_bytes(bytes))
    }

    pub const fn as_hash(&self) -> &blake3::Hash {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl From<blake3::Hash> for BookHash {
    fn from(hash: blake3::Hash) -> Self {
        Self(hash)
    }
}

impl BookId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::InvalidId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookDomainError::InvalidId)?;
        Ok(Self(parsed))
    }
}

impl Default for BookId {
    fn default() -> Self {
        Self::new()
    }
}

impl BookTitle {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::MissingTitle);
        }

        if raw.trim() != raw {
            return Err(BookDomainError::InvalidTitleFormat);
        }

        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}


#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ReadingUnitOffset(f32);

impl ReadingUnitOffset {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Result<Self, BookDomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(BookDomainError::InvalidReadingUnitOffset);
        }
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub fn clamped(value: f32) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        let clamped = value.clamp(0.0, 1.0);
        Some(Self(if clamped == 0.0 { 0.0 } else { clamped }))
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

impl Default for ReadingUnitOffset {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BookProgression(f64);

impl BookProgression {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f64) -> Result<Self, BookDomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(BookDomainError::InvalidBookProgression);
        }
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub fn clamped(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        let clamped = value.clamp(0.0, 1.0);
        Some(Self(if clamped == 0.0 { 0.0 } else { clamped }))
    }

    pub const fn value(self) -> f64 {
        self.0
    }
}

impl Default for BookProgression {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Canonical package-relative EPUB path (e.g. `"OEBPS/Text/ch01.xhtml"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubResourcePath(String);

impl EpubResourcePath {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let s = raw.as_ref();
        if s.is_empty()
            || s.starts_with('/')
            || s.ends_with('/')
            || s.contains("//")
            || s.chars().any(|c| matches!(c, '\\' | '#' | '?' | '\0'))
        {
            return Err(BookDomainError::InvalidEpubResourcePath);
        }
        if s.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..") {
            return Err(BookDomainError::InvalidEpubResourcePath);
        }
        Ok(Self(s.to_owned()))
    }

    pub fn normalize(raw: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let s = raw.as_ref();
        if s.ends_with('/') || s.chars().any(|c| matches!(c, '\\' | '#' | '?' | '\0')) {
            return Err(BookDomainError::InvalidEpubResourcePath);
        }
        let mut segments = Vec::new();
        for seg in s.trim_start_matches('/').split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    segments.pop().ok_or(BookDomainError::InvalidEpubResourcePath)?;
                }
                _ => segments.push(seg),
            }
        }
        if segments.is_empty() {
            return Err(BookDomainError::InvalidEpubResourcePath);
        }
        Ok(Self(segments.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TocEntryFlags: u16 {
        /// Entry is collapsed by default (e.g. PDF outline `/Count < 0`).
        const DEFAULT_COLLAPSED = 1 << 0;
        /// Entry points to an external link/URI rather than an in-book location.
        const EXTERNAL_LINK = 1 << 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PdfDestination {
    PageOnly,
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
        /// Pre-computed distance ratio from top edge of page `[0.0, 1.0]`.
        normalized_top: Option<ReadingUnitOffset>,
    },
    FitH {
        top: Option<f32>,
        normalized_top: Option<ReadingUnitOffset>,
    },
    FitPage,
}

impl PdfDestination {
    pub fn xyz(
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
        normalized_top: Option<ReadingUnitOffset>,
    ) -> Result<Self, BookDomainError> {
        let destination = Self::Xyz {
            left,
            top,
            zoom,
            normalized_top,
        };
        destination.validate()?;
        Ok(destination)
    }

    pub fn fit_h(
        top: Option<f32>,
        normalized_top: Option<ReadingUnitOffset>,
    ) -> Result<Self, BookDomainError> {
        let destination = Self::FitH {
            top,
            normalized_top,
        };
        destination.validate()?;
        Ok(destination)
    }

    pub fn validate(&self) -> Result<(), BookDomainError> {
        match *self {
            Self::PageOnly | Self::FitPage => Ok(()),
            Self::Xyz {
                left, top, zoom, ..
            } => {
                if left.is_some_and(|v| !v.is_finite())
                    || top.is_some_and(|v| !v.is_finite())
                    || zoom.is_some_and(|v| !v.is_finite() || v <= 0.0)
                {
                    return Err(BookDomainError::InvalidPdfDestination);
                }
                Ok(())
            }
            Self::FitH { top, .. } => {
                if top.is_some_and(|v| !v.is_finite()) {
                    return Err(BookDomainError::InvalidPdfDestination);
                }
                Ok(())
            }
        }
    }

    pub const fn normalized_top(&self) -> Option<ReadingUnitOffset> {
        match *self {
            Self::Xyz { normalized_top, .. } | Self::FitH { normalized_top, .. } => normalized_top,
            Self::PageOnly | Self::FitPage => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BookLocator {
    Pdf {
        /// Always uses 0-based physical page index, never printed page labels.
        page_index: u32,
        /// Target viewport details (raw coordinates + normalized top ratio).
        destination: PdfDestination,
    },
    Epub {
        /// Resolved Spine index (`None` if pointing to a non-spine/non-linear manifest item).
        spine_index: Option<u32>,
        /// Canonical package-relative path (e.g. `"OEBPS/Text/ch01.xhtml"`).
        path: EpubResourcePath,
        /// URL anchor ID (without `'#'`).
        fragment: Option<String>,
        /// Optional high-precision EPUB CFI.
        cfi: Option<String>,
    },
}

impl BookLocator {
    pub fn pdf(page_index: u32, destination: PdfDestination) -> Result<Self, BookDomainError> {
        destination.validate()?;
        Ok(Self::Pdf {
            page_index,
            destination,
        })
    }

    pub fn epub(
        spine_index: Option<u32>,
        path: EpubResourcePath,
        fragment: Option<String>,
        cfi: Option<String>,
    ) -> Result<Self, BookDomainError> {
        let loc = Self::Epub {
            spine_index,
            path,
            fragment,
            cfi,
        };
        loc.validate()?;
        Ok(loc)
    }

    pub fn validate(&self) -> Result<(), BookDomainError> {
        match self {
            Self::Pdf { destination, .. } => destination.validate(),
            Self::Epub {
                path,
                fragment,
                cfi,
                ..
            } => {
                EpubResourcePath::parse(path.as_str())?;
                if let Some(frag) = fragment
                    && (frag.is_empty() || frag.starts_with('#'))
                {
                    return Err(BookDomainError::InvalidEpubFragment);
                }
                if let Some(cfi) = cfi
                    && cfi.trim().is_empty()
                {
                    return Err(BookDomainError::InvalidEpubCfi);
                }
                Ok(())
            }
        }
    }

    pub const fn unit_index(&self) -> Option<u32> {
        match self {
            Self::Pdf { page_index, .. } => Some(*page_index),
            Self::Epub { spine_index, .. } => *spine_index,
        }
    }

    pub const fn default_unit_offset(&self) -> ReadingUnitOffset {
        match self {
            Self::Pdf { destination, .. } => match destination.normalized_top() {
                Some(offset) => offset,
                None => ReadingUnitOffset::ZERO,
            },
            Self::Epub { .. } => ReadingUnitOffset::ZERO,
        }
    }
}