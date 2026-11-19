use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{str::FromStr, sync::Mutex};

use anyhow::Context;
use argon2::Params;
use dashmap::DashMap;
use pdfium_render::prelude::{Pdfium, PdfiumError};
use toasty::Db;
use tracing::{debug, info, warn};

use crate::application::identity::authentication::errors::AuthenticationApplicationError;
use crate::application::identity::authentication::service::AuthenticationService;
use crate::application::identity::crypto::service::CryptoService;
use crate::application::identity::errors::IdentityApplicationError;
use crate::application::identity::user::service::UserService;

use crate::domain::{EntityId, identity::user::entity::User};
use crate::{
    application::library::book::file::service::BookFileService,
    config::Config,
    infrastructure::{db, dependencies, storage::book_file_storage::BookFileStorage},
};

// 允许AppState可被重复build（供单元测试使用）
static PDFIUM: Mutex<Option<Arc<Mutex<Pdfium>>>> = Mutex::new(None);

fn bind_system_pdfium() -> Result<Pdfium, PdfiumError> {
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(error) => Err(error),
    }
}

fn shared_pdfium(libs_dir: &Path) -> anyhow::Result<Arc<Mutex<Pdfium>>> {
    let mut shared = PDFIUM
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium initialization lock poisoned"))?;
    if let Some(pdfium) = shared.as_ref() {
        return Ok(pdfium.clone());
    }

    let local_library = Pdfium::pdfium_platform_library_name_at_path(libs_dir);
    let pdfium = if local_library.is_file() {
        let resolved_path =
            std::path::absolute(&local_library).unwrap_or_else(|_| local_library.clone());
        match Pdfium::bind_to_library(&resolved_path) {
            Ok(bindings) => {
                info!(path = %resolved_path.display(), "loaded Pdfium library");
                Pdfium::new(bindings)
            }
            Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
            Err(local_error) => {
                warn!(
                    path = %resolved_path.display(),
                    error = %local_error,
                    "failed to load Pdfium from libs directory, falling back to system library"
                );
                bind_system_pdfium().map_err(|system_error| {
                    anyhow::anyhow!(
                        "failed to load Pdfium from {} ({local_error}) and system library ({system_error})",
                        resolved_path.display()
                    )
                })?
            }
        }
    } else {
        debug!(
            path = %local_library.display(),
            "Pdfium library not found in libs directory, loading system library"
        );
        bind_system_pdfium().with_context(|| {
            format!(
                "failed to load Pdfium (not found at {} and system library unavailable)",
                local_library.display()
            )
        })?
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

    pub user_database_provider: Arc<UserDatabaseProvider>,
}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.root_directory)?;
        std::fs::create_dir_all(config.root_directory.join("users"))?;

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
        let pdfium = shared_pdfium(&dependencies::libs_dir(&config.root_directory))?;

        let identity_db =
            db::pool::connect_identity(&config.root_directory.join("user.db")).await?;

        let user_database_provider = Arc::new(UserDatabaseProvider::new(&config));
        let users = User::all().exec(&mut identity_db.clone()).await?;
        for user in users {
            user_database_provider
                .append(&user.id().to_string())
                .await?;
        }

        let mut book_file_storage = BookFileStorage::new(
            config.root_directory.join("books"),
            String::from_str("book")?,
            pdfium,
        );
        book_file_storage.scan()?;

        Ok(Self {
            user_service: Arc::new(UserService::new(
                identity_db.clone(),
                user_database_provider.clone(),
                crypto_service.clone(),
            )),
            auth_service: Arc::new(AuthenticationService::new(
                identity_db,
                crypto_service.clone(),
            )),
            bookfile_service: Arc::new(BookFileService::new(book_file_storage)),
            user_database_provider,
        })
    }
}

pub struct UserDatabaseProvider {
    root_directory: PathBuf,
    map: DashMap<String, Db>,
}

impl UserDatabaseProvider {
    pub fn new(config: &Config) -> Self {
        Self {
            root_directory: config.root_directory.clone(),
            map: DashMap::new(),
        }
    }
    pub fn route(&self, user_id: &str) -> Result<Db, IdentityApplicationError> {
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
        let pool = db::pool::connect_library(&base_path.join("library.db")).await?;
        self.map.insert(user_id.to_owned(), pool);
        Ok(())
    }

    pub async fn remove(&self, user_id: &str) -> anyhow::Result<()> {
        if let Some(pool) = self.map.remove(user_id) {
            drop(pool.1);
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
