use std::sync::Arc;
use std::path::PathBuf;

use crate::{
    application::library::bookshelf::folder::{ports::FolderRepository, service::FolderService},
    config::Config,
    infrastructure::{db, repositories::sqlite_folder_repository::SqliteFolderRepository},
};


#[derive(Clone)]
pub struct AppState {
    pub folder_service: Arc<FolderService>,
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(PathBuf::from("users").join("default"))?;
        
        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;

        let folder_repository: Arc<dyn FolderRepository> =
            Arc::new(SqliteFolderRepository::new(pool));
        let folder_service = Arc::new(FolderService::new(folder_repository));

        Ok(Self { folder_service })
    }
}
