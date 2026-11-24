use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use argon2::Params;
use dashmap::DashMap;
use shiro_application::{
    identity::{
        authentication::{errors::AuthenticationApplicationError, service::AuthenticationService},
        errors::IdentityApplicationError,
        ports::{UserLibraryProvisioner, UserRepository},
        user::service::UserService,
    },
    library::{
        book::file::service::{BookFileService, UploadTimeoutPolicy},
        context::LibraryContext,
    },
};
use shiro_infrastructure::{
    crypto::CryptoService,
    db::pool::{connect_identity, connect_library},
    library_runtime::{SqliteBookFileService, SqliteLibraryContext},
    pdfium::load_server_pdfium,
    repositories::{
        book_repository::SqliteBookRepository,
        session_repository::SqliteSessionRepository,
        tag_repository::SqliteTagRepository,
        user_repository::SqliteUserRepository,
    },
    storage::book_file_storage::LocalBookFileStorage,
};
use toasty::Db;

use crate::{app::DEFAULT_STANDARD_REQUEST_TIMEOUT, config::Config, infrastructure::dependencies};

pub type ServerUserService = UserService<SqliteUserRepository, UserDatabaseProvider, CryptoService>;
pub type ServerAuthenticationService =
    AuthenticationService<SqliteUserRepository, SqliteSessionRepository, CryptoService>;

#[derive(Clone)]
pub struct AppState {
    pub user_service: Arc<ServerUserService>,
    pub auth_service: Arc<ServerAuthenticationService>,
    pub bookfile_service: Arc<SqliteBookFileService>,
    pub user_database_provider: Arc<UserDatabaseProvider>,
    pub max_upload_size: usize,
    pub standard_request_timeout: Duration,
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
        let pdfium = load_server_pdfium(&dependencies::libs_dir(&config.root_directory))?;

        let identity_db = connect_identity(&config.root_directory.join("user.db")).await?;
        let user_repository = SqliteUserRepository::new(identity_db.clone());
        let session_repository = SqliteSessionRepository::new(identity_db);

        let user_database_provider = Arc::new(UserDatabaseProvider::new(config));
        for user in user_repository.list(&[]).await? {
            user_database_provider
                .append(&user.id().to_string())
                .await?;
        }

        let book_file_storage =
            LocalBookFileStorage::new(config.root_directory.join("books"), "book", Some(pdfium))?
                .scan_async()
                .await?;

        let base_timeout = Duration::from_secs(config.server.upload_base_timeout_secs);
        let timeout_policy = UploadTimeoutPolicy {
            base_timeout,
            min_bytes_per_sec: config.server.upload_min_bytes_per_sec,
            min_timeout: base_timeout,
            ..UploadTimeoutPolicy::default()
        };

        Ok(Self {
            user_service: Arc::new(UserService::new(
                user_repository.clone(),
                Arc::clone(&user_database_provider),
                Arc::clone(&crypto_service),
            )),
            auth_service: Arc::new(AuthenticationService::new(
                user_repository,
                session_repository,
                crypto_service,
            )),
            bookfile_service: Arc::new(BookFileService::with_timeout_policy(
                book_file_storage,
                timeout_policy,
            )),
            user_database_provider,
            max_upload_size: config.server.max_upload_size,
            standard_request_timeout: DEFAULT_STANDARD_REQUEST_TIMEOUT,
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

    pub fn route_db(&self, user_id: &str) -> Result<Db, IdentityApplicationError> {
        if let Some(entry) = self.map.get(user_id) {
            Ok(entry.value().clone())
        } else {
            Err(IdentityApplicationError::Authentication(
                AuthenticationApplicationError::NotFound,
            ))
        }
    }

    pub fn route_library(
        &self,
        user_id: &str,
        book_files: Arc<SqliteBookFileService>,
    ) -> Result<SqliteLibraryContext, IdentityApplicationError> {
        let db = self.route_db(user_id)?;
        Ok(LibraryContext::new(
            SqliteBookRepository::new(db.clone()),
            SqliteTagRepository::new(db.clone()),
            book_files,
        ))
    }

    pub async fn append(&self, user_id: &str) -> anyhow::Result<()> {
        let base_path = self.root_directory.join("users").join(user_id);
        std::fs::create_dir_all(&base_path)?;
        std::fs::create_dir_all(base_path.join("covers"))?;
        std::fs::create_dir_all(base_path.join("notes"))?;
        let pool = connect_library(&base_path.join("library.db")).await?;
        self.map.insert(user_id.to_owned(), pool);
        Ok(())
    }

    pub async fn remove(&self, user_id: &str) -> anyhow::Result<()> {
        if let Some((_, pool)) = self.map.remove(user_id) {
            drop(pool);
            std::fs::remove_dir_all(self.root_directory.join("users").join(user_id))?;
            Ok(())
        } else {
            anyhow::bail!("user library database pool not found for {user_id}")
        }
    }

    pub fn clear(&self, user_id: &str) -> anyhow::Result<()> {
        let path = self.root_directory.join("users").join(user_id);
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}

impl UserLibraryProvisioner for UserDatabaseProvider {
    async fn provision_library(&self, user_id: &str) -> anyhow::Result<()> {
        self.append(user_id).await
    }

    async fn remove_library(&self, user_id: &str) -> anyhow::Result<()> {
        self.remove(user_id).await
    }

    fn cleanup_failed_provision(&self, user_id: &str) -> anyhow::Result<()> {
        self.clear(user_id)
    }
}
