#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc};

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

pub const UNKNOWN_UUID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
pub const UNKNOWN_HASH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
pub const ROOT_FOLDER_ID: &str = "00000000-0000-0000-0000-000000000000";
pub const DEFAULT_BOOK_NAME: &str = "书书";
pub const DEFAULT_BOOKSHELF_NAME: &str = "书shelf";
pub const DEAFULT_FOLDER_NAME: &str = "文件夹";

pub struct TestApp {
    router: Router,
    _temp_dir: TempDir,
}

pub struct Bookshelf<'a> {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub response: JsonHttpResponse,
    app: &'a TestApp,
}

pub struct Folder<'a> {
    pub bookshelf_id: String,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub response: JsonHttpResponse,
    app: &'a TestApp,
}

pub struct FolderTreeNode<'a> {
    pub folder: Folder<'a>,
    pub children: Vec<FolderTreeNode<'a>>,
}

pub struct Book<'a> {
    pub id: String,
    pub bookshelf_id: String,
    pub folder_id: Option<String>,

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

        let mut book_file_storage = BookFileStorage::new(
            books_dir,
            "book".to_owned(),
            Pdfium::new(Pdfium::bind_to_system_library().unwrap()),
        );
        book_file_storage.scan().expect("scan test storage");

        let bookfile_service = Arc::new(BookFileService::new(book_file_storage));
        let bookshelf_repository: Arc<dyn BookshelfRepository> =
            Arc::new(SqliteBookshelfRepository::new(pool.clone()));
        let folder_repository: Arc<dyn FolderRepository> =
            Arc::new(SqliteFolderRepository::new(pool.clone()));
        let book_repository: Arc<dyn BookRepository> =
            Arc::new(SqliteBookRepository::new(pool.clone()));

        let state = AppState {
            book_service: Arc::new(BookService::new(book_repository, bookfile_service.clone())),
            bookshelf_service: Arc::new(BookshelfService::new(bookshelf_repository)),
            folder_service: Arc::new(FolderService::new(folder_repository)),
            bookfile_service: bookfile_service,
        };

        Self {
            router: build_router(state),
            _temp_dir: temp_dir,
        }
    }

    pub async fn create_bookshelf<'a>(&'a self, name: &str) -> Bookshelf<'a> {
        let response = JsonHttpResponse::from(
            self.post_json("/api/v1/library/bookshelves", json!({ "name": name }))
                .await,
        )
        .await;

        let created_at = response.json["data"]["created_at"]
            .as_str()
            .unwrap_or("")
            .to_owned();

        Bookshelf {
            id: response.json["data"]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            updated_at: response.json["data"]["updated_at"]
                .as_str()
                .unwrap_or(&created_at)
                .to_owned(),
            name: name.to_owned(),
            created_at: created_at,
            response: response,
            app: self,
        }
    }

    pub async fn create_default_bookshelf<'a>(&'a self) -> Bookshelf<'a> {
        self.create_bookshelf(DEFAULT_BOOKSHELF_NAME).await
    }

    pub async fn get_book_file(&self, hash: &str) -> Response<Body> {
        self.get(&format!("/api/v1/library/books/files?hash={hash}"))
            .await
    }

    pub async fn get_book<'a>(&'a self, book_id: &str) -> Book<'a> {
        let response = JsonHttpResponse::from(
            self.get(&format!("/api/v1/library/books?id={book_id}"))
                .await,
        )
        .await;
        Book::from(self, &response.json["data"].to_owned(), response)
    }

    pub async fn get_bookshelf<'a>(&'a self, id: &str) -> Bookshelf<'a> {
        let response = JsonHttpResponse::from(
            self.get(&format!("/api/v1/library/bookshelves?id={id}"))
                .await,
        )
        .await;

        Bookshelf {
            id: response.json["data"]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            name: response.json["data"]["name"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            created_at: response.json["data"]["created_at"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            updated_at: response.json["data"]["updated_at"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            response: response,
            app: self,
        }
    }
    pub async fn get_bookshelves<'a>(&'a self) -> Vec<Bookshelf<'a>> {
        let response =
            JsonHttpResponse::from(self.get(&format!("/api/v1/library/bookshelves")).await).await;

        let mut result: Vec<Bookshelf> = vec![];
        for node in response.json["data"].as_array().unwrap() {
            result.push(Bookshelf {
                id: node["id"].as_str().unwrap_or("").to_owned(),
                name: node["name"].as_str().unwrap_or("").to_owned(),
                created_at: node["created_at"].as_str().unwrap_or("").to_owned(),
                updated_at: node["updated_at"].as_str().unwrap_or("").to_owned(),
                response: response.clone(), // 我知道没必要，但我懒..
                app: self,
            });
        }
        result
    }

    async fn create_folder<'a>(
        &'a self,
        bookshelf_id: &str,
        parent_id: &str,
        name: &str,
    ) -> Folder<'a> {
        let response = JsonHttpResponse::from(
            self.post_json(
                &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
                json!({ "parent_id": parent_id, "name": name }),
            )
            .await,
        )
        .await;

        let create_at = response.json["data"]["created_at"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        Folder {
            id: response.json["data"]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            created_at: create_at.to_owned(),
            updated_at: create_at,
            bookshelf_id: bookshelf_id.to_owned(),
            response: response,
            name: name.to_owned(),
            app: self,
        }
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

    async fn create_book<'a>(
        &'a self,
        bookshelf_id: &str,
        folder_id: Option<&str>,
        title: &str,
        hash: &str,
    ) -> Book<'a> {
        let response = JsonHttpResponse::from(
            self.post_json(
                "/api/v1/library/books",
                json!({
                "title": title,
                "hash": hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": folder_id
                }),
            )
            .await,
        )
        .await;
        let create_at = response.json["data"]["created_at"]
            .as_str()
            .unwrap_or("")
            .to_owned();

        Book {
            id: response.json["data"]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            bookshelf_id: bookshelf_id.to_owned(),
            folder_id: folder_id.and_then(|f| Some(f.to_owned())),
            title: title.to_owned(),
            file_type: EMPTY_STRING.to_owned(),
            hash: hash.to_owned(),
            created_at: create_at.to_owned(),
            updated_at: create_at,
            response: response,
            app: self,
        }
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

