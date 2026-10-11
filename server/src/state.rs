use std::{str::FromStr, sync::Mutex};
use std::sync::Arc;


use pdfium_render::prelude::Pdfium;

use crate::{
    application::library::{
        book::{file::service::BookFileService, ports::BookRepository, service::BookService},
    },
    config::Config,
    infrastructure::{
        db,
        repositories::{
            sqlite_book_repository::SqliteBookRepository,
        }, storage::book_file_storage::BookFileStorage,
    },
};


#[derive(Clone)]
pub struct AppState {
    pub book_service: Arc<BookService>,
    pub bookfile_service: Arc<BookFileService>,
    
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(config.root_directory.join("users").join("default"))?;
       
        let pdfium = Arc::new(Mutex::new(Pdfium::new(Pdfium::bind_to_system_library()?)));

        let mut book_file_storage = BookFileStorage::new(config.root_directory.join("books"), String::from_str("book")?, pdfium);
        book_file_storage.scan()?;

        let bookfile_service = Arc::new(BookFileService::new(book_file_storage));
        
        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;

        let book_repository: Arc<dyn BookRepository> =
            Arc::new(SqliteBookRepository::new(pool.clone()));
        let book_service = Arc::new(BookService::new(book_repository, bookfile_service.clone()));

      
        Ok(Self {
            bookfile_service,
            book_service,
        })
    }
}
