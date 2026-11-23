use std::{collections::HashSet, future::Future, io, pin::Pin};

use blake3::Hash;
use bytes::Bytes;
use futures_util::Stream;
use shiro_domain::library::{
    book::{
        chapter::{Chapter, ChapterId},
        entity::{Book, BookChange},
        file::entity::{BookFileChapter, BookFileType},
        value_objects::BookId,
    },
    tag::{
        entity::{BookTag, BookTagChange, TagDefinition, TagDefinitionChange},
        value_objects::TagId,
    },
};
use tokio::{
    fs::File,
    io::{AsyncRead, Take},
};

pub type CoverByteStream = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send + 'static>>;

#[derive(Debug, thiserror::Error)]
pub enum BookChapterError {
    #[error("book not found")]
    BookNotFound,

    #[error("chapter not found")]
    ChapterNotFound,

    #[error("a chapter already exists at this position")]
    PositionConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum BookRepositoryError {
    #[error("book not found")]
    NotFound,

    #[error("book title conflict")]
    TitleConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

pub trait BookRepository: Clone + Send + Sync + 'static {
    type Tx<'a>: BookTransaction + Send + 'a
    where
        Self: 'a;

    fn create(
        &mut self,
        book: Book,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn find_by_id(
        &mut self,
        id: &BookId,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn list_active(
        &mut self,
        ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn list_trashed(
        &mut self,
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn create_chapter(
        &mut self,
        chapter: Chapter,
    ) -> impl Future<Output = Result<Chapter, BookChapterError>> + Send;

    fn delete_chapter(
        &mut self,
        book_id: &BookId,
        chapter_id: &ChapterId,
    ) -> impl Future<Output = Result<(), BookChapterError>> + Send;

    fn list_chapters(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<Vec<Chapter>, BookChapterError>> + Send;

    fn begin_transaction(
        &mut self,
    ) -> impl Future<Output = Result<Self::Tx<'_>, BookRepositoryError>> + Send;
}

pub trait BookTransaction: Send {
    fn create_book(
        &mut self,
        book: Book,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn create_chapter(
        &mut self,
        chapter: Chapter,
    ) -> impl Future<Output = Result<Chapter, BookChapterError>> + Send;

    fn find_by_id(
        &mut self,
        id: &BookId,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn find_by_ids(
        &mut self,
        ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn list_active_titles(
        &mut self,
    ) -> impl Future<Output = Result<HashSet<String>, BookRepositoryError>> + Send;

    fn apply_book_change(
        &mut self,
        change: BookChange<'_>,
    ) -> impl Future<Output = Result<(), BookRepositoryError>> + Send;

    fn delete_books(
        &mut self,
        books: Vec<Book>,
    ) -> impl Future<Output = Result<(), BookRepositoryError>> + Send;

    fn list_tag_bindings(
        &mut self,
        book_ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<BookTag>, BookRepositoryError>> + Send;

    fn apply_tag_binding_change(
        &mut self,
        change: BookTagChange<'_>,
    ) -> impl Future<Output = Result<(), BookRepositoryError>> + Send;

    fn commit(self) -> impl Future<Output = Result<(), BookRepositoryError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum TagRepositoryError {
    #[error("tag not found")]
    NotFound,

    #[error("parent tag not found")]
    ParentNotFound,

    #[error("a sibling tag with this name already exists")]
    SiblingNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

pub trait TagRepository: Clone + Send + Sync + 'static {
    type Tx<'a>: TagTransaction + Send + 'a
    where
        Self: 'a;

    fn create(
        &mut self,
        definition: TagDefinition,
    ) -> impl Future<Output = Result<TagDefinition, TagRepositoryError>> + Send;

    fn list_definitions(
        &mut self,
    ) -> impl Future<Output = Result<Vec<TagDefinition>, TagRepositoryError>> + Send;

    fn active_book_exists(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<bool, TagRepositoryError>> + Send;

    fn list_active_book_tags(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<Vec<BookTag>, TagRepositoryError>> + Send;

    fn begin_transaction(
        &mut self,
    ) -> impl Future<Output = Result<Self::Tx<'_>, TagRepositoryError>> + Send;
}

pub trait TagTransaction: Send {
    fn find_definition_by_id(
        &mut self,
        id: &TagId,
    ) -> impl Future<Output = Result<TagDefinition, TagRepositoryError>> + Send;

    fn definition_exists(
        &mut self,
        id: &TagId,
    ) -> impl Future<Output = Result<bool, TagRepositoryError>> + Send;

    fn list_definitions(
        &mut self,
    ) -> impl Future<Output = Result<Vec<TagDefinition>, TagRepositoryError>> + Send;

    fn apply_definition_change(
        &mut self,
        change: TagDefinitionChange<'_>,
    ) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;

    fn delete_definitions(
        &mut self,
        definitions: Vec<TagDefinition>,
    ) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;

    fn active_book_exists(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<bool, TagRepositoryError>> + Send;

    fn find_binding(
        &mut self,
        book_id: &BookId,
        tag_id: &TagId,
    ) -> impl Future<Output = Result<Option<BookTag>, TagRepositoryError>> + Send;

    fn create_binding(
        &mut self,
        binding: BookTag,
    ) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;

    fn apply_binding_change(
        &mut self,
        change: BookTagChange<'_>,
    ) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;

    fn delete_binding(
        &mut self,
        binding: BookTag,
    ) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;

    fn commit(self) -> impl Future<Output = Result<(), TagRepositoryError>> + Send;
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct BookFileStorageError(#[from] pub io::Error);

impl BookFileStorageError {
    pub fn kind(&self) -> io::ErrorKind {
        self.0.kind()
    }
}

impl From<io::ErrorKind> for BookFileStorageError {
    fn from(kind: io::ErrorKind) -> Self {
        Self(io::Error::from(kind))
    }
}

pub trait BookFileStore: Send + Sync + 'static {
    fn contains(&self, hash: &Hash) -> bool;

    fn get_type(&self, hash: &Hash) -> Option<BookFileType>;

    fn get_chapters(&self, hash: &Hash) -> Vec<BookFileChapter>;

    fn save<R>(
        &self,
        hash: &Hash,
        reader: R,
    ) -> impl Future<Output = Result<(), BookFileStorageError>> + Send
    where
        R: AsyncRead + Unpin + Send;

    fn open(
        &self,
        hash: &Hash,
    ) -> impl Future<Output = Result<Take<File>, BookFileStorageError>> + Send;

    fn open_cover(
        &self,
        hash: Hash,
    ) -> impl Future<Output = Result<(CoverByteStream, String), BookFileStorageError>> + Send;

    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<(), BookFileStorageError>> + Send;
}
