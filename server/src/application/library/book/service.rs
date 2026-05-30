use std::sync::Arc;

use derive_new::new;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

use crate::{
    application::library::{
        book::{
            commands::{
                CreateBookCommand, CreateBookOutput, DeleteBookCommand, GetBookCommand,
                GetBookOutput, GetBooksCommand, GetBooksOutput, RenameBookCommand,
            },
            errors::BookApplicationError,
            file::service::BookFileService,
            ports::BookRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::{
        book::{
            entity::{Book, BookFileType},
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


        let mut file = self.bookfile.download_book_file(&command.hash).await?;
        let file_type = if compare_head(&mut file, b"%PDF-").await? {
            BookFileType::PDF
        } else if compare_head(&mut file, b"PK\x03\x04\x14\x00\x00\x00\x00\x00\xF0\x92\xFEBoa\xAB,\x14\x00\x00\x00\x14\x00\x00\x00\x08\x00\x00\x00mimetypeapplication/epub").await?{
            BookFileType::EPUB
        } else { 
            BookFileType::TXT // 设计上，前端就不该上传未支持格式的文件过来，因此遇到一律当TXT处理（我不能抛错误，因为我最终肯定要支持TXT文件，但TXT没有固定头部，我不可能在服务端区分文件是未支持格式还是奇怪的TXT）
        };


        let book = Book::new(
            id,
            title,
            hash,
            file_type,
            bookshelf_id,
            folder_id,
            now,
            now,
        );
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
                .map(Into::into)
                .collect(),
        ))
    }

    pub async fn get_book(
        &self,
        command: GetBookCommand,
    ) -> Result<GetBookOutput, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        Ok(self.repository.find_by_id(&id).await?.into())
    }
}

async fn compare_head(file: &mut File, expected_head: &[u8]) -> Result<bool, LibraryApplicationError> {
    let mut buf = vec![0u8; expected_head.len()];
    file.seek(std::io::SeekFrom::Start(0)).await.map_err(|_| LibraryApplicationError::Book(BookApplicationError::InvailFile))?;
    file.read_exact(&mut buf).await.map_err(|_| LibraryApplicationError::Book(BookApplicationError::InvailFile))?;
    Ok(buf == expected_head)
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
