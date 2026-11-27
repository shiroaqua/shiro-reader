use std::borrow::Cow;

use derive_more::{AsRef, Display, Into};

use crate::library::book::value_objects::BookFileType;

use super::{errors::BookDomainError};

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
    fn validate_str(s: &str) -> Result<(), BookDomainError> {
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
        Ok(())
    }

    pub fn parse(raw: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        Self::validate_str(raw.as_ref())?;
        Ok(Self(raw.into()))
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
                    segments
                        .pop()
                        .ok_or(BookDomainError::InvalidEpubResourcePath)?;
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

    pub fn into_inner(self) -> String {
        self.0
    }
}

/// URL anchor ID within an EPUB XHTML document (without leading `'#'`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubFragment(String);

impl EpubFragment {
    fn validate_str(s: &str) -> Result<(), BookDomainError> {
        if s.is_empty()
            || s != s.trim()
            || s.starts_with('#')
            || s.chars().any(char::is_control)
        {
            return Err(BookDomainError::InvalidEpubFragment);
        }
        Ok(())
    }

    pub fn parse(raw: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        Self::validate_str(raw.as_ref())?;
        Ok(Self(raw.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CfiStructure {
    Point,
    Range {
        first_comma: usize,
        second_comma: usize,
    },
}

fn parse_cfi_structure(raw: &str) -> Result<CfiStructure, BookDomainError> {
    if raw != raw.trim() || raw.chars().any(char::is_control) {
        return Err(BookDomainError::InvalidEpubCfi);
    }
    let Some(body) = raw.strip_prefix("epubcfi(").and_then(|s| s.strip_suffix(')')) else {
        return Err(BookDomainError::InvalidEpubCfi);
    };
    if body.is_empty() || !body.starts_with('/') {
        return Err(BookDomainError::InvalidEpubCfi);
    }

    let mut in_bracket = false;
    let mut escaped = false;
    let mut prev_non_bracket: Option<char> = None;
    let mut commas = Vec::new();

    for (idx, c) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '^' {
            escaped = true;
            continue;
        }
        if c == '[' {
            if in_bracket {
                return Err(BookDomainError::InvalidEpubCfi);
            }
            in_bracket = true;
            prev_non_bracket = None;
            continue;
        }
        if c == ']' {
            if !in_bracket {
                return Err(BookDomainError::InvalidEpubCfi);
            }
            in_bracket = false;
            prev_non_bracket = None;
            continue;
        }
        if !in_bracket {
            if c.is_whitespace() {
                return Err(BookDomainError::InvalidEpubCfi);
            }
            if c == '-' && prev_non_bracket == Some('/') {
                return Err(BookDomainError::InvalidEpubCfi);
            }
            if c == ',' {
                commas.push(idx);
            }
            prev_non_bracket = Some(c);
        }
    }

    if escaped || in_bracket {
        return Err(BookDomainError::InvalidEpubCfi);
    }

    match commas.as_slice() {
        [] => Ok(CfiStructure::Point),
        [c1, c2] => {
            let parent = &body[..*c1];
            let start = &body[*c1 + 1..*c2];
            let end = &body[*c2 + 1..];

            if !parent.starts_with('/') || (start.is_empty() && end.is_empty()) {
                return Err(BookDomainError::InvalidEpubCfi);
            }

            for side in [start, end] {
                if !side.is_empty()
                    && !matches!(
                        side.chars().next(),
                        Some('/' | ':' | '[' | '@' | '~' | '!')
                    )
                {
                    return Err(BookDomainError::InvalidEpubCfi);
                }
            }

            Ok(CfiStructure::Range {
                first_comma: *c1,
                second_comma: *c2,
            })
        }
        _ => Err(BookDomainError::InvalidEpubCfi),
    }
}

/// Canonical EPUB point CFI (e.g. `"epubcfi(/6/4[ch1]!/4/2/1:12)"` or section base `"epubcfi(/6/4[ch1]!)"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubPointCfi(String);

impl EpubPointCfi {
    fn validate_str(s: &str) -> Result<(), BookDomainError> {
        match parse_cfi_structure(s)? {
            CfiStructure::Point => Ok(()),
            CfiStructure::Range { .. } => Err(BookDomainError::InvalidEpubCfi),
        }
    }

    pub fn parse(raw: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        Self::validate_str(raw.as_ref())?;
        Ok(Self(raw.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

/// Canonical EPUB range CFI (`epubcfi(parent,start,end)`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubRangeCfi(String);

impl EpubRangeCfi {
    fn validate_str(s: &str) -> Result<(), BookDomainError> {
        match parse_cfi_structure(s)? {
            CfiStructure::Range { .. } => Ok(()),
            CfiStructure::Point => Err(BookDomainError::InvalidEpubCfi),
        }
    }

    pub fn parse(raw: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        Self::validate_str(raw.as_ref())?;
        Ok(Self(raw.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    pub fn start_point(&self) -> EpubPointCfi {
        let body = &self.0["epubcfi(".len()..self.0.len() - 1];
        let CfiStructure::Range {
            first_comma,
            second_comma,
        } = parse_cfi_structure(&self.0).expect("validated range cfi")
        else {
            unreachable!("validated range cfi must remain a range");
        };
        let parent = &body[..first_comma];
        let start = &body[first_comma + 1..second_comma];
        EpubPointCfi(format!("epubcfi({parent}{start})"))
    }

    pub fn end_point(&self) -> EpubPointCfi {
        let body = &self.0["epubcfi(".len()..self.0.len() - 1];
        let CfiStructure::Range {
            first_comma,
            second_comma,
        } = parse_cfi_structure(&self.0).expect("validated range cfi")
        else {
            unreachable!("validated range cfi must remain a range");
        };
        let parent = &body[..first_comma];
        let end = &body[second_comma + 1..];
        EpubPointCfi(format!("epubcfi({parent}{end})"))
    }
}

/// Validated EPUB CFI that may represent either a point or a range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct EpubCfi(String);

impl EpubCfi {
    pub fn parse(raw: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        parse_cfi_structure(raw.as_ref())?;
        Ok(Self(raw.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    pub fn is_point(&self) -> bool {
        matches!(parse_cfi_structure(&self.0), Ok(CfiStructure::Point))
    }

    pub fn is_range(&self) -> bool {
        matches!(
            parse_cfi_structure(&self.0),
            Ok(CfiStructure::Range { .. })
        )
    }

    pub fn into_point(self) -> Result<EpubPointCfi, BookDomainError> {
        if self.is_point() {
            Ok(EpubPointCfi(self.0))
        } else {
            Err(BookDomainError::InvalidEpubCfi)
        }
    }

    pub fn into_range(self) -> Result<EpubRangeCfi, BookDomainError> {
        if self.is_range() {
            Ok(EpubRangeCfi(self.0))
        } else {
            Err(BookDomainError::InvalidEpubCfi)
        }
    }
}

impl From<EpubPointCfi> for EpubCfi {
    fn from(value: EpubPointCfi) -> Self {
        Self(value.0)
    }
}

impl From<EpubRangeCfi> for EpubCfi {
    fn from(value: EpubRangeCfi) -> Self {
        Self(value.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EpubNavTarget {
    Start,
    Fragment(EpubFragment),
    Cfi(EpubPointCfi),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EpubPointTarget {
    Cfi(EpubPointCfi),
    UnitOffset(ReadingUnitOffset),
}

impl EpubPointTarget {
    pub fn into_nav_target(self) -> EpubNavTarget {
        match self {
            Self::Cfi(cfi) => EpubNavTarget::Cfi(cfi),
            Self::UnitOffset(_) => EpubNavTarget::Start,
        }
    }

    pub fn to_nav_target(&self) -> EpubNavTarget {
        self.clone().into_nav_target()
    }

    pub const fn unit_offset(&self) -> ReadingUnitOffset {
        match self {
            Self::Cfi(_) => ReadingUnitOffset::ZERO,
            Self::UnitOffset(offset) => *offset,
        }
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

/// Page-relative normalized bounding box (`[0.0, 1.0]`, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl NormalizedRect {
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Result<Self, BookDomainError> {
        if !left.is_finite()
            || !top.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || !(0.0..=1.0).contains(&left)
            || !(0.0..=1.0).contains(&top)
            || width <= 0.0
            || height <= 0.0
            || left + width > 1.0
            || top + height > 1.0
        {
            return Err(BookDomainError::InvalidNormalizedRect);
        }
        Ok(Self {
            left: if left == 0.0 { 0.0 } else { left },
            top: if top == 0.0 { 0.0 } else { top },
            width,
            height,
        })
    }

    pub const fn left(self) -> f32 {
        self.left
    }

    pub const fn top(self) -> f32 {
        self.top
    }

    pub const fn width(self) -> f32 {
        self.width
    }

    pub const fn height(self) -> f32 {
        self.height
    }

    pub fn right(self) -> f32 {
        self.left + self.width
    }

    pub fn bottom(self) -> f32 {
        self.top + self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfPageRects {
    page_index: u32,
    rects: Vec<NormalizedRect>,
}

impl PdfPageRects {
    pub fn new(page_index: u32, rects: Vec<NormalizedRect>) -> Result<Self, BookDomainError> {
        if rects.is_empty() {
            return Err(BookDomainError::InvalidPdfSelection);
        }
        Ok(Self { page_index, rects })
    }

    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    pub fn rects(&self) -> &[NormalizedRect] {
        &self.rects
    }

    pub fn into_inner(self) -> (u32, Vec<NormalizedRect>) {
        (self.page_index, self.rects)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfSelectionTarget {
    rects: Vec<NormalizedRect>,
    extra_pages: Vec<PdfPageRects>,
}

impl PdfSelectionTarget {
    pub fn new(
        rects: Vec<NormalizedRect>,
        extra_pages: Vec<PdfPageRects>,
    ) -> Result<Self, BookDomainError> {
        if rects.is_empty() {
            return Err(BookDomainError::InvalidPdfSelection);
        }
        let mut prev_page: Option<u32> = None;
        for page in &extra_pages {
            if page.rects.is_empty() {
                return Err(BookDomainError::InvalidPdfSelection);
            }
            if let Some(prev) = prev_page
                && page.page_index <= prev
            {
                return Err(BookDomainError::InvalidPdfSelection);
            }
            prev_page = Some(page.page_index);
        }
        Ok(Self { rects, extra_pages })
    }

    pub fn rects(&self) -> &[NormalizedRect] {
        &self.rects
    }

    pub fn extra_pages(&self) -> &[PdfPageRects] {
        &self.extra_pages
    }

    pub fn into_inner(self) -> (Vec<NormalizedRect>, Vec<PdfPageRects>) {
        (self.rects, self.extra_pages)
    }

    pub fn start_destination(&self) -> PdfDestination {
        let min_top = self
            .rects
            .iter()
            .map(|rect| rect.top())
            .fold(f32::INFINITY, f32::min);
        PdfDestination::FitH {
            top: None,
            normalized_top: ReadingUnitOffset::clamped(min_top),
        }
    }

    pub fn validate_for_page(&self, page_index: u32) -> Result<(), BookDomainError> {
        if self.rects.is_empty() {
            return Err(BookDomainError::InvalidPdfSelection);
        }
        let mut prev_page = page_index;
        for page in &self.extra_pages {
            if page.rects.is_empty() || page.page_index <= prev_page {
                return Err(BookDomainError::InvalidPdfSelection);
            }
            prev_page = page.page_index;
        }
        Ok(())
    }

    pub fn covered_pages(&self, start_page: u32) -> Vec<u32> {
        let mut pages = Vec::with_capacity(1 + self.extra_pages.len());
        pages.push(start_page);
        pages.extend(self.extra_pages.iter().map(PdfPageRects::page_index));
        pages
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SectionKey {
    Epub(EpubResourcePath),
    Pdf(u32),
}

impl SectionKey {
    pub fn as_key_str(&self) -> Cow<'_, str> {
        match self {
            Self::Epub(path) => Cow::Borrowed(path.as_str()),
            Self::Pdf(page) => Cow::Owned(page.to_string()),
        }
    }

    pub fn epub_href(&self) -> Option<&EpubResourcePath> {
        match self {
            Self::Epub(path) => Some(path),
            Self::Pdf(_) => None,
        }
    }

    pub const fn pdf_page_index(&self) -> Option<u32> {
        match self {
            Self::Epub(_) => None,
            Self::Pdf(page) => Some(*page),
        }
    }
}

pub trait ValidateEpubTarget {
    fn validate(&self) -> Result<(), BookDomainError>;
}

pub trait ValidatePdfTarget {
    fn validate_for_page(&self, page_index: u32) -> Result<(), BookDomainError>;
}

impl ValidateEpubTarget for EpubNavTarget {
    fn validate(&self) -> Result<(), BookDomainError> {
        match self {
            Self::Start => Ok(()),
            Self::Fragment(fragment) => EpubFragment::validate_str(fragment.as_str()),
            Self::Cfi(cfi) => EpubPointCfi::validate_str(cfi.as_str()),
        }
    }
}

impl ValidateEpubTarget for EpubPointTarget {
    fn validate(&self) -> Result<(), BookDomainError> {
        match self {
            Self::Cfi(cfi) => EpubPointCfi::validate_str(cfi.as_str()),
            Self::UnitOffset(offset) => {
                ReadingUnitOffset::new(offset.value())?;
                Ok(())
            }
        }
    }
}

impl ValidateEpubTarget for EpubRangeCfi {
    fn validate(&self) -> Result<(), BookDomainError> {
        EpubRangeCfi::validate_str(self.as_str())
    }
}

impl ValidatePdfTarget for PdfDestination {
    fn validate_for_page(&self, _page_index: u32) -> Result<(), BookDomainError> {
        self.validate()
    }
}

impl ValidatePdfTarget for PdfSelectionTarget {
    fn validate_for_page(&self, page_index: u32) -> Result<(), BookDomainError> {
        PdfSelectionTarget::validate_for_page(self, page_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EpubAnchor<T> {
    pub spine_index: Option<u32>,
    pub href: EpubResourcePath,
    pub target: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PdfAnchor<T> {
    pub page_index: u32,
    pub target: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BookAnchor<EpubT, PdfT> {
    Epub(EpubAnchor<EpubT>),
    Pdf(PdfAnchor<PdfT>),
}

impl<E, P> BookAnchor<E, P> {
    pub const fn file_type(&self) -> BookFileType {
        match self {
            Self::Epub(_) => BookFileType::EPUB,
            Self::Pdf(_) => BookFileType::PDF,
        }
    }

    pub const fn unit_index(&self) -> Option<u32> {
        match self {
            Self::Epub(anchor) => anchor.spine_index,
            Self::Pdf(anchor) => Some(anchor.page_index),
        }
    }

    pub fn epub_href(&self) -> Option<&EpubResourcePath> {
        match self {
            Self::Epub(anchor) => Some(&anchor.href),
            Self::Pdf(_) => None,
        }
    }

    pub fn section_key(&self) -> SectionKey {
        match self {
            Self::Epub(anchor) => SectionKey::Epub(anchor.href.clone()),
            Self::Pdf(anchor) => SectionKey::Pdf(anchor.page_index),
        }
    }

    pub fn map_targets<E2, P2>(
        self,
        map_epub: impl FnOnce(E) -> E2,
        map_pdf: impl FnOnce(P) -> P2,
    ) -> BookAnchor<E2, P2> {
        match self {
            Self::Epub(anchor) => BookAnchor::Epub(EpubAnchor {
                spine_index: anchor.spine_index,
                href: anchor.href,
                target: map_epub(anchor.target),
            }),
            Self::Pdf(anchor) => BookAnchor::Pdf(PdfAnchor {
                page_index: anchor.page_index,
                target: map_pdf(anchor.target),
            }),
        }
    }
}

impl<E: ValidateEpubTarget, P: ValidatePdfTarget> BookAnchor<E, P> {
    pub fn epub(
        spine_index: Option<u32>,
        href: EpubResourcePath,
        target: E,
    ) -> Result<Self, BookDomainError> {
        let anchor = Self::Epub(EpubAnchor {
            spine_index,
            href,
            target,
        });
        anchor.validate()?;
        Ok(anchor)
    }

    pub fn pdf(page_index: u32, target: P) -> Result<Self, BookDomainError> {
        let anchor = Self::Pdf(PdfAnchor { page_index, target });
        anchor.validate()?;
        Ok(anchor)
    }

    pub fn validate(&self) -> Result<(), BookDomainError> {
        match self {
            Self::Epub(anchor) => {
                EpubResourcePath::validate_str(anchor.href.as_str())?;
                anchor.target.validate()
            }
            Self::Pdf(anchor) => anchor.target.validate_for_page(anchor.page_index),
        }
    }
}

pub type NavigationLocator = BookAnchor<EpubNavTarget, PdfDestination>;
pub type BookmarkLocator = BookAnchor<EpubPointTarget, PdfDestination>;
pub type HighlightLocator = BookAnchor<EpubRangeCfi, PdfSelectionTarget>;

impl NavigationLocator {
    pub const fn default_unit_offset(&self) -> ReadingUnitOffset {
        match self {
            Self::Pdf(anchor) => match anchor.target.normalized_top() {
                Some(offset) => offset,
                None => ReadingUnitOffset::ZERO,
            },
            Self::Epub(_) => ReadingUnitOffset::ZERO,
        }
    }
}

impl BookmarkLocator {
    pub const fn default_unit_offset(&self) -> ReadingUnitOffset {
        match self {
            Self::Pdf(anchor) => match anchor.target.normalized_top() {
                Some(offset) => offset,
                None => ReadingUnitOffset::ZERO,
            },
            Self::Epub(anchor) => anchor.target.unit_offset(),
        }
    }

    pub fn to_navigation_locator(&self) -> NavigationLocator {
        self.clone().into_navigation_locator()
    }

    pub fn into_navigation_locator(self) -> NavigationLocator {
        self.map_targets(EpubPointTarget::into_nav_target, |dest| dest)
    }
}

impl HighlightLocator {
    pub fn to_bookmark_locator(&self) -> BookmarkLocator {
        match self {
            Self::Epub(anchor) => BookAnchor::Epub(EpubAnchor {
                spine_index: anchor.spine_index,
                href: anchor.href.clone(),
                target: EpubPointTarget::Cfi(anchor.target.start_point()),
            }),
            Self::Pdf(anchor) => BookAnchor::Pdf(PdfAnchor {
                page_index: anchor.page_index,
                target: anchor.target.start_destination(),
            }),
        }
    }

    pub fn to_navigation_locator(&self) -> NavigationLocator {
        self.to_bookmark_locator().into_navigation_locator()
    }

    pub fn covered_section_keys(&self) -> Vec<SectionKey> {
        match self {
            Self::Epub(anchor) => vec![SectionKey::Epub(anchor.href.clone())],
            Self::Pdf(anchor) => anchor
                .target
                .covered_pages(anchor.page_index)
                .into_iter()
                .map(SectionKey::Pdf)
                .collect(),
        }
    }
}