impl<'a> Bookshelf<'a> {
    pub async fn create_book(&self, title: &str, hash: &str) -> Book<'a> {
        self.app.create_book(&self.id, None, title, hash).await
    }

    pub async fn create_folder(&self, name: &str) -> Folder<'a> {
        self.app.create_folder(&self.id, ROOT_FOLDER_ID, name).await
    }

    pub async fn create_default_book(&self) -> Book<'a> {
        let hash = self.app.upload_sample_book_file(SampleFile::PDF).await.1;
        self.create_book(DEFAULT_BOOK_NAME, &hash.to_hex()).await
    }

    pub async fn create_default_folder(&self) -> Folder<'a> {
        self.create_folder(DEAFULT_FOLDER_NAME).await
    }

    pub async fn delete(self) -> JsonHttpResponse {
        JsonHttpResponse::from(
            self.app
                .delete(&format!("/api/v1/library/bookshelves/{}", &self.id))
                .await,
        )
        .await
    }

    pub async fn rename(&mut self, new_name: &str) {
        let response = JsonHttpResponse::from(
            self.app
                .patch_json(
                    &format!("/api/v1/library/bookshelves/{}", &self.id),
                    json!({ "name": new_name }),
                )
                .await,
        )
        .await;
        self.name = new_name.to_owned();
        self.response = response;
    }

    pub async fn get_folder(&self, folder_id: &str) -> Folder<'a> {
        let response = JsonHttpResponse::from(
            self.app
                .get(&format!(
                    "/api/v1/library/bookshelves/{}/folders?id={}",
                    self.id, folder_id
                ))
                .await,
        )
        .await;

        Folder {
            id: response.json["data"][0]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            name: response.json["data"][0]["name"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            created_at: response.json["data"][0]["created_at"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            updated_at: response.json["data"][0]["updated_at"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            bookshelf_id: self.id.to_owned(),
            response: response,
            app: self.app,
        }
    }

    pub async fn get_books(&self) -> Vec<Book<'a>> {
        let response = JsonHttpResponse::from(
            self.app
                .get(&format!("/api/v1/library/bookshelves/{}/books", self.id))
                .await,
        )
        .await;
        let mut books: Vec<Book> = vec![];
        for json in response.json["data"].as_array().unwrap() {
            books.push(Book::from(self.app, json, response.to_owned()));
        }
        books
    }

    pub async fn get_folders(&self) -> Vec<FolderTreeNode<'a>> {
        let response = JsonHttpResponse::from(
            self.app
                .get(&format!(
                    "/api/v1/library/bookshelves/{}/folders?recursive=true",
                    self.id
                ))
                .await,
        )
        .await;
        self.make_tree(&response.json["data"], &response)
    }

    fn make_tree(&self, json: &Value, response: &JsonHttpResponse) -> Vec<FolderTreeNode<'a>> {
        let mut tree: Vec<FolderTreeNode<'a>> = vec![];
        for n in json.as_array().unwrap() {
            let children = self.make_tree(&n["children"], &response);
            tree.push(FolderTreeNode {
                folder: Folder {
                    bookshelf_id: self.id.to_owned(),
                    id: n["id"].as_str().unwrap_or("").to_owned(),
                    name: n["name"].as_str().unwrap_or("").to_owned(),
                    created_at: n["created_at"].as_str().unwrap_or("").to_owned(),
                    updated_at: n["updated_at"].as_str().unwrap_or("").to_owned(),
                    response: response.to_owned(),
                    app: self.app,
                },
                children: children,
            });
        }
        tree
    }

    pub async fn create_folder_tree(&self) {
        for i in 0..2 {
            let folder_first = self.create_folder(&format!("{}00", i)).await;
            for j in 0..3 {
                let folder_second = folder_first.create_folder(&format!("{}{}0", i, j)).await;
                for n in 0..3 {
                    folder_second
                        .create_folder(&format!("{}{}{}", i, j, n))
                        .await;
                }
            }
        }
    }

    pub fn assert_data(&self) {
        Uuid::parse_str(&self.id).expect("expected valid uuid");
        DateTime::parse_from_rfc3339(&self.created_at).expect("expected valid RFC3339 datetime");
        DateTime::parse_from_rfc3339(&self.updated_at).expect("expected valid RFC3339 datetime");
    }
}

