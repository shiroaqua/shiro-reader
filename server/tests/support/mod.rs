#![allow(dead_code, unused_imports)]

pub mod crypto;
mod identity;
mod library;
mod lifecycle;
mod runtime;
mod tag;

pub use identity::*;
pub use library::*;
pub use lifecycle::Restart;
pub use tag::*;

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode},
};
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

    pub async fn at(root: &std::path::Path) -> Self {
        let state = AppState::build(&runtime::config(root))
            .await
            .expect("build application");
        Self {
            router: build_router(state.clone()),
            state,
            _directory: None,
        }
    }

    async fn request(
        &self,
        method: Method,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
        authorization: Option<&str>,
    ) -> HttpResponse {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(value) = content_type {
            request = request.header("content-type", value);
        }
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        HttpResponse {
            status: parts.status,
            headers: parts.headers,
            bytes: to_bytes(body, 16 * 1024 * 1024)
                .await
                .expect("read response"),
        }
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        json: Option<Value>,
        session: Option<&Session>,
    ) -> HttpResponse {
        let authorization = session.map(|session| format!("Bearer {}", session.access_token));
        let content_type = json.as_ref().map(|_| "application/json");
        let body = json
            .map(|value| Body::from(serde_json::to_vec(&value).unwrap()))
            .unwrap_or_else(Body::empty);
        self.request(method, uri, body, content_type, authorization.as_deref())
            .await
    }
}

pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    bytes: Bytes,
}

impl HttpResponse {
    fn json(&self) -> Value {
        assert!(
            self.headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
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
        assert_eq!(
            self.json(),
            json!({"error": {"code": expected_error_code(message), "message": message}})
        );
    }

    pub fn assert_png(&self) {
        self.assert_image(image::ImageFormat::Png);
    }

    pub fn assert_image(&self, format: image::ImageFormat) {
        self.assert_status(200);
        assert_eq!(self.headers["content-type"], format.to_mime_type());
        assert_eq!(image::guess_format(&self.bytes).unwrap(), format);
        let image = image::load_from_memory_with_format(&self.bytes, format).unwrap();
        assert!(image.width() > 0 && image.height() > 0);
    }

    pub fn assert_cover_bytes(&self, expected: &[u8]) {
        assert_eq!(self.bytes.as_ref(), expected);
    }

    pub fn assert_blank_cover(&self) {
        self.assert_png();
        let image = image::load_from_memory(&self.bytes).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (1000, 1600));
        assert!(image.pixels().all(|pixel| pixel.0 == [255, 255, 255, 255]));
    }

    pub fn assert_file(&self, file: &SampleFile) {
        self.assert_status(200);
        assert_eq!(self.headers["content-type"], "application/octet-stream");
        assert_eq!(self.headers["x-book-hash"], file.hash());
        assert_eq!(
            self.headers["content-disposition"],
            format!("attachment; filename=\"{}.book\"", file.hash())
        );
        assert_eq!(self.bytes.as_ref(), file.bytes());
    }
}

fn expected_error_code(message: &str) -> u32 {
    match message {
        "identity.authentication.invalid_credentials" => 1001,
        "identity.authentication.missing_bearer_token" => 1002,
        "identity.authentication.invalid_token" => 1003,
        "identity.authentication.user_not_found" => 1004,
        "identity.authentication.invalid_token_type" => 1005,
        "identity.user.invalid_id" => 1101,
        "identity.user.missing_user_name" => 1102,
        "identity.user.invalid_user_name_format" => 1103,
        "identity.user.invalid_password_hash" => 1104,
        "identity.user.not_found" => 1105,
        "identity.user.name_conflict" => 1106,
        "identity.user.name_unchanged" => 1107,
        "identity.user.password_hash_unchanged" => 1108,
        "library.book.invalid_id" => 2001,
        "library.book.missing_title" => 2002,
        "library.book.invalid_title_format" => 2003,
        "library.book.invalid_hash_format" => 2004,
        "library.book.invalid_file" => 2005,
        "library.book.not_found" => 2006,
        "library.book.title_conflict" => 2007,
        "library.book.title_unchanged" => 2008,
        "library.book.already_deleted" => 2009,
        "library.book.not_deleted" => 2010,
        "library.book.version_conflict" => 2011,
        "library.book.file.upload.conflict" => 2101,
        "library.book.file.already_exists" => 2102,
        "library.book.file.invalid_hash_format" => 2103,
        "library.book.file.hash_mismatch" => 2104,
        "library.book.file.not_found" => 2105,
        "library.book.file.unsupported" => 2106,
        "library.book.file.upload.duplicate_hash" => 2110,
        "library.book.file.upload.missing_hash" => 2111,
        "library.book.file.upload.missing_file" => 2112,
        "library.tag.invalid_id" => 2201,
        "library.tag.missing_name" => 2202,
        "library.tag.invalid_name_format" => 2203,
        "library.tag.name_unchanged" => 2204,
        "library.tag.invalid_color" => 2205,
        "library.tag.color_unchanged" => 2206,
        "library.tag.invalid_value_regex" => 2207,
        "library.tag.value_regex_not_allowed" => 2208,
        "library.tag.value_required" => 2209,
        "library.tag.value_not_allowed" => 2210,
        "library.tag.value_regex_mismatch" => 2211,
        "library.tag.already_deleted" => 2212,
        "library.tag.not_deleted" => 2213,
        "library.tag.not_found" => 2214,
        "library.tag.parent_not_found" => 2215,
        "library.tag.invalid_move" => 2216,
        "library.tag.invalid_delete_strategy" => 2217,
        "library.tag.sibling_name_conflict" => 2218,
        "common.payload_too_large" => 9001,
        "common.internal_server_error" => 9002,
        _ => panic!("missing expected error code for {message}"),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HttpError {
    MissingTitle,
    InvalidTitle,
    UnchangedTitle,
    TitleConflict,
    BookAlreadyDeleted,
    BookNotDeleted,
    InvalidBookId,
    BookNotFound,
    InvalidHash,
    FileNotFound,
    DuplicateFile,
    MissingHash,
    MissingFile,
    DuplicateHash,
    HashMismatch,
    UnsupportedFile,
    InvalidCredentials,
    InvalidUsername,
    MissingUsername,
    MissingBearer,
    InvalidToken,
    WrongTokenType,
    UserNotFound,
    AccountNotFound,
}

impl HttpError {
    fn contract(self) -> (u16, &'static str) {
        use HttpError::*;
        match self {
            MissingTitle => (400, "library.book.missing_title"),
            InvalidTitle => (400, "library.book.invalid_title_format"),
            UnchangedTitle => (400, "library.book.title_unchanged"),
            TitleConflict => (409, "library.book.title_conflict"),
            BookAlreadyDeleted => (409, "library.book.already_deleted"),
            BookNotDeleted => (409, "library.book.not_deleted"),
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
    let text = value[field]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field: {field}"));
    assert!(!text.is_empty(), "empty field: {field}");
    text.into()
}
