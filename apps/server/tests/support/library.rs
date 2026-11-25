use super::*;
use jiff::Timestamp;

#[derive(Clone, Copy)]
pub enum SampleFile {
    Pdf,
    Epub,
}

impl SampleFile {
    pub fn bytes(&self) -> Vec<u8> {
        let name = match self {
            Self::Pdf => "sample_document.pdf",
            Self::Epub => "sample_document.epub",
        };
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/samples")
                .join(name),
        )
        .unwrap()
    }
    pub fn hash(&self) -> String {
        blake3::hash(&self.bytes()).to_hex().to_string()
    }
    pub(super) fn file_type(&self) -> &str {
        match self {
            Self::Pdf => "PDF",
            Self::Epub => "EPUB",
        }
    }
}

pub fn epub_with_cover(cover: Option<&[u8]>) -> Vec<u8> {
    use std::io::{Cursor, Write};

    let mut source = zip::ZipArchive::new(Cursor::new(SampleFile::Epub.bytes())).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().expect("valid EPUB entry name").into_owned();
        if name == "EPUB/cover.svg" {
            if let Some(cover) = cover {
                output.start_file(&name, options).unwrap();
                output.write_all(cover).unwrap();
            }
        } else {
            output.start_file(&name, options).unwrap();
            std::io::copy(&mut entry, &mut output).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BookData {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrashedBookData {
    #[serde(flatten)]
    pub book: BookData,
    pub deleted_at: Timestamp,
}

pub struct Book<'a> {
    pub data: BookData,
    app: &'a TestApp,
    session: Session,
}

impl TestApp {
    pub async fn trash(&self, session: &Session) -> Vec<TrashedBookData> {
        let response = self.send(Method::GET, TRASH, None, Some(session)).await;
        response.assert_status(200);
        serde_json::from_value(response.json()).expect("trash list fields")
    }

    pub async fn restore_book(&self, session: &Session, id: &str) -> HttpResponse {
        self.send(
            Method::POST,
            &format!("{TRASH}/{id}/restore"),
            None,
            Some(session),
        )
        .await
    }

    pub async fn permanently_delete_book(&self, session: &Session, id: &str) -> HttpResponse {
        self.send(
            Method::DELETE,
            &format!("{TRASH}/{id}"),
            None,
            Some(session),
        )
        .await
    }

    pub async fn delete_books(&self, session: &Session, ids: &[&str]) -> HttpResponse {
        self.send(
            Method::DELETE,
            BOOKS,
            Some(json!({"ids": ids})),
            Some(session),
        )
        .await
    }

    pub async fn permanently_delete_books(&self, session: &Session, ids: &[&str]) -> HttpResponse {
        self.send(
            Method::DELETE,
            TRASH,
            Some(json!({"ids": ids})),
            Some(session),
        )
        .await
    }

    pub async fn books(&self, session: &Session) -> Vec<BookData> {
        self.books_by_ids(session, &[]).await
    }

    pub async fn books_by_ids(&self, session: &Session, ids: &[&str]) -> Vec<BookData> {
        let response = self.find_books(session, ids).await;
        response.assert_status(200);
        serde_json::from_value(response.json()).expect("book list fields")
    }

    pub async fn find_books(&self, session: &Session, ids: &[&str]) -> HttpResponse {
        let query = ids
            .iter()
            .map(|id| format!("id={id}"))
            .collect::<Vec<_>>()
            .join("&");
        self.send(
            Method::GET,
            &format!("{BOOKS}?{query}"),
            None,
            Some(session),
        )
        .await
    }

    pub async fn create_book(&self, session: &Session, title: &str, hash: &str) -> HttpResponse {
        self.send(
            Method::POST,
            BOOKS,
            Some(json!({"title": title, "hash": hash})),
            Some(session),
        )
        .await
    }

    // The create and list endpoints expose the same complete book representation.
    pub async fn add_book(&self, session: &Session, title: &str, file: SampleFile) -> Book<'_> {
        let before = Timestamp::now();
        let response = self.create_book(session, title, &file.hash()).await;
        response.assert_status(201);
        let created = response.json();
        assert_eq!(created.as_object().unwrap().len(), 6);
        let data: BookData = serde_json::from_value(created).expect("created book fields");
        uuid::Uuid::parse_str(&data.id).expect("book ID");
        assert_eq!(data.title, title);
        assert_eq!(data.hash, file.hash());
        assert_eq!(data.file_type, file.file_type());
        assert!((before..=Timestamp::now()).contains(&data.created_at));
        assert_eq!(data.created_at, data.updated_at);
        let books = self.books_by_ids(session, &[&data.id]).await;
        assert_eq!(books.len(), 1);
        assert_eq!(books[0], data);
        Book {
            data,
            app: self,
            session: session.clone(),
        }
    }

    pub async fn upload_and_add_book(
        &self,
        session: &Session,
        title: &str,
        bytes: &[u8],
    ) -> BookData {
        let hash = blake3::hash(bytes).to_hex().to_string();
        self.upload_parts(Some(session), &[&hash], Some(bytes))
            .await
            .assert_empty(201);
        let response = self.create_book(session, title, &hash).await;
        response.assert_status(201);
        serde_json::from_value(response.json()).expect("created book fields")
    }

    pub async fn rename_book(&self, session: &Session, id: &str, title: &str) -> HttpResponse {
        self.send(
            Method::PATCH,
            &format!("{BOOKS}/{id}"),
            Some(json!({"title": title})),
            Some(session),
        )
        .await
    }
    pub async fn delete_book(&self, session: &Session, id: &str) -> HttpResponse {
        self.send(
            Method::DELETE,
            &format!("{BOOKS}/{id}"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn cover(&self, session: &Session, id: &str) -> HttpResponse {
        self.send(
            Method::GET,
            &format!("{BOOKS}/{id}/cover"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn book_navigation(&self, session: Option<&Session>, id: &str) -> HttpResponse {
        self.send(
            Method::GET,
            &shiro_http_contract::paths::book_navigation(id),
            None,
            session,
        )
        .await
    }
    pub async fn upload(&self, session: &Session, file: SampleFile) {
        self.upload_parts(Some(session), &[&file.hash()], Some(&file.bytes()))
            .await
            .assert_empty(201);
    }
    pub async fn upload_again(&self, session: &Session, file: SampleFile) -> HttpResponse {
        self.upload_parts(Some(session), &[&file.hash()], Some(&file.bytes()))
            .await
    }
    pub async fn find_file(&self, session: &Session, hash: &str) -> HttpResponse {
        self.send(
            Method::GET,
            &format!("{FILES}?hash={hash}"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn find_file_raw(&self, session: &Session, raw_query: &str) -> HttpResponse {
        self.send(
            Method::GET,
            &format!("{FILES}{raw_query}"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn download(&self, session: &Session, book_id: &str) -> HttpResponse {
        self.send(
            Method::GET,
            &format!("{BOOKS}/{book_id}/file"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn head_book_file(&self, session: &Session, book_id: &str) -> HttpResponse {
        self.send(
            Method::HEAD,
            &format!("{BOOKS}/{book_id}/file"),
            None,
            Some(session),
        )
        .await
    }
    pub async fn download_with_headers(
        &self,
        session: &Session,
        book_id: &str,
        headers: &[(&str, &str)],
    ) -> HttpResponse {
        let auth = format!("Bearer {}", session.access_token);
        self.request_with_headers(
            Method::GET,
            &format!("{BOOKS}/{book_id}/file"),
            Body::empty(),
            None,
            Some(&auth),
            headers,
        )
        .await
    }
    pub async fn raw_get(&self, session: &Session, uri: &str) -> HttpResponse {
        self.send(Method::GET, uri, None, Some(session)).await
    }
    pub async fn upload_raw_multipart(
        &self,
        session: Option<&Session>,
        parts: &[(&str, Option<&str>, &[u8])],
    ) -> HttpResponse {
        let boundary = "shiro-reader-test-boundary";
        let mut bytes = Vec::new();
        for &(name, filename, content) in parts {
            match filename {
                Some(filename) => {
                    bytes.extend_from_slice(
                        format!(
                            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                }
                None => {
                    bytes.extend_from_slice(
                        format!(
                            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                }
            }
            bytes.extend_from_slice(content);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let authorization = session.map(|s| format!("Bearer {}", s.access_token));
        self.request(
            Method::POST,
            FILES,
            Body::from(bytes),
            Some(&format!("multipart/form-data; boundary={boundary}")),
            authorization.as_deref(),
        )
        .await
    }
    pub async fn upload_parts(
        &self,
        session: Option<&Session>,
        hashes: &[&str],
        file: Option<&[u8]>,
    ) -> HttpResponse {
        let mut parts: Vec<(&str, Option<&str>, &[u8])> = hashes
            .iter()
            .map(|hash| ("hash", None, hash.as_bytes()))
            .collect();
        let size_str = file.map(|bytes| bytes.len().to_string());
        if let Some(ref size) = size_str {
            parts.push(("size", None, size.as_bytes()));
        }
        if let Some(file) = file {
            parts.push(("file", Some("book.book"), file));
        }
        self.upload_raw_multipart(session, &parts).await
    }
}

impl Book<'_> {
    pub async fn rename(&self, title: &str) -> HttpResponse {
        self.app
            .rename_book(&self.session, &self.data.id, title)
            .await
    }
    pub async fn delete(&self) -> HttpResponse {
        self.app.delete_book(&self.session, &self.data.id).await
    }
    pub async fn cover(&self) -> HttpResponse {
        self.app.cover(&self.session, &self.data.id).await
    }
    pub async fn reload(&self) -> BookData {
        let books = self.app.books_by_ids(&self.session, &[&self.data.id]).await;
        assert_eq!(books.len(), 1);
        books.into_iter().next().unwrap()
    }
    pub async fn assert_renamed(&self, title: &str) {
        let current = self.reload().await;
        assert_eq!(current.title, title);
        assert_eq!(current.id, self.data.id);
        assert_eq!(current.hash, self.data.hash);
        assert_eq!(current.file_type, self.data.file_type);
        assert_eq!(current.created_at, self.data.created_at);
        assert!(current.updated_at > self.data.updated_at);
    }
    pub async fn assert_unchanged(&self) {
        assert_eq!(self.reload().await, self.data);
    }
}

pub struct UnsupportedSample {
    pub bytes: Vec<u8>,
    pub hash: String,
}
impl UnsupportedSample {
    pub fn new() -> Self {
        let bytes = b"valid hash but unsupported book format".to_vec();
        let hash = blake3::hash(&bytes).to_hex().to_string();
        Self { bytes, hash }
    }
}
