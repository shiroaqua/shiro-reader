use std::path::PathBuf;
use std::sync::Arc;
use std::{str::FromStr, sync::Mutex};

use anyhow::Context;
use argon2::Params;
use dashmap::DashMap;
use pdfium_render::prelude::{Pdfium, PdfiumError};
use sqlx::{Pool, Sqlite};

use crate::application::identity::authentication::errors::AuthenticationApplicationError;
use crate::application::identity::authentication::service::AuthenticationService;
use crate::application::identity::crypto::service::CryptoService;
use crate::application::identity::errors::IdentityApplicationError;
use crate::application::identity::user::service::UserService;

use crate::application::identity::user::ports::UserRepository;
use crate::infrastructure::repositories::sqlite_user_repository::SqliteUserRepository;
use crate::{
    application::library::book::file::service::BookFileService,
    config::Config,
    infrastructure::{db, storage::book_file_storage::BookFileStorage},
};

// 允许AppState可被重复build（供单元测试使用）
static PDFIUM: Mutex<Option<Arc<Mutex<Pdfium>>>> = Mutex::new(None);

fn shared_pdfium() -> anyhow::Result<Arc<Mutex<Pdfium>>> {
    let mut shared = PDFIUM.lock().map_err(|_| anyhow::anyhow!("Pdfium initialization lock poisoned"))?;
    if let Some(pdfium) = shared.as_ref() {
        return Ok(pdfium.clone());
    }

    let pdfium = match Pdfium::bind_to_system_library() {
        Ok(bindings) => Pdfium::new(bindings),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
        Err(error) => return Err(error.into()),
    };
    let pdfium = Arc::new(Mutex::new(pdfium));
    *shared = Some(pdfium.clone());
    Ok(pdfium)
}

#[derive(Clone)]
pub struct AppState {
    pub user_service: Arc<UserService>,
    pub auth_service: Arc<AuthenticationService>,
    pub bookfile_service: Arc<BookFileService>,

    pub user_sqlpool_provider: Arc<UserSqlPoolProvider>,
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        let crypto_params = Params::new(
            config.crypto.argon2_memory_cost,
            config.crypto.argon2_time_cost,
            config.crypto.argon2_parallelism,
            Some(config.crypto.argon2_output_length),
        )
        .context("invalid Argon2 configuration")?;
        let crypto_service = Arc::new(CryptoService::new(
            crypto_params,
            config.crypto.jwt_secret.as_bytes(),
            config.crypto.jwt_issuer.clone(),
            config.crypto.jwt_audience.clone(),
            config.crypto.jwt_access_ttl_secs,
            config.crypto.jwt_refresh_ttl_secs,
            config.crypto.jwt_validation_leeway_secs,
        ));
        let pdfium = shared_pdfium()?;

        let pool = db::pool::connect(
            &config.root_directory.join("user.db"),
            config.database.max_connections,
        )
        .await?;
        sqlx::migrate!("./migrations/identity").run(&pool).await?;

        let user_repository: Arc<dyn UserRepository> =
            Arc::new(SqliteUserRepository::new(pool.clone()));

        let user_sqlpool_provider = Arc::new(UserSqlPoolProvider::new(&config));
        let ids = user_repository.list_ids().await?;
        for id in ids {
            user_sqlpool_provider.append(&id).await?;
        }

        let mut book_file_storage = BookFileStorage::new(
            config.root_directory.join("books"),
            String::from_str("book")?,
            pdfium,
        );
        book_file_storage.scan()?;

        Ok(Self {
            user_service: Arc::new(UserService::new(
                user_repository.clone(),
                user_sqlpool_provider.clone(),
                crypto_service.clone(),
            )),
            auth_service: Arc::new(AuthenticationService::new(
                user_repository.clone(),
                crypto_service.clone(),
            )),
            bookfile_service: Arc::new(BookFileService::new(book_file_storage)),
            user_sqlpool_provider,
        })
    }
}



pub struct UserSqlPoolProvider {
    root_directory: PathBuf,
    max_connections: u32,
    map: DashMap<String, Pool<Sqlite>>,
}

impl UserSqlPoolProvider {
    pub fn new(config: &Config) -> Self {
        Self {
            root_directory: config.root_directory.clone(),
            max_connections: config.database.max_connections,
            map: DashMap::new(),
        }
    }
    pub fn route(&self, user_id: &str) -> Result<Pool<Sqlite>, IdentityApplicationError> {
        if let Some(t) = self.map.get(user_id) {
            Ok(t.value().clone())
        } else {
            Err(IdentityApplicationError::Authentication(
                AuthenticationApplicationError::NotFound,
            ))
        }
    }

    pub async fn append(&self, user_id: &str) -> anyhow::Result<()> {
        let base_path = self.root_directory.join("users").join(&user_id);
        std::fs::create_dir_all(&base_path)?;
        std::fs::create_dir_all(&base_path.join("covers"))?;
        std::fs::create_dir_all(&base_path.join("notes"))?;
        let pool = db::pool::connect(
            &base_path.join("library.db"),
            self.max_connections,
        )
        .await?;
        sqlx::migrate!("./migrations/library").run(&pool).await?;
        self.map.insert(user_id.to_owned(), pool);
        Ok(())
    }

    pub async fn remove(&self, user_id: &str) -> anyhow::Result<()> {
        if let Some(pool) = self.map.remove(user_id) {
            pool.1.close().await;
            std::fs::remove_dir_all(self.root_directory.join("users").join(&user_id))?;
            Ok(())
        } else {
            anyhow::bail!("todo (remove sql pool failed.)")
        }
    }

    pub fn clear(&self, user_id: &str) -> anyhow::Result<()> {     
        std::fs::remove_dir_all(self.root_directory.join("users").join(&user_id))?;
        Ok(())
    }

}