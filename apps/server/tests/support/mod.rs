#![allow(dead_code)]

pub mod crypto;
mod identity;
mod library;
pub mod runtime;
mod tag;

pub use identity::*;
pub use library::*;
#[allow(unused_imports)]
pub use tag::*;

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode},
};
use serde::Deserialize;
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
const BOOKS: &str = shiro_http_contract::paths::BOOKS;
const TRASH: &str = shiro_http_contract::paths::TRASH;
const TAGS: &str = shiro_http_contract::paths::TAGS;
const FILES: &str = shiro_http_contract::paths::BOOK_FILES;
const LOGIN: &str = shiro_http_contract::paths::AUTH_LOGIN;
const REFRESH: &str = shiro_http_contract::paths::AUTH_REFRESH;
const LOGOUT: &str = shiro_http_contract::paths::AUTH_LOGOUT;
const ME: &str = shiro_http_contract::paths::AUTH_ME;
const META: &str = shiro_http_contract::paths::META;
const HEALTHZ: &str = shiro_http_contract::paths::HEALTHZ;

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
        runtime::prepare_libs(root).await;
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
        self.request_with_headers(method, uri, body, content_type, authorization, &[])
            .await
    }

    async fn request_with_headers(
        &self,
        method: Method,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
        authorization: Option<&str>,
        extra_headers: &[(&str, &str)],
    ) -> HttpResponse {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(value) = content_type {
            request = request.header("content-type", value);
        }
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        for &(name, value) in extra_headers {
            request = request.header(name, value);
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

    pub async fn healthz(&self) -> HttpResponse {
        self.send(Method::GET, HEALTHZ, None, None).await
    }

    pub async fn meta(&self) -> HttpResponse {
        self.send(Method::GET, META, None, None).await
    }

    pub async fn me(&self, session: &Session) -> HttpResponse {
        self.send(Method::GET, ME, None, Some(session)).await
    }

    pub async fn me_anonymous(&self) -> HttpResponse {
        self.send(Method::GET, ME, None, None).await
    }

    pub async fn send_anonymous(&self, method: Method, uri: &str) -> HttpResponse {
        self.send(method, uri, None, None).await
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

    pub fn json_as<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_value(self.json()).expect("deserialize JSON response")
    }

    pub fn assert_status(&self, status: u16) {
        assert_eq!(
            self.status.as_u16(),
            status,
            "unexpected HTTP status, response body: {:?}",
            String::from_utf8_lossy(&self.bytes)
        );
    }

    pub fn assert_empty(&self, status: u16) {
        self.assert_status(status);
        assert!(self.bytes.is_empty(), "expected an empty response");
    }

    pub fn assert_error(&self, error: HttpError) {
        let (status, code, message) = error.contract();
        self.assert_error_contract(status, code, message);
    }

    pub(super) fn assert_error_contract(&self, status: u16, code: u32, message: &str) {
        self.assert_status(status);
        assert_eq!(
            self.json(),
            json!({"error": {"code": code, "message": message}})
        );
    }

    pub fn assert_png(&self) {
        self.assert_image(image::ImageFormat::Png);
    }

    pub fn assert_rendered_png(&self) {
        self.assert_png();
        let image = image::load_from_memory(&self.bytes).unwrap().to_rgba8();
        assert!(
            image
                .pixels()
                .any(|pixel| pixel.0[0] < 245 || pixel.0[1] < 245 || pixel.0[2] < 245),
            "expected rendered non-blank PNG cover"
        );
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

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    pub fn assert_file(&self, file: &SampleFile) {
        self.assert_status(200);
        let expected_ct = match file {
            SampleFile::Pdf => "application/pdf",
            SampleFile::Epub => "application/epub+zip",
        };
        let bytes = file.bytes();
        let hash = file.hash();
        assert_eq!(self.headers["content-type"], expected_ct);
        assert_eq!(self.headers["accept-ranges"], "bytes");
        assert_eq!(self.headers["etag"], format!("\"{hash}\""));
        assert_eq!(self.headers["cache-control"], "private, immutable");
        assert_eq!(self.headers["x-book-hash"], hash);
        assert_eq!(self.headers["x-book-type"], file.file_type());
        assert_eq!(self.headers["content-length"], bytes.len().to_string());
        assert_eq!(self.bytes.as_ref(), bytes.as_slice());
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HttpError {
    MissingTitle,
    InvalidTitle,
    InvalidChapterTitle,
    UnchangedTitle,
    TitleConflict,
    BookAlreadyDeleted,
    BookNotDeleted,
    InvalidBookId,
    InvalidChapterLocation,
    BookNotFound,
    PositionConflict,
    InvalidHash,
    FileNotFound,
    UploadConflict,
    UploadTimeout,
    SizeMismatch,
    DuplicateFile,
    MissingHash,
    MissingSize,
    DuplicateSize,
    InvalidSize,
    RangeNotSatisfiable,
    InvalidRange,
    MissingFile,
    DuplicateHash,
    HashMismatch,
    UnsupportedFile,
    PayloadTooLarge,
    InvalidCredentials,
    InvalidUsername,
    MissingUsername,
    MissingBearer,
    InvalidToken,
    WrongTokenType,
}

impl HttpError {
    fn contract(self) -> (u16, u32, &'static str) {
        use HttpError::*;
        match self {
            InvalidCredentials => (401, 1001, "identity.authentication.invalid_credentials"),
            MissingBearer => (401, 1002, "identity.authentication.missing_bearer_token"),
            InvalidToken => (401, 1003, "identity.authentication.invalid_token"),
            WrongTokenType => (401, 1005, "identity.authentication.invalid_token_type"),
            MissingUsername => (400, 1102, "identity.user.missing_user_name"),
            InvalidUsername => (400, 1103, "identity.user.invalid_user_name_format"),
            InvalidBookId => (400, 2001, "library.book.invalid_id"),
            MissingTitle => (400, 2002, "library.book.missing_title"),
            InvalidTitle => (400, 2003, "library.book.invalid_title_format"),
            InvalidChapterTitle => (400, 2011, "library.book.invalid_chapter_title"),
            InvalidChapterLocation => (400, 2004, "library.book.invalid_location"),
            PositionConflict => (409, 2005, "library.book.position_conflict"),
            BookNotFound => (404, 2006, "library.book.not_found"),
            TitleConflict => (409, 2007, "library.book.title_conflict"),
            UnchangedTitle => (400, 2008, "library.book.title_unchanged"),
            BookAlreadyDeleted => (409, 2009, "library.book.already_deleted"),
            BookNotDeleted => (409, 2010, "library.book.not_deleted"),
            UploadConflict => (409, 2101, "library.book.file.upload.conflict"),
            DuplicateFile => (409, 2102, "library.book.file.already_exists"),
            InvalidHash => (400, 2103, "library.book.file.invalid_hash_format"),
            HashMismatch => (400, 2104, "library.book.file.hash_mismatch"),
            FileNotFound => (404, 2105, "library.book.file.not_found"),
            UnsupportedFile => (400, 2106, "library.book.file.unsupported"),
            UploadTimeout => (408, 2107, "library.book.file.upload.timeout"),
            SizeMismatch => (400, 2108, "library.book.file.size_mismatch"),
            DuplicateHash => (400, 2110, "library.book.file.upload.duplicate_hash"),
            MissingHash => (400, 2111, "library.book.file.upload.missing_hash"),
            MissingFile => (400, 2112, "library.book.file.upload.missing_file"),
            DuplicateSize => (400, 2113, "library.book.file.upload.duplicate_size"),
            MissingSize => (400, 2114, "library.book.file.upload.missing_size"),
            InvalidSize => (400, 2115, "library.book.file.upload.invalid_size"),
            RangeNotSatisfiable => (416, 2116, "library.book.file.range_not_satisfiable"),
            InvalidRange => (400, 2117, "library.book.file.invalid_range"),
            PayloadTooLarge => (413, 9001, "common.payload_too_large"),
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
