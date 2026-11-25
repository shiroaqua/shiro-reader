use blake3::Hash;
use serde::{Deserialize, Serialize};
use shiro_domain::library::book::{
    entity::Book,
    file::entity::*,
    navigation::{BookLocator, BookNavigation, LinearUnit, PdfDest, TocNode},
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
    pub locator: Option<BookLocatorOutput>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookLocatorOutput {
    Pdf {
        page_index: u32,
        dest: PdfDestOutput,
    },
    Epub {
        spine_index: Option<u32>,
        href: String,
        fragment: Option<String>,
        cfi: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PdfDestOutput {
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

impl From<PdfDest> for PdfDestOutput {
    fn from(dest: PdfDest) -> Self {
        match dest {
            PdfDest::PageOnly => Self::PageOnly,
            PdfDest::Xyz {
                left,
                top,
                zoom,
                normalized_top,
            } => Self::Xyz {
                left,
                top,
                zoom,
                normalized_top: normalized_top.map(|offset| offset.value()),
            },
            PdfDest::FitH {
                top,
                normalized_top,
            } => Self::FitH {
                top,
                normalized_top: normalized_top.map(|offset| offset.value()),
            },
            PdfDest::FitPage => Self::FitPage,
        }
    }
}

impl From<&BookLocator> for BookLocatorOutput {
    fn from(locator: &BookLocator) -> Self {
        match locator {
            BookLocator::Pdf { page_index, dest } => Self::Pdf {
                page_index: *page_index,
                dest: (*dest).into(),
            },
            BookLocator::Epub {
                spine_index,
                href,
                fragment,
                cfi,
            } => Self::Epub {
                spine_index: *spine_index,
                href: href.as_str().to_owned(),
                fragment: fragment.clone(),
                cfi: cfi.clone(),
            },
        }
    }
}

impl From<&LinearUnit> for LinearUnitOutput {
    fn from(unit: &LinearUnit) -> Self {
        Self {
            index: unit.index(),
            is_linear: unit.is_linear(),
            weight: unit.weight(),
            cumulative_weight: unit.cumulative_weight(),
            display_label: unit.display_label().map(str::to_owned),
        }
    }
}

impl From<&TocNode> for TocNodeOutput {
    fn from(node: &TocNode) -> Self {
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
            linear_units: nav.linear_units().iter().map(Into::into).collect(),
            total_weight: nav.total_weight(),
        }
    }
}
