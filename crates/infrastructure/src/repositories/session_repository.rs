use shiro_application::identity::ports::{SessionRepository, SessionRepositoryError};
use shiro_domain::identity::{
    session::{AuthAccessToken, AuthSession},
    user::value_objects::UserId,
};
use toasty::Db;

use crate::db::models::{AuthAccessTokenRecord, AuthSessionRecord};

#[derive(Clone)]
pub struct SqliteSessionRepository {
    db: Db,
}

impl SqliteSessionRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl SessionRepository for SqliteSessionRepository {
    async fn create_session(
        &self,
        session: AuthSession,
        access_token: AuthAccessToken,
    ) -> Result<(), SessionRepositoryError> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(map_session_db_error)?;
        AuthSessionRecord::from(session)
            .into_create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        AuthAccessTokenRecord::from(access_token)
            .into_create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        tx.commit().await.map_err(map_session_db_error)
    }

    async fn add_access_token(
        &self,
        user_id: &UserId,
        access_token: AuthAccessToken,
    ) -> Result<bool, SessionRepositoryError> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(map_session_db_error)?;
        let session = AuthSessionRecord::filter(
            AuthSessionRecord::fields()
                .refresh_hash()
                .eq(access_token.refresh_hash().to_owned())
                .and(AuthSessionRecord::fields().user_id().eq(user_id.as_uuid())),
        )
        .first()
        .exec(&mut tx)
        .await
        .map_err(map_session_db_error)?;
        if session.is_none() {
            return Ok(false);
        }
        AuthAccessTokenRecord::from(access_token)
            .into_create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        tx.commit().await.map_err(map_session_db_error)?;
        Ok(true)
    }

    async fn contains_access_token(
        &self,
        access_hash: String,
    ) -> Result<bool, SessionRepositoryError> {
        let access = AuthAccessTokenRecord::filter(
            AuthAccessTokenRecord::fields()
                .access_hash()
                .eq(access_hash),
        )
        .first()
        .exec(&mut self.db.clone())
        .await
        .map_err(map_session_db_error)?;
        Ok(access.is_some())
    }

    async fn revoke(
        &self,
        user_id: &UserId,
        refresh_hash: String,
    ) -> Result<(), SessionRepositoryError> {
        AuthSessionRecord::filter(
            AuthSessionRecord::fields()
                .refresh_hash()
                .eq(refresh_hash)
                .and(AuthSessionRecord::fields().user_id().eq(user_id.as_uuid())),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(map_session_db_error)
    }
}

fn map_session_db_error(error: toasty::Error) -> SessionRepositoryError {
    SessionRepositoryError::Storage(error.into())
}
