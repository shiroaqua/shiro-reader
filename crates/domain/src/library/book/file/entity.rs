use derive_more::Display;

use crate::library::book::chapter::ChapterLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum BookFileType {
    PDF,
    EPUB,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookFileChapter {
    pub position: u32,
    pub title: String,
    pub location: ChapterLocation,
}