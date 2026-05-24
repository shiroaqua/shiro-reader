#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    http::{Method, Request, Response, StatusCode, header::CONTENT_TYPE},
};
use chrono::DateTime;
use serde_json::{Value, json};
use shiro_reader_server::{
    app::build_router,
    application::library::{
        book::{file::service::BookFileService, ports::BookRepository, service::BookService},
        bookshelf::{
            folder::{ports::FolderRepository, service::FolderService},
            ports::BookshelfRepository,
            service::BookshelfService,
        },
    },
    infrastructure::{
        db,
        repositories::{
            sqlite_book_repository::SqliteBookRepository,
            sqlite_bookshelf_repository::SqliteBookshelfRepository,
            sqlite_folder_repository::SqliteFolderRepository,
        },
        storage::hash_file_storage::HashFileStorage,
    },
    state::AppState,
};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;


pub const SAMPLE_BOOK_BYTES: &[u8] = b"sample-book-file";
pub const ANOTHER_SAMPLE_BOOK_BYTES: &[u8] = b"another-sample-book-file";
pub const THIRD_SAMPLE_BOOK_BYTES: &[u8] = b"third-sample-book-file";
pub const MISSING_SAMPLE_BOOK_BYTES: &[u8] = b"missing-sample-book-file";
pub const HASH_MISMATCH_BOOK_BYTES: &[u8] = b"hash-mismatch-book-file";
pub const INVALID_BOOK_HASH: &str = "not-a-hash";
pub const INVALID_UUID: &str = "not-a-uuid";
pub const EMPTY_STRING: &str = "";

pub const SAMPLE_BOOK_TITLE: &str = "一本书";
pub const RENAMED_BOOK_TITLE: &str = "一本同样的书";

pub const SAMPLE_BOOKSHELF_NAME: &str = "Bookshelf";
pub const ANOTHER_SAMPLE_BOOKSHELF_NAME: &str = "书架";
pub const RENAMED_BOOKSHELF_NAME: &str = "书";


pub const SAMPLE_ROOT_FOLDER_NAME: &str = "根目录";
pub const SAMPLE_CHILD_FOLDER_NAME: &str = "子文件夹";
pub const RENAMED_FOLDER_NAME: &str = "文件夹";
pub const OTHER_ROOT_FOLDER_NAME: &str = "另一只根文件夹";

pub const INVALID_BOOK_TITLES: [&str; 6] = [
    " Book",
    "Book ",
    "\tBook",
    "Book\t",
    "\nBook",
    "Book\n",
];

pub const INVALID_BOOKSHELF_NAMES: [&str; 8] = [
    "一块 Shelf",
    " 一块Shelf",
    "Shelf块 ",
    "Shelf-块",
    "Shelf_块",
    "Shelf.qwq",
    "Shelf/nya",
    "书架\n架",
];


pub const INVALID_FOLDER_NAMES: [&str; 8] = [
    "一只 文件夹",
    " Folder",
    "Folder ",
    "Folder-xxx",
    "Folder_不行",
    "Folder.不行不行不行",
    "Folder/不",
    "Folder\nsubFolder",
];


pub const UNKNOWN_UUID: &str = "00000000-0000-0000-0000-000000000000";


pub struct TestApp {
    router: Router,
    _temp_dir: TempDir,
}

impl TestApp {
    pub async fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp test directory");
        let db_path = temp_dir.path().join("test.db");
        let db_url = sqlite_url(&db_path);

        let pool = db::pool::connect_sqlite_url(&db_url, 1)
            .await
            .expect("connect test database");
        db::migrate(&pool).await.expect("run migrations");

        let books_dir = temp_dir.path().join("books");
        std::fs::create_dir_all(books_dir.join(".temp")).expect("create test storage directory");

        let mut book_file_storage = HashFileStorage::new(books_dir, "book".to_owned());
        book_file_storage.scan().expect("scan test storage");

        let bookfile_service = Arc::new(BookFileService::new(book_file_storage));
        let bookshelf_repository: Arc<dyn BookshelfRepository> =
            Arc::new(SqliteBookshelfRepository::new(pool.clone()));
        let folder_repository: Arc<dyn FolderRepository> =
            Arc::new(SqliteFolderRepository::new(pool.clone()));
        let book_repository: Arc<dyn BookRepository> =
            Arc::new(SqliteBookRepository::new(pool.clone()));


        let state = AppState {
            book_service: Arc::new(BookService::new(book_repository,bookfile_service.clone())),
            bookshelf_service: Arc::new(BookshelfService::new(bookshelf_repository)),
            folder_service: Arc::new(FolderService::new(folder_repository)),
            bookfile_service: bookfile_service,
        };

