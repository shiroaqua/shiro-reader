use std::sync::Arc;

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;

use crate::{
    application::library::{
        book::{
            commands::{
                CreateBookCommand, DeleteBookCommand, GetBookCoverCommand, RenameBookCommand,
            },
            dto::{CreateBookOutput, GetBookOutput},
            file::service::BookFileService,
            ports::BookRepository,
            queries::GetBooksQuery,
        },
        errors::LibraryApplicationError,
    }, domain::library::{
        book::{
            entity::Book,
            value_objects::{BookId, BookTitle},
        },
    }, shared::time::now_ns,
};

#[derive(new)]
pub struct BookService<T: BookRepository> {
    repository: T,
    bookfile: Arc<BookFileService>,
}

impl<T: BookRepository> BookService<T> {
    pub async fn create_book(
        &self,
        command: CreateBookCommand,
    ) -> Result<CreateBookOutput, LibraryApplicationError> {
        let now = now_ns();
   
        let id = BookId::new();
        let title = BookTitle::parse(command.title)?;
        let hash = self.bookfile.parse_existing_hash(&command.hash)?;
        let book = Book::new(id, title, hash,now);
        self.repository.create(&book).await?;

        Ok(CreateBookOutput {
            id: book.id().to_string(),
            file_type: self.bookfile.get_book_type(&hash).ok_or(crate::application::library::book::file::errors::BookFileApplicationError::Unsupported)?.to_string(),
            created_at: now,
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
        let mut book = self.repository.find_by_id(&id).await?;
        book.rename(new_title)?;
        self.repository.commit(book).await?;
        Ok(())
    }

    pub async fn get_books(
        &self,
        query: GetBooksQuery,
    ) -> Result<Vec<GetBookOutput>, LibraryApplicationError> {
        let mut books = self
            .repository
            .list(
                query
                    .book_ids
                    .iter()
                    .map(|b| BookId::parse(b))
                    .collect::<Result<_, _>>()?,
            )
            .await?;

        for b in &mut books {
            let hash = self.bookfile.parse_existing_hash(&b.hash)?;
            b.file_type = self.bookfile.get_book_type(&hash).ok_or(crate::application::library::book::file::errors::BookFileApplicationError::Unsupported)?.to_string();
        }
        Ok(books)
    }

    pub async fn get_cover(
        &self,
        command: GetBookCoverCommand,
    ) -> Result<impl Stream<Item = std::io::Result<Bytes>> + 'static, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        // 此处获得的是共享书库中的默认封面，此后添加用户系统后需改为获取对应书籍的封面（换句话说就是每个book_id对应的封面）
        self.bookfile.download_book_cover_file(*book.hash()).await
    }
}
