use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    application::library::bookshelf::{
        folder::{ports::FolderRepository, service::FolderService},
        ports::BookshelfRepository,
        servise::BookshelfService,
    },
    config::Config,
    infrastructure::{
        db,
        repositories::{
            sqlite_bookshelf_repository::SqliteBookshelfRepository,
            sqlite_folder_repository::SqliteFolderRepository,
        },
    },
};

#[derive(Clone)]
pub struct AppState {
    pub bookshelf_service: Arc<BookshelfService>,
    pub folder_service: Arc<FolderService>,
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(PathBuf::from("users").join("default"))?;

        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;

        let bookshelf_repository: Arc<dyn BookshelfRepository> =
            Arc::new(SqliteBookshelfRepository::new(pool.clone()));
        let bookshelf_service = Arc::new(BookshelfService::new(bookshelf_repository));

        let folder_repository: Arc<dyn FolderRepository> =
            Arc::new(SqliteFolderRepository::new(pool.clone()));
        let folder_service = Arc::new(FolderService::new(folder_repository));

        Ok(Self {
            bookshelf_service,
            folder_service,
        })
    }
}
