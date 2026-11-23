use std::sync::Arc;

use axum::{extract::FromRequestParts, http::header::AUTHORIZATION};
use shiro_application::identity::{
    authentication::{commands::VerifyAccessTokenCommand, errors::AuthenticationApplicationError},
    errors::IdentityApplicationError,
};
use shiro_infrastructure::library_runtime::{
    SqliteBookService, SqliteLibraryContext, SqliteTagService,
};

use crate::{error::AppError, state::AppState};

pub struct User {
    pub id: String,
}

pub struct RoutedState {
    pub user_id: String,
    pub library: SqliteLibraryContext,
}

impl RoutedState {
    pub fn book_service(&self) -> SqliteBookService {
        self.library.book_service()
    }

    pub fn tag_service(&self) -> SqliteTagService {
        self.library.tag_service()
    }
}

impl FromRequestParts<AppState> for RoutedState {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthenticationApplicationError::MissingBearerToken)?;

        let mut header_parts = raw_header.splitn(2, ' ');
        let scheme = header_parts.next().unwrap_or_default();
        let token = header_parts.next().unwrap_or_default().trim();

        if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() {
            return Err(AuthenticationApplicationError::MissingBearerToken.into());
        }

        let user_id = state
            .auth_service
            .verify_access_token(VerifyAccessTokenCommand {
                token: token.to_owned(),
            })
            .await?
            .user_id;

        let library = state
            .user_database_provider
            .route_library(&user_id, Arc::clone(&state.bookfile_service))
            .map_err(|_| {
                IdentityApplicationError::Authentication(
                    AuthenticationApplicationError::InvalidToken,
                )
            })?;

        Ok(RoutedState { user_id, library })
    }
}
