use bitflags::bitflags;
use derive_more::{AsRef, Display, Into};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookNavigationError {
    #[error("normalized offset must be finite and within [0.0, 1.0]")]
    InvalidNormalizedOffset,

    #[error("progression must be finite and within [0.0, 1.0]")]
    InvalidProgression,

    #[error("invalid epub href")]
    InvalidEpubHref,

    #[error("invalid epub fragment")]
    InvalidFragment,

    #[error("invalid epub cfi")]
    InvalidCfi,

    #[error("invalid pdf destination coordinate")]
    InvalidPdfCoordinate,

    #[error("linear unit index {index} out of bounds (unit count: {unit_count})")]
    UnitIndexOutOfBounds { index: u32, unit_count: usize },

    #[error("too many linear units")]
    TooManyLinearUnits,

    #[error("too many toc nodes")]
    TooManyTocNodes,

    #[error("maximum toc depth exceeded")]
    MaxDepthExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct NormalizedOffset(f32);

impl NormalizedOffset {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Result<Self, BookNavigationError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(BookNavigationError::InvalidNormalizedOffset);
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

impl Default for NormalizedOffset {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Progression(f64);

impl Progression {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f64) -> Result<Self, BookNavigationError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(BookNavigationError::InvalidProgression);
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

impl Default for Progression {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Canonical package-relative EPUB path (e.g. `"OEBPS/Text/ch01.xhtml"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubHref(String);

impl EpubHref {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, BookNavigationError> {
        let s = raw.as_ref();
        if s.is_empty()
            || s.starts_with('/')
            || s.ends_with('/')
            || s.contains("//")
            || s.chars().any(|c| matches!(c, '\\' | '#' | '?' | '\0'))
        {
            return Err(BookNavigationError::InvalidEpubHref);
        }
        if s.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..") {
            return Err(BookNavigationError::InvalidEpubHref);
        }
        Ok(Self(s.to_owned()))
    }

    pub fn normalize(raw: impl AsRef<str>) -> Result<Self, BookNavigationError> {
        let s = raw.as_ref();
        if s.chars().any(|c| matches!(c, '\\' | '#' | '?' | '\0')) {
            return Err(BookNavigationError::InvalidEpubHref);
        }
        let mut segments = Vec::new();
        for seg in s.trim_start_matches('/').split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    segments.pop().ok_or(BookNavigationError::InvalidEpubHref)?;
                }
                _ => segments.push(seg),
            }
        }
        if segments.is_empty() {
            return Err(BookNavigationError::InvalidEpubHref);
        }
        Ok(Self(segments.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TocNodeFlags: u16 {
        /// Node is collapsed by default (e.g. PDF outline `/Count < 0`).
        const DEFAULT_COLLAPSED = 1 << 0;
        /// Node points to an external link/URI rather than an in-book location.
        const EXTERNAL_LINK = 1 << 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PdfDest {
    PageOnly,
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
        /// Pre-computed distance ratio from top edge of page `[0.0, 1.0]`.
        normalized_top: Option<NormalizedOffset>,
    },
    FitH {
        top: Option<f32>,
        normalized_top: Option<NormalizedOffset>,
    },
    FitPage,
}

impl PdfDest {
    pub fn xyz(
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
        normalized_top: Option<NormalizedOffset>,
    ) -> Result<Self, BookNavigationError> {
        let dest = Self::Xyz {
            left,
            top,
            zoom,
            normalized_top,
        };
        dest.validate()?;
        Ok(dest)
    }

    pub fn fit_h(
        top: Option<f32>,
        normalized_top: Option<NormalizedOffset>,
    ) -> Result<Self, BookNavigationError> {
        let dest = Self::FitH {
            top,
            normalized_top,
        };
        dest.validate()?;
        Ok(dest)
    }

    pub fn validate(&self) -> Result<(), BookNavigationError> {
        match *self {
            Self::PageOnly | Self::FitPage => Ok(()),
            Self::Xyz {
                left, top, zoom, ..
            } => {
                if left.is_some_and(|v| !v.is_finite())
                    || top.is_some_and(|v| !v.is_finite())
                    || zoom.is_some_and(|v| !v.is_finite() || v <= 0.0)
                {
                    return Err(BookNavigationError::InvalidPdfCoordinate);
                }
                Ok(())
            }
            Self::FitH { top, .. } => {
                if top.is_some_and(|v| !v.is_finite()) {
                    return Err(BookNavigationError::InvalidPdfCoordinate);
                }
                Ok(())
            }
        }
    }

    pub const fn normalized_top(&self) -> Option<NormalizedOffset> {
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
        dest: PdfDest,
    },
    Epub {
        /// Resolved Spine index (`None` if pointing to a non-spine/non-linear manifest item).
        spine_index: Option<u32>,
        /// Canonical package-relative path (e.g. `"OEBPS/Text/ch01.xhtml"`).
        href: EpubHref,
        /// URL anchor ID (without `'#'`).
        fragment: Option<String>,
        /// Optional high-precision EPUB CFI.
        cfi: Option<String>,
    },
}

impl BookLocator {
    pub fn pdf(page_index: u32, dest: PdfDest) -> Result<Self, BookNavigationError> {
        dest.validate()?;
        Ok(Self::Pdf { page_index, dest })
    }

    pub fn epub(
        spine_index: Option<u32>,
        href: EpubHref,
        fragment: Option<String>,
        cfi: Option<String>,
    ) -> Result<Self, BookNavigationError> {
        let loc = Self::Epub {
            spine_index,
            href,
            fragment,
            cfi,
        };
        loc.validate()?;
        Ok(loc)
    }

    pub fn validate(&self) -> Result<(), BookNavigationError> {
        match self {
            Self::Pdf { dest, .. } => dest.validate(),
            Self::Epub {
                href,
                fragment,
                cfi,
                ..
            } => {
                EpubHref::parse(href.as_str())?;
                if let Some(frag) = fragment
                    && (frag.is_empty() || frag.starts_with('#'))
                {
                    return Err(BookNavigationError::InvalidFragment);
                }
                if let Some(cfi) = cfi
                    && cfi.trim().is_empty()
                {
                    return Err(BookNavigationError::InvalidCfi);
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

    pub const fn default_unit_offset(&self) -> NormalizedOffset {
        match self {
            Self::Pdf { dest, .. } => match dest.normalized_top() {
                Some(offset) => offset,
                None => NormalizedOffset::ZERO,
            },
            Self::Epub { .. } => NormalizedOffset::ZERO,
        }
    }
}
