#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    http::{Method, Request, Response, StatusCode, header::CONTENT_TYPE},
};
use blake3::Hash;
use chrono::DateTime;
use derive_new::new;
use pdfium_render::prelude::Pdfium;
use serde_json::{Value, json};
use shiro_reader_server::{
    app::build_router,
    application::library::book::{
        file::service::BookFileService, ports::BookRepository, service::BookService,
    },
    infrastructure::{
        db,
        repositories::sqlite_book_repository::SqliteBookRepository,
        storage::book_file_storage::BookFileStorage,
    },
    state::AppState,
};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

pub const EMPTY_STRING: &str = "";

pub const INVALID_BOOK_HASH: &str = "not-a-hash";
pub const INVALID_UUID: &str = "not-a-uuid";
pub const INVALID_HASH: &str = "not-a-hash";

pub const INVALID_BOOK_TITLES: [&str; 6] =
    [" Book", "Book ", "\tBook", "Book\t", "\nBook", "Book\n"];

pub const UNKNOWN_UUID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
pub const UNKNOWN_HASH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
pub const DEFAULT_BOOK_NAME: &str = "书书";

static PDFIUM: LazyLock<Arc<Mutex<Pdfium>>> = LazyLock::new(|| {
    Arc::new(Mutex::new(Pdfium::new(
        Pdfium::bind_to_system_library().expect("Failed to bind Pdfium"),
    )))
});

pub struct TestApp {
    router: Router,
    _temp_dir: TempDir,
}

pub struct Book<'a> {
    pub id: String,
    pub title: String,
    pub file_type: String,
    pub hash: String,
    pub created_at: String,
    pub updated_at: String,
    pub response: JsonHttpResponse,
    app: &'a TestApp,
}

#[derive(new, Debug, Clone)]
pub struct JsonHttpResponse {
    pub status_code: StatusCode,
    pub json: Value,
}

pub enum SampleFile {
    PDF,
    EPUB,
}