impl<'a> Folder<'a> {
    pub async fn create_book(&self, title: &str, hash: &str) -> Book<'a> {
        self.app
            .create_book(&self.bookshelf_id, Some(&self.id), title, hash)
            .await
    }

    pub async fn create_folder(&self, name: &str) -> Folder<'a> {
        self.app
            .create_folder(&self.bookshelf_id, &self.id, name)
            .await
    }

    pub async fn create_default_book(&self) -> Book<'a> {
        let hash = self.app.upload_sample_book_file(SampleFile::PDF).await.1;
        self.create_book(DEFAULT_BOOK_NAME, &hash.to_hex()).await
    }

    pub async fn create_default_folder(&self) -> Folder<'a> {
        self.create_folder(DEAFULT_FOLDER_NAME).await
    }

    pub async fn delete(self) -> JsonHttpResponse {
        JsonHttpResponse::from(
            self.app
                .delete(&format!(
                    "/api/v1/library/bookshelves/{}/folders/{}",
                    &self.bookshelf_id, &self.id
                ))
                .await,
        )
        .await
    }

    pub async fn rename(&mut self, new_name: &str) {
        let response = JsonHttpResponse::from(
            self.app
                .patch_json(
                    &format!(
                        "/api/v1/library/bookshelves/{}/folders/{}",
                        &self.bookshelf_id, &self.id
                    ),
                    json!({ "name": new_name , "parent_id": null}),
                )
                .await,
        )
        .await;
        self.name = new_name.to_owned();
        self.response = response;
    }

    pub async fn move_to(&mut self, parent_id: &str) {
        let response = JsonHttpResponse::from(
            self.app
                .patch_json(
                    &format!(
                        "/api/v1/library/bookshelves/{}/folders/{}",
                        &self.bookshelf_id, &self.id
                    ),
                    json!({ "name": null , "parent_id": parent_id}),
                )
                .await,
        )
        .await;
        self.response = response;
    }

    pub async fn get_books(&self) -> Vec<Book<'a>> {
        let response = JsonHttpResponse::from(
            self.app
                .get(&format!(
                    "/api/v1/library/bookshelves/{}/folders/{}/books",
                    self.bookshelf_id, self.id
                ))
                .await,
        )
        .await;
        let mut books: Vec<Book> = vec![];
        for json in response.json["data"].as_array().unwrap() {
            books.push(Book::from(self.app, json, response.to_owned()));
        }
        books
    }

    pub fn assert_data(&self) {
        Uuid::parse_str(&self.id).expect("expected valid uuid");
        DateTime::parse_from_rfc3339(&self.created_at).expect("expected valid RFC3339 datetime");
        DateTime::parse_from_rfc3339(&self.updated_at).expect("expected valid RFC3339 datetime");
    }
}

