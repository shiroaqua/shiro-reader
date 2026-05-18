use std::str::FromStr;
use std::path::PathBuf;
use std::sync::Arc;


use crate::{
    application::library::{bookfile::service::BookFileService, bookshelf::{
        folder::{ports::FolderRepository, service::FolderService},
        ports::BookshelfRepository,
        service::BookshelfService,
    }},
    config::Config,
    infrastructure::{
        db,
        repositories::{
            sqlite_bookshelf_repository::SqliteBookshelfRepository,
            sqlite_folder_repository::SqliteFolderRepository,
        }, storage::hash_file_storage::HashFileStorage,
    },
};


#[derive(Clone)]
pub struct AppState {
    pub bookfile_service: Arc<BookFileService>,
    pub bookshelf_service: Arc<BookshelfService>,
    pub folder_service: Arc<FolderService>,
    
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(PathBuf::from("users").join("default"))?;
        std::fs::create_dir_all(PathBuf::from("books").join(".temp"))?;

        let mut book_file_storage = HashFileStorage::new(PathBuf::from("books"), String::from_str("book")?);
        book_file_storage.scan()?;

        let bookfile_service = Arc::new(BookFileService::new(book_file_storage));
        
        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;
        
        let bookshelf_repository: Arc<dyn BookshelfRepository> =
            Arc::new(SqliteBookshelfRepository::new(pool.clone()));
        let bookshelf_service = Arc::new(BookshelfService::new(bookshelf_repository));

        let folder_repository: Arc<dyn FolderRepository> =
            Arc::new(SqliteFolderRepository::new(pool.clone()));
        let folder_service = Arc::new(FolderService::new(folder_repository));

      
        Ok(Self {
            bookfile_service,
            bookshelf_service,
            folder_service,
        })
    }
}
