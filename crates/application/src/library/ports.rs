use std::{collections::HashSet, future::Future, io, pin::Pin};

use blake3::Hash;
use bytes::Bytes;
use futures_util::Stream;
use shiro_domain::library::{
    book::{
        entity::{Book, BookChange, BookMetadata, BookMetadataChange},
        navigation::BookNavigation,
        value_objects::{
            BookAuthor, BookDescription, BookFileType, BookId, BookIdentifier, BookLanguage,
            BookPublishedDate, BookPublisher, BookSubtitle,
        },
    },
    tag::{
        entity::{BookTag, BookTagChange, TagDefinition, TagDefinitionChange},
        value_objects::TagId,
    },
};
use tokio::io::AsyncRead;

pub type CoverByteStream = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send + 'static>>;

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

    fn find_by_id(
        &mut self,
        id: &BookId,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn find_metadata_by_book_id(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<BookMetadata, BookRepositoryError>> + Send;

    fn list_active(
        &mut self,
        ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn list_trashed(
        &mut self,
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn begin_transaction(
        &mut self,
    ) -> impl Future<Output = Result<Self::Tx<'_>, BookRepositoryError>> + Send;
}

pub trait BookTransaction: Send {
    fn create_book(
        &mut self,
        book: Book,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn create_metadata(
        &mut self,
        metadata: BookMetadata,
    ) -> impl Future<Output = Result<BookMetadata, BookRepositoryError>> + Send;

    fn find_by_id(
        &mut self,
        id: &BookId,
    ) -> impl Future<Output = Result<Book, BookRepositoryError>> + Send;

    fn find_by_ids(
        &mut self,
        ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<Book>, BookRepositoryError>> + Send;

    fn find_metadata_by_book_id(
        &mut self,
        book_id: &BookId,
    ) -> impl Future<Output = Result<BookMetadata, BookRepositoryError>> + Send;

    fn find_metadata_by_book_ids(
        &mut self,
        book_ids: &[BookId],
    ) -> impl Future<Output = Result<Vec<BookMetadata>, BookRepositoryError>> + Send;

    fn list_active_titles(
        &mut self,
    ) -> impl Future<Output = Result<HashSet<String>, BookRepositoryError>> + Send;

    fn apply_book_change(
        &mut self,
        change: BookChange<'_>,
    ) -> impl Future<Output = Result<(), BookRepositoryError>> + Send;

    fn apply_metadata_change(
        &mut self,
        change: BookMetadataChange<'_>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookFileMetadata {
    pub file_type: BookFileType,
    pub size: u64,
    pub page_count: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedBookMetadata {
    pub subtitle: Option<BookSubtitle>,
    pub authors: Vec<BookAuthor>,
    pub publisher: Option<BookPublisher>,
    pub published_date: Option<BookPublishedDate>,
    pub language: Option<BookLanguage>,
    pub description: Option<BookDescription>,
    pub identifier: Option<BookIdentifier>,
}

#[derive(Debug)]
pub struct OpenedBookFile<R> {
    pub reader: R,
    pub total_size: u64,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum BookFileStorageError {
    #[error("book file not found")]
    NotFound,

    #[error("book file already exists")]
    AlreadyExists,

    #[error("byte range (offset={offset}, len={len:?}) out of bounds for file size {size}")]
    RangeOutOfBounds {
        offset: u64,
        len: Option<u64>,
        size: u64,
    },

    #[error("book file hash mismatch")]
    HashMismatch,

    #[error("unsupported book file format or container version")]
    Unsupported,

    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Append-only content-addressed book file storage.
///
/// Validated and persisted book files are never deleted.
pub trait BookFileStore: Send + Sync + 'static {
    type Reader: AsyncRead + Unpin + Send + 'static;

    fn contains(&self, hash: &Hash) -> bool {
        self.get_meta(hash).is_some()
    }

    fn get_type(&self, hash: &Hash) -> Option<BookFileType> {
        self.get_meta(hash).map(|meta| meta.file_type)
    }

    fn get_meta(&self, hash: &Hash) -> Option<BookFileMetadata>;

    fn import<R>(
        &self,
        hash: &Hash,
        reader: R,
    ) -> impl Future<Output = Result<(), BookFileStorageError>> + Send
    where
        R: AsyncRead + Unpin + Send;

    /// Opens a sequential bounded stream over `[offset .. offset + length)` for streaming transfer.
    fn open_range(
        &self,
        hash: &Hash,
        offset: u64,
        length: Option<u64>,
    ) -> impl Future<Output = Result<OpenedBookFile<Self::Reader>, BookFileStorageError>> + Send;

    /// Reads an exact byte slice `[offset .. offset + len)` for random-access archive parsing.
    fn read_exact_at(
        &self,
        hash: &Hash,
        offset: u64,
        len: usize,
    ) -> impl Future<Output = Result<Bytes, BookFileStorageError>> + Send;

    fn open(
        &self,
        hash: &Hash,
    ) -> impl Future<Output = Result<Self::Reader, BookFileStorageError>> + Send {
        async move { self.open_range(hash, 0, None).await.map(|opened| opened.reader) }
    }

    fn open_cover(
        &self,
        hash: Hash,
    ) -> impl Future<Output = Result<(CoverByteStream, String), BookFileStorageError>> + Send;

    /// Extracts the unified navigation index (TOC and linear units) from a stored book file.
    ///
    /// Returns an empty [`BookNavigation`] if extraction fails for any reason.
    fn extract_navigation(
        &self,
        hash: &Hash,
    ) -> impl Future<Output = BookNavigation> + Send;

    /// Best-effort extracts embedded descriptive metadata from a stored book file.
    ///
    /// Returns an empty [`ExtractedBookMetadata`] if extraction fails or fields are invalid.
    fn extract_metadata(
        &self,
        hash: &Hash,
    ) -> impl Future<Output = ExtractedBookMetadata> + Send;
}
