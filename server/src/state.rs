use std::sync::Arc;
use std::path::PathBuf;

use crate::{
    application::bookshelf::{ports::DirectoryRepository, service::DirectoryService},
    config::Config,
    infrastructure::{db, repositories::sqlite_directory_repository::SqliteDirectoryRepository},
};


#[derive(Clone)]
pub struct AppState {
    pub directory_service: Arc<DirectoryService>,
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(PathBuf::from("users").join("default"))?;
        
        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;

        let directory_repository: Arc<dyn DirectoryRepository> =
            Arc::new(SqliteDirectoryRepository::new(pool));
        let directory_service = Arc::new(DirectoryService::new(directory_repository));

        Ok(Self { directory_service })
    }
}