impl TestApp {
    pub async fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp test directory");
        let db_path = temp_dir.path().join("test.db");
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            db_path.display().to_string().replace('\\', "/")
        );
        let pool = db::pool::connect_sqlite_url(&db_url, 1)
            .await
            .expect("connect test database");
        db::migrate(&pool).await.expect("run migrations");

        let books_dir = temp_dir.path().join("books");

        let mut book_file_storage =
            BookFileStorage::new(books_dir, "book".to_owned(), PDFIUM.clone());
        book_file_storage.scan().expect("scan test storage");

        let bookfile_service = Arc::new(BookFileService::new(book_file_storage));
        let book_repository: Arc<dyn BookRepository> =
            Arc::new(SqliteBookRepository::new(pool.clone()));

        let state = AppState {
            book_service: Arc::new(BookService::new(book_repository, bookfile_service.clone())),
            bookfile_service: bookfile_service,
        };

        Self {
            router: build_router(state),
            _temp_dir: temp_dir,
        }
    }

    pub async fn get_book_file(&self, hash: &str) -> Response<Body> {
        self.get(&format!("/api/v1/library/books/files?hash={hash}"))
            .await
    }

    pub async fn get_books<'a>(&'a self) -> Vec<Book<'a>> {
        let response =
            JsonHttpResponse::from(self.get("/api/v1/library/bookshelves/books").await).await;
        assert_eq!(response.status_code, StatusCode::OK);
        Book::from_data_array(self, response)
    }

    pub async fn get_books_by_ids(&self, ids: &[&str]) -> JsonHttpResponse {
        let query = ids
            .iter()
            .map(|id| format!("id={id}"))
            .collect::<Vec<_>>()
            .join("&");
        JsonHttpResponse::from(
            self.get(&format!("/api/v1/library/bookshelves/books?{query}"))
                .await,
        )
        .await
    }

    pub async fn upload_book_file(&self, bytes: &[u8], hash: &str) -> JsonHttpResponse {
        let boundary = "shiro-reader-test-boundary";
        let mut body = Vec::new();

        if hash.len() > 0 {
            push_text_part(&mut body, boundary, "hash", hash);
        }
        push_file_part(&mut body, boundary, "file", "book.book", bytes);
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let response = self
            .request(
                Method::POST,
                "/api/v1/library/books/files",
                Body::from(body),
                Some(format!("multipart/form-data; boundary={boundary}")),
            )
            .await;
        JsonHttpResponse::from(response).await
    }

    pub async fn upload_sample_book_file(
        &self,
        sample_file: SampleFile,
    ) -> (JsonHttpResponse, Hash) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("samples")
            .join(match sample_file {
                SampleFile::PDF => "sample_document.pdf",
                SampleFile::EPUB => "sample_document.epub",
            });

        let bytes = std::fs::read(path).unwrap();
        let hash = blake3::hash(&bytes);
        let response = self.upload_book_file(&bytes, &hash.to_hex()).await;
        (response, hash)
    }
    pub async fn download_book_file(&self, hash: &str) -> Response<Body> {
        self.get(&format!("/api/v1/library/books/files/{hash}"))
            .await
    }

    pub async fn create_book<'a>(&'a self, title: &str, hash: &str) -> Book<'a> {
        let response = JsonHttpResponse::from(
            self.post_json(
                "/api/v1/library/bookshelves/books",
                json!({
                "title": title,
                "hash": hash,
                }),
            )
            .await,
        )
        .await;
        let data = &response.json;
        let created_at = json_string_or_empty(data, "created_at");

        Book {
            id: json_string_or_empty(data, "id"),
            title: title.to_owned(),
            file_type: EMPTY_STRING.to_owned(),
            hash: hash.to_owned(),
            created_at: created_at.to_owned(),
            updated_at: created_at,
            response: response,
            app: self,
        }
    }

    pub async fn create_default_book(&self) -> Book<'_> {
        let hash = self.upload_sample_book_file(SampleFile::PDF).await.1;
        self.create_book(DEFAULT_BOOK_NAME, &hash.to_hex()).await
    }

    async fn get(&self, uri: &str) -> Response<Body> {
        self.request(Method::GET, uri, Body::empty(), None).await
    }

    async fn delete(&self, uri: &str) -> Response<Body> {
        self.request(Method::DELETE, uri, Body::empty(), None).await
    }

    async fn post_json(&self, uri: &str, body: Value) -> Response<Body> {
        self.json_request(Method::POST, uri, body).await
    }

    async fn patch_json(&self, uri: &str, body: Value) -> Response<Body> {
        self.json_request(Method::PATCH, uri, body).await
    }

    async fn json_request(&self, method: Method, uri: &str, body: Value) -> Response<Body> {
        self.request(
            method,
            uri,
            Body::from(serde_json::to_vec(&body).expect("serialize request body")),
            Some("application/json".to_owned()),
        )
        .await
    }

    async fn request(
        &self,
        method: Method,
        uri: &str,
        body: Body,
        content_type: Option<String>,
    ) -> Response<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(content_type) = content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }

        self.router
            .clone()
            .oneshot(builder.body(body).expect("build request"))
            .await
            .expect("route request")
    }
}

impl<'a> Book<'a> {
    fn from_response(app: &'a TestApp, response: JsonHttpResponse) -> Self {
        let fields = Self::fields(&response.json);
        Self::from_fields(app, response, fields)
    }

    pub fn from_data_array(app: &'a TestApp, response: JsonHttpResponse) -> Vec<Self> {
        response
            .as_array()
            .iter()
            .map(|json| Self::from(app, json, response.to_owned()))
            .collect()
    }

    pub fn from(app: &'a TestApp, json: &Value, response: JsonHttpResponse) -> Self {
        let fields = Self::fields(json);
        Self::from_fields(app, response, fields)
    }

    fn from_fields(
        app: &'a TestApp,
        response: JsonHttpResponse,
        fields: (String, String, String, String, String, String),
    ) -> Self {
        let (id, title, file_type, hash, created_at, updated_at) = fields;

        Self {
            id,
            title,
            file_type,
            hash,
            created_at,
            updated_at,
            response,
            app,
        }
    }

