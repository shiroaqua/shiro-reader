use std::path::PathBuf;

use crate::{
    config::Config,
    infrastructure::db,
};

#[derive(Clone)]
pub struct AppState {

}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        // 编写用户系统前暂时先使用硬编码
        std::fs::create_dir_all(PathBuf::from("users").join("default"))?;
        
        let pool = db::pool::connect(&config.database).await?;
        db::migrate(&pool).await?;

        Ok(Self {})
    }
}