        Self {
            router: build_router(state),
            _temp_dir: temp_dir,
        }
    }

    pub async fn get(&self, uri: &str) -> Response<Body> {
        self.request(Method::GET, uri, Body::empty(), None).await
    }

    pub async fn delete(&self, uri: &str) -> Response<Body> {
        self.request(Method::DELETE, uri, Body::empty(), None).await
    }

    pub async fn post_json(&self, uri: &str, body: Value) -> Response<Body> {
        self.json_request(Method::POST, uri, body).await
    }

    pub async fn patch_json(&self, uri: &str, body: Value) -> Response<Body> {
        self.json_request(Method::PATCH, uri, body).await
    }

    pub async fn upload_book_file(
        &self,
        hash: &str,
        bytes: &[u8],
        extra_hash: Option<&str>,
    ) -> Response<Body> {
        let boundary = "shiro-reader-test-boundary";
        let mut body = Vec::new();

        push_text_part(&mut body, boundary, "hash", hash);
        if let Some(extra_hash) = extra_hash {
            push_text_part(&mut body, boundary, "hash", extra_hash);
        }
        push_file_part(&mut body, boundary, "file", "book.book", bytes);
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        self.request(
            Method::POST,
            "/api/v1/library/books/files",
            Body::from(body),
            Some(format!("multipart/form-data; boundary={boundary}")),
        )
        .await
    }

    pub async fn upload_book_file_without_hash(&self, bytes: &[u8]) -> Response<Body> {
        let boundary = "shiro-reader-test-boundary";
        let mut body = Vec::new();
        push_file_part(&mut body, boundary, "file", "book.book", bytes);
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        self.request(
            Method::POST,
            "/api/v1/library/books/files",
            Body::from(body),
            Some(format!("multipart/form-data; boundary={boundary}")),
        )
        .await
    }

    pub async fn upload_book_file_without_file(&self, hash: &str) -> Response<Body> {
        let boundary = "shiro-reader-test-boundary";
        let mut body = Vec::new();
        push_text_part(&mut body, boundary, "hash", hash);
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        self.request(
            Method::POST,
            "/api/v1/library/books/files",
            Body::from(body),
            Some(format!("multipart/form-data; boundary={boundary}")),
        )
        .await
    }

    pub async fn upload_book_bytes(&self, bytes: &[u8]) -> String {
        let hash = hash_for(bytes);
        let response = self.upload_book_file(&hash, bytes, None).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        hash
    }

    pub async fn upload_sample_book_file(&self) -> String {
        self.upload_book_bytes(SAMPLE_BOOK_BYTES).await
    }

    pub async fn upload_another_sample_book_file(&self) -> String {
        self.upload_book_bytes(ANOTHER_SAMPLE_BOOK_BYTES).await
    }

    pub async fn upload_third_sample_book_file(&self) -> String {
        self.upload_book_bytes(THIRD_SAMPLE_BOOK_BYTES).await
    }

    pub async fn create_bookshelf(&self, name: &str) -> String {
        let response = self
            .post_json("/api/v1/library/bookshelves", json!({ "name": name }))
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        json_body(response).await["data"]["id"]
            .as_str()
            .expect("bookshelf id")
            .to_owned()
    }

    pub async fn create_sample_bookshelf(&self) -> String {
        self.create_bookshelf(SAMPLE_BOOKSHELF_NAME).await
    }

    pub async fn create_another_sample_bookshelf(&self) -> String {
        self.create_bookshelf(ANOTHER_SAMPLE_BOOKSHELF_NAME).await
    }

    pub async fn create_folder(
        &self,
        bookshelf_id: &str,
        parent_id: Option<&str>,
        name: &str,
    ) -> String {
        let response = self
            .post_json(
                &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
                json!({ "parent_id": parent_id, "name": name }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        json_body(response).await["data"]["id"]
            .as_str()
            .expect("folder id")
            .to_owned()
    }

    pub async fn create_sample_root_folder(&self, bookshelf_id: &str) -> String {
        self.create_folder(bookshelf_id, None, SAMPLE_ROOT_FOLDER_NAME)
            .await
    }

    pub async fn create_sample_child_folder(&self, bookshelf_id: &str, parent_id: &str) -> String {
        self.create_folder(bookshelf_id, Some(parent_id), SAMPLE_CHILD_FOLDER_NAME)
            .await
    }

    pub async fn create_other_sample_root_folder(&self, bookshelf_id: &str) -> String {
        self.create_folder(bookshelf_id, None, OTHER_ROOT_FOLDER_NAME)
            .await
    }

    pub async fn create_book(
        &self,
        bookshelf_id: &str,
        folder_id: Option<&str>,
        title: &str,
        hash: &str,
    ) -> String {
        let response = self
            .post_json(
                "/api/v1/library/books",
                json!({
                    "title": title,
                    "hash": hash,
                    "bookshelf_id": bookshelf_id,
                    "folder_id": folder_id,
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        json_body(response).await["data"]["id"]
            .as_str()
            .expect("book id")
            .to_owned()
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

pub async fn body_bytes(response: Response<Body>) -> Bytes {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body")
}

pub async fn json_body(response: Response<Body>) -> Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).expect("parse json response body")
}

pub async fn assert_error(response: Response<Body>, status: StatusCode, message: &str) {
    assert_eq!(response.status(), status);
    let body = json_body(response).await;
    assert_eq!(body["error"]["message"], message);
}

pub fn assert_uuid_string(value: &Value) {
    let raw = value.as_str().expect("expected uuid string");
    Uuid::parse_str(raw).expect("expected valid uuid");
}

pub fn assert_rfc3339_datetime_string(value: &Value) {
    let raw = value.as_str().expect("expected datetime string");
    DateTime::parse_from_rfc3339(raw).expect("expected valid RFC3339 datetime");
}

pub fn hash_for(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn sqlite_url(path: &PathBuf) -> String {
    let path = path.display().to_string().replace('\\', "/");
    format!("sqlite://{path}?mode=rwc")
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
