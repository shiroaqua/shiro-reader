use std::sync::Arc;

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;

use crate::{
    application::library::{
        book::{
            commands::{
                CreateBookCommand, CreateBookOutput, DeleteBookCommand, GetBookCommand,
                GetBookCoverCommand, GetBookOutput, GetBooksCommand, GetBooksOutput,
                RenameBookCommand,
            },
            errors::BookApplicationError,
            file::service::BookFileService,
            ports::BookRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::{
        book::{
            entity::Book,
            errors::BookDomainError,
            value_objects::{BookId, BookTitle},
        },
        bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId},
    },
    shared::time::now_ms,
};

#[derive(new)]
pub struct BookService {
    repository: Arc<dyn BookRepository>,
    bookfile: Arc<BookFileService>,
}

impl BookService {
    pub async fn create_book(
        &self,
        command: CreateBookCommand,
    ) -> Result<CreateBookOutput, LibraryApplicationError> {
        let id = BookId::new();
        let title = BookTitle::parse(command.title)?;
        let hash = self.bookfile.parse_existing_hash(&command.hash)?;
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = command
            .folder_id
            .map(FolderId::parse_folder_id)
            .transpose()?;
        let now = now_ms();

        let book = Book::new(id, title, hash, bookshelf_id, folder_id, now, now);
        let created = self.repository.create(book).await?;

        Ok(CreateBookOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }

    pub async fn delete_book(
        &self,
        command: DeleteBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.repository.delete(&id).await?;
        Ok(())
    }

    pub async fn rename_book(
        &self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;
        self.repository.rename(&id, &new_title).await?;

        Ok(())
    }

    pub async fn get_books(
        &self,
        command: GetBooksCommand,
    ) -> Result<GetBooksOutput, LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = command
            .folder_id
            .and_then(|f| Some(FolderId::parse_folder_id(f)))
            .transpose()?;

        Ok(GetBooksOutput(
            self.repository
                .list(&bookshelf_id, folder_id.as_ref())
                .await?
                .into_iter()
                .map(|b| self.into(b))
                .collect(),
        ))
    }

    pub async fn get_cover(
        &self,
        command: GetBookCoverCommand,
    ) -> Result<impl Stream<Item = std::io::Result<Bytes>> + 'static, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        // 此处获得的是共享书库中的默认封面，此后添加用户系统后需改为获取对应书籍的封面（换句话说就是每个book_id对应的封面）
        self.bookfile.download_book_cover_file(book.hash).await
    }

    pub async fn get_book(
        &self,
        command: GetBookCommand,
    ) -> Result<GetBookOutput, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        Ok(self.into(self.repository.find_by_id(&id).await?))
    }

    fn into(&self, book: Book) -> GetBookOutput {
        let file_type = self
            .bookfile
            .get_book_type(&book.hash)
            .expect("Unable to get book file type.");
        GetBookOutput {
            id: book.id,
            title: book.title,
            hash: book.hash,
            file_type: file_type,
            bookshelf_id: book.bookshelf_id,
            folder_id: book.folder_id,
            created_at: book.created_at,
            updated_at: book.updated_at,
        }
    }
}

impl From<BookDomainError> for BookApplicationError {
    fn from(value: BookDomainError) -> Self {
        match value {
            BookDomainError::InvalidId => Self::InvalidId,
            BookDomainError::MissingTitle => Self::MissingTitle,
            BookDomainError::InvalidTitleFormat => Self::InvalidTitleFormat,
            BookDomainError::InvalidHash => Self::InvalidHashFormat,
        }
    }
}
