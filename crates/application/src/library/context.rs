use std::sync::Arc;

use crate::library::{
    book::{file::service::BookFileService, service::BookService},
    ports::{BookFileStore, BookRepository, TagRepository},
    tag::service::TagService,
};

/// Transport-neutral library facade combining book, tag, and book-file services.
///
/// Both the standalone local client runtime and the multi-user server runtime
/// assemble a `LibraryContext` without depending on HTTP extractors or IPC details.
pub struct LibraryContext<B, T, F> {
    books: B,
    tags: T,
    book_files: Arc<BookFileService<F>>,
}

impl<B: Clone, T: Clone, F> Clone for LibraryContext<B, T, F> {
    fn clone(&self) -> Self {
        Self {
            books: self.books.clone(),
            tags: self.tags.clone(),
            book_files: Arc::clone(&self.book_files),
        }
    }
}

impl<B, T, F> LibraryContext<B, T, F>
where
    B: BookRepository,
    T: TagRepository,
    F: BookFileStore,
{
    pub fn new(
        books: B,
        tags: T,
        book_files: Arc<BookFileService<F>>,
    ) -> Self {
        Self {
            books,
            tags,
            book_files,
        }
    }

    pub fn book_service(&self) -> BookService<B, F> {
        BookService::new(self.books.clone(), Arc::clone(&self.book_files))
    }

    pub fn tag_service(&self) -> TagService<T> {
        TagService::new(self.tags.clone())
    }

    pub fn book_file_service(&self) -> &Arc<BookFileService<F>> {
        &self.book_files
    }
}