impl<'a> Book<'a> {
    pub fn from(app: &'a TestApp, json: &Value, response: JsonHttpResponse) -> Self {
        Self {
            id: json["id"].as_str().unwrap_or("").to_owned(),
            bookshelf_id: json["bookshelf_id"].as_str().unwrap_or("").to_owned(),
            folder_id: json["folder_id"].as_str().and_then(|f| Some(f.to_owned())),
            title: json["title"].as_str().unwrap_or("").to_owned(),
            file_type: json["type"].as_str().unwrap_or("").to_owned(),
            hash: json["hash"].as_str().unwrap_or("").to_owned(),
            created_at: json["created_at"].as_str().unwrap_or("").to_owned(),
            updated_at: json["updated_at"].as_str().unwrap_or("").to_owned(),
            response: response,
            app: app,
        }
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

    pub fn assert_bookshelf_missing_name_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.missing_name"
        );
    }

    pub fn assert_bookshelf_invalid_name_format_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.invalid_name_format"
        );
    }

    pub fn assert_bookshelf_invalid_id_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.invalid_id"
        );
    }

    pub fn assert_bookshelf_not_found_error(&self) {
        assert_eq!(self.status_code, StatusCode::NOT_FOUND);
        assert_eq!(self.json["error"]["message"], "library.bookshelf.not_found");
    }

    pub fn assert_bookshelf_name_conflict_error(&self) {
        assert_eq!(self.status_code, StatusCode::CONFLICT);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.name_conflict"
        );
    }

    pub fn assert_folder_missing_name_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.missing_name"
        );
    }

    pub fn assert_folder_name_conflict_error(&self) {
        assert_eq!(self.status_code, StatusCode::CONFLICT);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.name_conflict"
        );
    }

    pub fn assert_folder_cycled_error(&self) {
        assert_eq!(self.status_code, StatusCode::CONFLICT);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.cycled"
        );
    }

    pub fn assert_folder_not_found_error(&self) {
        assert_eq!(self.status_code, StatusCode::NOT_FOUND);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.not_found"
        );
    }

    pub fn assert_folder_invalid_id_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.invalid_id"
        );
    }

    pub fn assert_folder_parent_not_found_error(&self) {
        assert_eq!(self.status_code, StatusCode::NOT_FOUND);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.parent_not_found"
        );
    }

    pub fn assert_folder_invalid_name_format_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.invalid_name_format"
        );
    }

    pub fn assert_folder_invalid_parent_id_error(&self) {
        assert_eq!(self.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(
            self.json["error"]["message"],
            "library.bookshelf.folder.invalid_parent_id"
        );
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
