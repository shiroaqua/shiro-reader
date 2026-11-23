use entity_derive::Entity;
use serde::{Deserialize, Serialize};

use crate::library::book::value_objects::BookId;

use super::value_objects::{ChapterId, ChapterLocation, ChapterTitle};

/// A format-independent chapter outline entry.
///
/// `position` provides a common reading order, while `location` keeps the
/// format-specific address needed to navigate the source document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
#[serde(deny_unknown_fields)]
pub struct Chapter {
    #[key]
    id: ChapterId,
    book_id: BookId,
    /// Zero-based position in the book's chapter outline.
    position: u32,
    title: ChapterTitle,
    location: ChapterLocation,
}

impl Chapter {
    pub fn new(
        id: ChapterId,
        book_id: BookId,
        position: u32,
        title: ChapterTitle,
        location: ChapterLocation,
    ) -> Self {
        Self {
            id,
            book_id,
            position,
            title,
            location,
        }
    }

    pub fn id(&self) -> &ChapterId {
        &self.id
    }

    pub fn book_id(&self) -> &BookId {
        &self.book_id
    }

    pub fn position(&self) -> u32 {
        self.position
    }

    pub fn title(&self) -> &ChapterTitle {
        &self.title
    }

    pub fn location(&self) -> &ChapterLocation {
        &self.location
    }
}
