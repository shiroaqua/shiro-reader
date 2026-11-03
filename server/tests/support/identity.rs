use super::*;
use shiro_reader_server::application::identity::{
    authentication::commands::VerifyAccessToeknCommand,
    errors::IdentityApplicationError,
    user::{commands::{UserRegisterCommand, UserReanmeCommand, UserChangePasswordCommand, UserUnregisterCommand},},
};
pub use shiro_reader_server::application::identity::user::errors::UserApplicationError as UserError;

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub(super) access_token: String,
    refresh_token: String,
}

pub struct UserResult(Result<(), IdentityApplicationError>);
impl UserResult {
    pub fn assert_ok(&self) { self.0.as_ref().expect("user operation should succeed"); }
    pub fn assert_error(&self, expected: UserError) {
        let Err(IdentityApplicationError::User(actual)) = &self.0 else {
            panic!("expected user error {expected:?}, got {:?}", self.0);
        };
        assert_eq!(std::mem::discriminant(actual), std::mem::discriminant(&expected), "{actual:?}");
    }
    pub fn is_ok(&self) -> bool { self.0.is_ok() }
}

impl TestApp {
    pub async fn register(&self, name: &str) -> UserResult {
        UserResult(self.state.user_service.register(UserRegisterCommand {
            name: name.into(), password: PASSWORD.into(),
        }).await)
    }
    pub async fn session(&self, name: &str) -> Session {
        self.register(name).await.assert_ok();
        self.login(name, PASSWORD).await
    }
    pub async fn attempt_login(&self, name: &str, password: &str) -> HttpResponse {
        self.send(Method::POST, LOGIN, Some(json!({"username": name, "password": password})), None).await
    }
    pub async fn login(&self, name: &str, password: &str) -> Session {
        let response = self.attempt_login(name, password).await;
        response.assert_status(200);
        let json = response.json();
        assert_eq!(json.as_object().unwrap().len(), 2, "login exposes only tokens");
        let access_token = text(&json, "access_token");
        // Identity comes from the application's verifier, not a storage query.
        let user_id = self.state.auth_service.verify_access_token(VerifyAccessToeknCommand {
            token: access_token.clone(),
        }).await.expect("verify issued token").user_id;
        uuid::Uuid::parse_str(&user_id).expect("valid user ID");
        Session { user_id, access_token, refresh_token: text(&json, "refresh_token") }
    }
    pub async fn refresh(&self, session: &Session) -> Session {
        let response = self.refresh_token(&session.refresh_token).await;
        response.assert_status(200);
        let json = response.json();
        assert_eq!(json.as_object().unwrap().len(), 1, "refresh exposes only access token");
        let access_token = text(&json, "access_token");
        let identity = self.state.auth_service.verify_access_token(VerifyAccessToeknCommand {
            token: access_token.clone(),
        }).await.unwrap();
        assert_eq!(identity.user_id, session.user_id);
        Session { access_token, ..session.clone() }
    }
    pub async fn assert_refresh_result_revoked(&self, response: HttpResponse) {
        if response.status == StatusCode::OK {
            self.access_with_token(&text(&response.json(), "access_token")).await.assert_error(HttpError::InvalidToken);
        } else {
            response.assert_error(HttpError::InvalidToken);
        }
    }
    pub async fn logout(&self, session: &Session) -> HttpResponse {
        self.logout_token(&session.refresh_token).await
    }
    pub async fn logout_token(&self, token: &str) -> HttpResponse {
        self.send(Method::POST, "/api/v1/auth/logout", Some(json!({"token": token})), None).await
    }
    pub async fn logout_with_access(&self, session: &Session) -> HttpResponse {
        self.logout_token(&session.access_token).await
    }
    pub async fn refresh_token(&self, token: &str) -> HttpResponse {
        self.send(Method::POST, REFRESH, Some(json!({"token": token})), None).await
    }
    pub async fn refresh_with_access(&self, session: &Session) -> HttpResponse {
        self.refresh_token(&session.access_token).await
    }
    pub async fn attempt_refresh(&self, session: &Session) -> HttpResponse {
        self.refresh_token(&session.refresh_token).await
    }
    pub async fn rename_user(&self, id: &str, name: &str) -> UserResult {
        UserResult(self.state.user_service.rename(UserReanmeCommand { id: id.into(), name: name.into() }).await)
    }
    pub async fn change_password(&self, id: &str, password: &str) -> UserResult {
        UserResult(self.state.user_service.change_password(UserChangePasswordCommand { id: id.into(), password: password.into() }).await)
    }
    pub async fn unregister(&self, id: &str) -> UserResult {
        UserResult(self.state.user_service.unregister(UserUnregisterCommand { id: id.into() }).await)
    }
    pub async fn access_books(&self, session: &Session) -> HttpResponse {
        self.send(Method::GET, BOOKS, None, Some(session)).await
    }
    pub async fn access_with_header(&self, authorization: Option<&str>) -> HttpResponse {
        self.request(Method::GET, BOOKS, Body::empty(), None, authorization).await
    }
    pub async fn access_with_token(&self, token: &str) -> HttpResponse {
        self.access_with_header(Some(&format!("Bearer {token}"))).await
    }
    pub async fn access_with_refresh(&self, session: &Session) -> HttpResponse {
        self.access_with_token(&session.refresh_token).await
    }
    pub async fn access_with_mixed_case_bearer(&self, session: &Session) -> HttpResponse {
        self.access_with_header(Some(&format!("bEaReR {}", session.access_token))).await
    }
    pub async fn anonymous_library_requests(&self) -> Vec<HttpResponse> {
        let mut responses = Vec::new();
        for (method, uri, body) in [
            (Method::GET, BOOKS.into(), None),
            (Method::POST, BOOKS.into(), Some(json!({"title": "Book", "hash": UNKNOWN_HASH}))),
            (Method::PATCH, format!("{BOOKS}/{UNKNOWN_ID}"), Some(json!({"title": "Renamed"}))),
            (Method::DELETE, format!("{BOOKS}/{UNKNOWN_ID}"), None),
            (Method::GET, format!("{BOOKS}/{UNKNOWN_ID}/cover"), None),
            (Method::GET, format!("{FILES}?hash={UNKNOWN_HASH}"), None),
            (Method::GET, format!("{FILES}/{UNKNOWN_HASH}"), None),
        ] {
            responses.push(self.send(method, &uri, body, None).await);
        }
        responses.push(self.upload_parts(None, &[UNKNOWN_HASH], Some(b"file")).await);
        responses
    }
    pub async fn malformed_auth_requests(&self) -> Vec<(HttpResponse, u16)> {
        let mut responses = Vec::new();
        for uri in [LOGIN, REFRESH, "/api/v1/auth/logout"] {
            for (body, content_type, status) in [
                ("{}", "application/json", 422), ("{", "application/json", 400),
                ("{}", "text/plain", 415),
                (r#"{"username":42,"password":[],"token":42}"#, "application/json", 422),
            ] {
                responses.push((self.request(Method::POST, uri, Body::from(body), Some(content_type), None).await, status));
            }
        }
        responses
    }
}
