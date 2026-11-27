use blake3::Hash;
use serde::{Deserialize, Serialize};
use shiro_domain::library::book::{
    entity::Book,
    errors::BookDomainError,
    file::entity::*,
    locator::{
        BookAnchor, EpubNavTarget, EpubPointCfi, EpubPointTarget, EpubRangeCfi,
        EpubResourcePath, EpubFragment, NormalizedRect, PdfDestination, PdfPageRects,
        PdfSelectionTarget, ReadingUnitOffset, ValidateEpubTarget, ValidatePdfTarget,
    },
    navigation::{BookNavigation, ReadingUnit, TocEntry},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<CreateBookOutput> for GetBookOutput {
    fn from(value: CreateBookOutput) -> Self {
        Self {
            id: value.id,
            title: value.title,
            hash: value.hash,
            file_type: value.file_type,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashedBookOutput {
    #[serde(flatten)]
    pub book: GetBookOutput,
    pub deleted_at: String,
}

impl From<(Book, BookFileType)> for CreateBookOutput {
    fn from((book, file_type): (Book, BookFileType)) -> Self {
        let book = book.into_inner();
        Self {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: file_type.to_string(),
            created_at: book.created_at.to_string(),
            updated_at: book.updated_at.to_string(),
        }
    }
}

impl From<(Book, BookFileType)> for GetBookOutput {
    fn from((book, file_type): (Book, BookFileType)) -> Self {
        let book = book.into_inner();
        Self {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: file_type.to_string(),
            created_at: book.created_at.to_string(),
            updated_at: book.updated_at.to_string(),
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookFileDescriptor {
    pub hash: Hash,
    pub file_type: BookFileType,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookNavigationOutput {
    pub toc: Vec<TocNodeOutput>,
    pub linear_units: Vec<LinearUnitOutput>,
    pub total_weight: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TocNodeOutput {
    pub parent: Option<u32>,
    pub depth: u16,
    pub subtree_end: u32,
    pub title: String,
    pub locator: Option<NavigationLocatorDto>,
    pub unit_index: Option<u32>,
    pub unit_offset: f32,
    pub start_progression: Option<f64>,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearUnitOutput {
    pub index: u32,
    pub is_linear: bool,
    pub weight: u64,
    pub cumulative_weight: u64,
    pub display_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookAnchorDto<E, P> {
    Epub {
        spine_index: Option<u32>,
        href: String,
        target: E,
    },
    Pdf {
        page_index: u32,
        target: P,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EpubNavTargetDto {
    Start,
    Fragment { fragment: String },
    Cfi { cfi: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EpubPointTargetDto {
    Cfi { cfi: String },
    UnitOffset { offset: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubRangeTargetDto {
    pub cfi: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PdfDestDto {
    PageOnly,
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
        normalized_top: Option<f32>,
    },
    FitH {
        top: Option<f32>,
        normalized_top: Option<f32>,
    },
    FitPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedRectDto {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfPageRectsDto {
    pub page_index: u32,
    pub rects: Vec<NormalizedRectDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfSelectionTargetDto {
    pub rects: Vec<NormalizedRectDto>,
    #[serde(default)]
    pub extra_pages: Vec<PdfPageRectsDto>,
}

pub type NavigationLocatorDto = BookAnchorDto<EpubNavTargetDto, PdfDestDto>;
pub type BookmarkLocatorDto = BookAnchorDto<EpubPointTargetDto, PdfDestDto>;
pub type HighlightLocatorDto = BookAnchorDto<EpubRangeTargetDto, PdfSelectionTargetDto>;

pub trait IntoDomainTarget {
    type Domain;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError>;
}

impl IntoDomainTarget for EpubNavTargetDto {
    type Domain = EpubNavTarget;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        match self {
            Self::Start => Ok(EpubNavTarget::Start),
            Self::Fragment { fragment } => Ok(EpubNavTarget::Fragment(EpubFragment::parse(fragment)?)),
            Self::Cfi { cfi } => Ok(EpubNavTarget::Cfi(EpubPointCfi::parse(cfi)?)),
        }
    }
}

impl IntoDomainTarget for EpubPointTargetDto {
    type Domain = EpubPointTarget;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        match self {
            Self::Cfi { cfi } => Ok(EpubPointTarget::Cfi(EpubPointCfi::parse(cfi)?)),
            Self::UnitOffset { offset } => Ok(EpubPointTarget::UnitOffset(ReadingUnitOffset::new(offset)?)),
        }
    }
}

impl IntoDomainTarget for EpubRangeTargetDto {
    type Domain = EpubRangeCfi;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        EpubRangeCfi::parse(self.cfi)
    }
}

impl IntoDomainTarget for PdfDestDto {
    type Domain = PdfDestination;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        match self {
            Self::PageOnly => Ok(PdfDestination::PageOnly),
            Self::Xyz {
                left,
                top,
                zoom,
                normalized_top,
            } => PdfDestination::xyz(
                left,
                top,
                zoom,
                normalized_top.map(ReadingUnitOffset::new).transpose()?,
            ),
            Self::FitH {
                top,
                normalized_top,
            } => PdfDestination::fit_h(
                top,
                normalized_top.map(ReadingUnitOffset::new).transpose()?,
            ),
            Self::FitPage => Ok(PdfDestination::FitPage),
        }
    }
}

impl IntoDomainTarget for NormalizedRectDto {
    type Domain = NormalizedRect;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        NormalizedRect::new(self.left, self.top, self.width, self.height)
    }
}

impl IntoDomainTarget for PdfPageRectsDto {
    type Domain = PdfPageRects;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        let rects = self
            .rects
            .into_iter()
            .map(IntoDomainTarget::try_into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        PdfPageRects::new(self.page_index, rects)
    }
}

impl IntoDomainTarget for PdfSelectionTargetDto {
    type Domain = PdfSelectionTarget;

    fn try_into_domain(self) -> Result<Self::Domain, BookDomainError> {
        let rects = self
            .rects
            .into_iter()
            .map(IntoDomainTarget::try_into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        let extra_pages = self
            .extra_pages
            .into_iter()
            .map(IntoDomainTarget::try_into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        PdfSelectionTarget::new(rects, extra_pages)
    }
}

impl<EDto, PDto> BookAnchorDto<EDto, PDto>
where
    EDto: IntoDomainTarget,
    PDto: IntoDomainTarget,
    EDto::Domain: ValidateEpubTarget,
    PDto::Domain: ValidatePdfTarget,
{
    pub fn try_into_domain(self) -> Result<BookAnchor<EDto::Domain, PDto::Domain>, BookDomainError> {
        match self {
            Self::Epub {
                spine_index,
                href,
                target,
            } => BookAnchor::epub(
                spine_index,
                EpubResourcePath::normalize(href)?,
                target.try_into_domain()?,
            ),
            Self::Pdf { page_index, target } => {
                BookAnchor::pdf(page_index, target.try_into_domain()?)
            }
        }
    }
}

impl From<&EpubNavTarget> for EpubNavTargetDto {
    fn from(target: &EpubNavTarget) -> Self {
        match target {
            EpubNavTarget::Start => Self::Start,
            EpubNavTarget::Fragment(fragment) => Self::Fragment {
                fragment: fragment.as_str().to_owned(),
            },
            EpubNavTarget::Cfi(cfi) => Self::Cfi {
                cfi: cfi.as_str().to_owned(),
            },
        }
    }
}

impl From<&EpubPointTarget> for EpubPointTargetDto {
    fn from(target: &EpubPointTarget) -> Self {
        match target {
            EpubPointTarget::Cfi(cfi) => Self::Cfi {
                cfi: cfi.as_str().to_owned(),
            },
            EpubPointTarget::UnitOffset(offset) => Self::UnitOffset {
                offset: offset.value(),
            },
        }
    }
}

impl From<&EpubRangeCfi> for EpubRangeTargetDto {
    fn from(cfi: &EpubRangeCfi) -> Self {
        Self {
            cfi: cfi.as_str().to_owned(),
        }
    }
}

impl From<&PdfDestination> for PdfDestDto {
    fn from(dest: &PdfDestination) -> Self {
        match *dest {
            PdfDestination::PageOnly => Self::PageOnly,
            PdfDestination::Xyz {
                left,
                top,
                zoom,
                normalized_top,
            } => Self::Xyz {
                left,
                top,
                zoom,
                normalized_top: normalized_top.map(ReadingUnitOffset::value),
            },
            PdfDestination::FitH {
                top,
                normalized_top,
            } => Self::FitH {
                top,
                normalized_top: normalized_top.map(ReadingUnitOffset::value),
            },
            PdfDestination::FitPage => Self::FitPage,
        }
    }
}

impl From<PdfDestination> for PdfDestDto {
    fn from(dest: PdfDestination) -> Self {
        Self::from(&dest)
    }
}

impl From<&NormalizedRect> for NormalizedRectDto {
    fn from(rect: &NormalizedRect) -> Self {
        Self {
            left: rect.left(),
            top: rect.top(),
            width: rect.width(),
            height: rect.height(),
        }
    }
}

impl From<&PdfPageRects> for PdfPageRectsDto {
    fn from(page: &PdfPageRects) -> Self {
        Self {
            page_index: page.page_index(),
            rects: page.rects().iter().map(Into::into).collect(),
        }
    }
}

impl From<&PdfSelectionTarget> for PdfSelectionTargetDto {
    fn from(selection: &PdfSelectionTarget) -> Self {
        Self {
            rects: selection.rects().iter().map(Into::into).collect(),
            extra_pages: selection.extra_pages().iter().map(Into::into).collect(),
        }
    }
}

impl<'a, E, P, EDto, PDto> From<&'a BookAnchor<E, P>> for BookAnchorDto<EDto, PDto>
where
    EDto: From<&'a E>,
    PDto: From<&'a P>,
{
    fn from(anchor: &'a BookAnchor<E, P>) -> Self {
        match anchor {
            BookAnchor::Epub(epub) => Self::Epub {
                spine_index: epub.spine_index,
                href: epub.href.as_str().to_owned(),
                target: (&epub.target).into(),
            },
            BookAnchor::Pdf(pdf) => Self::Pdf {
                page_index: pdf.page_index,
                target: (&pdf.target).into(),
            },
        }
    }
}

impl From<&ReadingUnit> for LinearUnitOutput {
    fn from(unit: &ReadingUnit) -> Self {
        Self {
            index: unit.index(),
            is_linear: unit.is_linear(),
            weight: unit.weight(),
            cumulative_weight: unit.cumulative_weight(),
            display_label: unit.display_label().map(str::to_owned),
        }
    }
}

impl From<&TocEntry> for TocNodeOutput {
    fn from(node: &TocEntry) -> Self {
        Self {
            parent: node.parent(),
            depth: node.depth(),
            subtree_end: node.subtree_end(),
            title: node.title().to_owned(),
            locator: node.locator().map(Into::into),
            unit_index: node.unit_index(),
            unit_offset: node.unit_offset().value(),
            start_progression: node.start_progression().map(|p| p.value()),
            flags: node.flags().bits(),
        }
    }
}

impl From<BookNavigation> for BookNavigationOutput {
    fn from(nav: BookNavigation) -> Self {
        Self {
            toc: nav.toc().iter().map(Into::into).collect(),
            linear_units: nav.reading_units().iter().map(Into::into).collect(),
            total_weight: nav.total_weight(),
        }
    }
}