    fn fields(
        json: &Value,
    ) -> (String, String, String, String, String, String) {
        (
            json_string_or_empty(json, "id"),
            json_string_or_empty(json, "title"),
            json_string_or_empty(json, "type"),
            json_string_or_empty(json, "hash"),
            json_string_or_empty(json, "created_at"),
            json_string_or_empty(json, "updated_at"),
        )
    }
    pub async fn delete(self) -> JsonHttpResponse {
        JsonHttpResponse::from(
            self.app
                .delete(&format!("/api/v1/library/books/{}", self.id))
                .await,
        )
        .await
    }
    pub async fn rename(&mut self, new_title: &str) {
        let response = JsonHttpResponse::from(
            self.app
                .patch_json(
                    &format!("/api/v1/library/books/{}", self.id),
                    json!({ "title": new_title }),
                )
                .await,
        )
        .await;
        self.title = new_title.to_owned();
        self.response = response;
    }
    pub async fn get_cover(&self) -> Response<Body> {
        self.app
            .get(&format!("/api/v1/library/books/{}/cover", self.id))
            .await
    }
    pub fn assert_data(&self) {
        Uuid::parse_str(&self.id).expect("expected valid uuid");
        DateTime::parse_from_rfc3339(&self.created_at).expect("expected valid RFC3339 datetime");
        DateTime::parse_from_rfc3339(&self.updated_at).expect("expected valid RFC3339 datetime");
    }
}

impl JsonHttpResponse {
    pub async fn from(value: Response<Body>) -> Self {
        JsonHttpResponse::new(value.status(), json_body(value).await)
    }

    pub fn as_array(&self) -> &[Value] {
        self.json.as_array().expect("expected response data array")
    }

    pub fn assert_book_file_upload_missing_hash_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.upload.missing_hash"
        );
    }

    pub fn assert_book_file_upload_missing_file_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.upload.missing_file"
        );
    }

    pub fn assert_book_file_upload_duplicate_hash_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.upload.duplicate_hash"
        );
    }

    pub fn assert_book_file_hash_mismatch_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.hash_mismatch"
        );
    }

    pub fn assert_book_file_already_exists_error(&self) {
        assert_eq!(self.status_code, StatusCode::CONFLICT);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.already_exists"
        );
    }

    pub fn assert_book_file_invalid_hash_format_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.file.invalid_hash_format"
        );
    }

    pub fn assert_book_file_not_found_error(&self) {
        assert_eq!(self.status_code, StatusCode::NOT_FOUND);
        assert_eq!(self.json["error"]["message"], "library.book.file.not_found");
    }

    pub fn assert_book_missing_title_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(self.json["error"]["message"], "library.book.missing_title");
    }

    pub fn assert_book_invalid_title_format_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.book.invalid_title_format"
        );
    }

    pub fn assert_book_invalid_id_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(self.json["error"]["message"], "library.book.invalid_id");
    }

    pub fn assert_book_not_found_error(&self) {
        assert_eq!(self.status_code, StatusCode::NOT_FOUND);
        assert_eq!(self.json["error"]["message"], "library.book.not_found");
    }

    pub fn assert_book_title_conflict_error(&self) {
        assert_eq!(self.status_code, StatusCode::CONFLICT);
        assert_eq!(self.json["error"]["message"], "library.book.title_conflict");
    }
}

async fn json_body(response: Response<Body>) -> Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

pub async fn body_bytes(response: Response<Body>) -> Bytes {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body")
}

fn assert_uuid_string(value: &Value) {
    let raw = value.as_str().expect("expected uuid string");
    Uuid::parse_str(raw).expect("expected valid uuid");
}

fn assert_rfc3339_datetime_string(value: &Value) {
    let raw = value.as_str().expect("expected datetime string");
    DateTime::parse_from_rfc3339(raw).expect("expected valid RFC3339 datetime");
}

fn json_string_or_empty(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").to_owned()
}

fn json_optional_string(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .expect("expected string or null response field")
                .to_owned(),
        ),
    }
}

fn push_text_part(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

fn push_file_part(body: &mut Vec<u8>, boundary: &str, name: &str, filename: &str, bytes: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}
