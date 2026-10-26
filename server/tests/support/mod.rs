#![allow(dead_code, unused_imports)]

pub mod crypto;
mod identity;
mod lifecycle;
mod library;
mod runtime;

pub use identity::*;
pub use lifecycle::Restart;
pub use library::*;

use axum::{Router, body::{Body, Bytes, to_bytes}, http::{Method, Request, StatusCode, HeaderMap}};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shiro_reader_server::{app::build_router, state::AppState};
use tempfile::TempDir;
use tower::ServiceExt;

pub const PASSWORD: &str = "test-password";
pub const UNKNOWN_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
pub const INVALID_ID: &str = "not-a-uuid";
pub const UNKNOWN_HASH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
pub const INVALID_HASH: &str = "not-a-hash";
pub const INVALID_TITLES: [&str; 6] = [" Book", "Book ", "\tBook", "Book\t", "\nBook", "Book\n"];
const BOOKS: &str = "/api/v1/library/bookshelf/books";
const FILES: &str = "/api/v1/library/files/book";
const LOGIN: &str = "/api/v1/auth/login";
const REFRESH: &str = "/api/v1/auth/refresh";

pub struct TestApp {
    state: AppState,
    router: Router,
    _directory: Option<TempDir>,
}

impl TestApp {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().expect("create isolated application root");
        let mut app = Self::at(directory.path()).await;
        app._directory = Some(directory);
        app
    }

    async fn at(root: &std::path::Path) -> Self {
        let state = AppState::build(&runtime::config(root)).await.expect("build application");
        Self { router: build_router(state.clone()), state, _directory: None }
    }

    async fn request(&self, method: Method, uri: &str, body: Body,
        content_type: Option<&str>, authorization: Option<&str>) -> HttpResponse {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(value) = content_type { request = request.header("content-type", value); }
        if let Some(value) = authorization { request = request.header("authorization", value); }
        let response = self.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        HttpResponse { status: parts.status, headers: parts.headers,
            bytes: to_bytes(body, 16 * 1024 * 1024).await.expect("read response") }
    }

    async fn send(&self, method: Method, uri: &str, json: Option<Value>, session: Option<&Session>) -> HttpResponse {
        let authorization = session.map(|session| format!("Bearer {}", session.access_token));
        let content_type = json.as_ref().map(|_| "application/json");
        let body = json.map(|value| Body::from(serde_json::to_vec(&value).unwrap())).unwrap_or_else(Body::empty);
        self.request(method, uri, body, content_type, authorization.as_deref()).await
    }
}

pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    bytes: Bytes,
}

impl HttpResponse {
    fn json(&self) -> Value {
        assert!(self.headers["content-type"].to_str().unwrap().starts_with("application/json"));
        serde_json::from_slice(&self.bytes).expect("valid JSON response")
    }

    pub fn assert_status(&self, status: u16) {
        assert_eq!(self.status.as_u16(), status, "unexpected HTTP status");
    }

    pub fn assert_empty(&self, status: u16) {
        self.assert_status(status);
        assert!(self.bytes.is_empty(), "expected an empty response");
    }

    pub fn assert_error(&self, error: HttpError) {
        let (status, message) = error.contract();
        self.assert_status(status);
        assert_eq!(self.json(), json!({"error": {"message": message}}));
    }

    pub fn assert_png(&self) {
        self.assert_status(200);
        assert_eq!(self.headers["content-type"], "image/png");
        assert!(self.bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "cover bytes must match image/png");
        let image = image::load_from_memory_with_format(&self.bytes, image::ImageFormat::Png).unwrap();
        assert!(image.width() > 0 && image.height() > 0);
    }

    pub fn assert_file(&self, file: &SampleFile) {
        self.assert_status(200);
        assert_eq!(self.headers["content-type"], "application/octet-stream");
        assert_eq!(self.headers["x-book-hash"], file.hash());
        assert_eq!(self.headers["content-disposition"], format!("attachment; filename=\"{}.book\"", file.hash()));
        assert_eq!(self.bytes.as_ref(), file.bytes());
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HttpError {
    MissingTitle, InvalidTitle, UnchangedTitle, TitleConflict, InvalidBookId, BookNotFound,
    InvalidHash, FileNotFound, DuplicateFile, MissingHash, MissingFile, DuplicateHash, HashMismatch, UnsupportedFile,
    InvalidCredentials, InvalidUsername, MissingUsername, MissingBearer, InvalidToken, WrongTokenType, UserNotFound, AccountNotFound,
}

impl HttpError {
    fn contract(self) -> (u16, &'static str) {
        use HttpError::*;
        match self {
            MissingTitle => (400, "library.book.missing_title"),
            InvalidTitle => (400, "library.book.invalid_title_format"),
            UnchangedTitle => (400, "library.book.title_unchanged"),
            TitleConflict => (409, "library.book.title_conflict"),
            InvalidBookId => (400, "library.book.invalid_id"),
            BookNotFound => (404, "library.book.not_found"),
            InvalidHash => (400, "library.book.file.invalid_hash_format"),
            FileNotFound => (404, "library.book.file.not_found"),
            DuplicateFile => (409, "library.book.file.already_exists"),
            MissingHash => (400, "library.book.file.upload.missing_hash"),
            MissingFile => (400, "library.book.file.upload.missing_file"),
            DuplicateHash => (400, "library.book.file.upload.duplicate_hash"),
            HashMismatch => (400, "library.book.file.hash_mismatch"),
            UnsupportedFile => (400, "library.book.file.unsupported"),
            InvalidCredentials => (401, "identity.authentication.invalid_credentials"),
            InvalidUsername => (400, "identity.user.invalid_user_name_format"),
            MissingUsername => (400, "identity.user.missing_user_name"),
            MissingBearer => (401, "identity.authentication.missing_bearer_token"),
            InvalidToken => (401, "identity.authentication.invalid_token"),
            WrongTokenType => (401, "identity.authentication.invalid_token_type"),
            AccountNotFound => (404, "identity.user.not_found"),
            UserNotFound => (404, "identity.authentication.user_not_found"),
        }
    }
}

fn text(value: &Value, field: &str) -> String {
    let text = value[field].as_str().unwrap_or_else(|| panic!("missing string field: {field}"));
    assert!(!text.is_empty(), "empty field: {field}");
    text.into()
}
