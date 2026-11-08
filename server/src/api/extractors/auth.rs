use axum::{extract::FromRequestParts, http::header::AUTHORIZATION};

use crate::{
    application::{
        identity::{authentication::{
            commands::VerifyAccessToeknCommand, errors::AuthenticationApplicationError,
        }, errors::IdentityApplicationError}, library::{book::service::BookService, tag::service::TagService},
    }, error::AppError, infrastructure::repositories::{sqlite_book_repository::SqliteBookRepository, sqlite_tag_repository::SqliteTagRepository}, state::AppState,
};

pub struct User {
    pub id: String,
}

pub struct RoutedState {
    pub book_service: BookService<SqliteBookRepository>,
    pub tag_service: TagService<SqliteTagRepository>,
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

        let mut parts = raw_header.splitn(2, ' ');

        let scheme = parts.next().unwrap_or_default();
        let token = parts.next().unwrap_or_default().trim();

        if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() {
            return Err(AuthenticationApplicationError::MissingBearerToken.into());
        }

        let user_id = state
            .auth_service
            .verify_access_token(VerifyAccessToeknCommand {
                token: token.to_owned(),
            })
            .await?
            .user_id;

        let pool = state.user_sqlpool_provider.route(&user_id).map_err(|_| IdentityApplicationError::Authentication(AuthenticationApplicationError::InvalidToken))?;


        Ok(RoutedState {
            book_service: BookService::new(
                SqliteBookRepository::new(pool.clone()),
                state.bookfile_service.clone(),
            ),
            tag_service: TagService::new(SqliteTagRepository::new(pool)),
        })
    }
}
