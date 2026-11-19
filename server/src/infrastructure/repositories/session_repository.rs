use toasty::Db;

use crate::domain::{
    EntityCreate,
    identity::{
        session::{AuthAccessToken, AuthSession},
        user::value_objects::UserId,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum SessionRepositoryError {
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct SessionRepository {
    db: Db,
}

impl SessionRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create_session(
        &self,
        session: AuthSession,
        access_token: AuthAccessToken,
    ) -> Result<(), SessionRepositoryError> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(map_session_db_error)?;
        session
            .create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        access_token
            .create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        tx.commit().await.map_err(map_session_db_error)
    }

    /// Atomically attach an access token only if the session still exists.
    pub async fn add_access_token(
        &self,
        user_id: &UserId,
        access_token: AuthAccessToken,
    ) -> Result<bool, SessionRepositoryError> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(map_session_db_error)?;
        let session = AuthSession::filter(
            AuthSession::fields()
                .refresh_hash()
                .eq(access_token.refresh_hash.clone())
                .and(AuthSession::fields().user_id().inner().eq(*user_id.as_ref())),
        )
        .first()
        .exec(&mut tx)
        .await
        .map_err(map_session_db_error)?;
        if session.is_none() {
            return Ok(false);
        }
        access_token
            .create()
            .exec(&mut tx)
            .await
            .map_err(map_session_db_error)?;
        tx.commit().await.map_err(map_session_db_error)?;
        Ok(true)
    }

    pub async fn contains_access_token(
        &self,
        access_hash: String,
    ) -> Result<bool, SessionRepositoryError> {
        let access =
            AuthAccessToken::filter(AuthAccessToken::fields().access_hash().eq(access_hash))
                .first()
                .exec(&mut self.db.clone())
                .await
                .map_err(map_session_db_error)?;
        Ok(access.is_some())
    }

    /// Idempotently revoke the session and every access token belonging to it.
    pub async fn revoke(
        &self,
        user_id: &UserId,
        refresh_hash: String,
    ) -> Result<(), SessionRepositoryError> {
        AuthSession::filter(
            AuthSession::fields()
                .refresh_hash()
                .eq(refresh_hash)
                .and(AuthSession::fields().user_id().inner().eq(*user_id.as_ref())),
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
