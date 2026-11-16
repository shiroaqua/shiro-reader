use std::sync::Arc;

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;
use toasty::Db;

use crate::{
    application::{
        MapDbResult,
        errors::MapToastyError,
        library::{
            book::{
                commands::{
                    CreateBookCommand, DeleteBookCommand, GetBookCoverCommand, RenameBookCommand,
                },
                dto::{CreateBookOutput, GetBookOutput},
                file::{errors::BookFileApplicationError, service::BookFileService},
                queries::GetBooksQuery,
            },
            errors::LibraryApplicationError,
        },
    },
    domain::{
        EntityCreate, EntityId, RequiresVerificationEntity,
        library::book::{
            entity::Book,
            value_objects::{BookId, BookTitle},
        },
    },
};

#[derive(new)]
pub struct BookService {
    db: Db,
    bookfile: Arc<BookFileService>,
}

impl BookService {
    pub async fn create_book(
        &mut self,
        command: CreateBookCommand,
    ) -> Result<CreateBookOutput, LibraryApplicationError> {
        let book = Book::new(
            BookId::new(),
            BookTitle::parse(command.title)?,
            self.bookfile.parse_existing_hash(&command.hash)?,
        )?;
        let book = book.create().exec(&mut self.db).await.map_result()?;

        Ok(CreateBookOutput {
            id: book.id().to_string(),
            title: book.title().to_string(),
            hash: book.hash().to_hex().to_string(),
            file_type: self
                .bookfile
                .get_book_type(&book.hash())
                .ok_or(BookFileApplicationError::Unsupported)?
                .to_string(),
            created_at: book.created_at(),
            updated_at: book.updated_at(),
        })
    }

    pub async fn delete_book(
        &mut self,
        command: DeleteBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = Book::get_by_id(&mut self.db, &id).await.map_result()?;
        book.delete()
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)
    }

    pub async fn rename_book(
        &mut self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;

        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let mut book = Book::get_by_id(&mut tx, &id).await.map_result()?;
        book.rename(new_title)?
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?;
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn get_books(
        &mut self,
        query: GetBooksQuery,
    ) -> Result<Vec<GetBookOutput>, LibraryApplicationError> {
        let book_ids = query
            .book_ids
            .iter()
            .map(BookId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let mut books_query = Book::all();
        if !book_ids.is_empty() {
            books_query = books_query.filter(
                Book::fields().id().inner().in_list(
                    book_ids
                        .iter()
                        .map(|id| *id.as_ref())
                        .collect::<Vec<uuid::Uuid>>(),
                ),
            );
        }
        let mut books = books_query
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)?
            .into_iter()
            .map(|book| {
                let book = book.verify()?;
                Ok(GetBookOutput {
                    id: book.id().to_string(),
                    title: book.title().to_string(),
                    hash: book.hash().to_hex().to_string(),
                    file_type: String::new(),
                    created_at: book.created_at(),
                    updated_at: book.updated_at(),
                })
            })
            .collect::<Result<Vec<_>, LibraryApplicationError>>()?;
        for book in &mut books {
            let hash = self.bookfile.parse_existing_hash(&book.hash)?;
            book.file_type = self
                .bookfile
                .get_book_type(&hash)
                .ok_or(BookFileApplicationError::Unsupported)?
                .to_string();
        }
        Ok(books)
    }

    pub async fn get_cover(
        &mut self,
        command: GetBookCoverCommand,
    ) -> Result<
        (impl Stream<Item = std::io::Result<Bytes>> + 'static, String),
        LibraryApplicationError,
    > {
        let id = BookId::parse(command.id)?;
        let book = Book::get_by_id(&mut self.db, &id).await.map_result()?;
        self.bookfile.download_book_cover_file(*book.hash()).await
    }
}
