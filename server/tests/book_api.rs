use axum::http::StatusCode;

use crate::support::{Book, EMPTY_STRING, INVALID_BOOK_TITLES, INVALID_HASH, INVALID_UUID, SampleFile, TestApp, UNKNOWN_HASH, UNKNOWN_UUID, body_bytes};

mod support;

#[tokio::test]
async fn create_book() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
  
    let book = bookshelf.create_default_book().await;
    book.assert_data();
    assert_eq!(book.response.status_code, StatusCode::CREATED);
}

#[tokio::test]
async fn create_book_missing_titile_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let book = bookshelf.create_book(EMPTY_STRING, UNKNOWN_HASH).await;
    book.response.assert_book_missing_title_error();
}


#[tokio::test]
async fn create_book_invalid_title_format_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    for title in INVALID_BOOK_TITLES {
     bookshelf.create_book(title, UNKNOWN_HASH).await.response.assert_book_invalid_title_format_error();
    }
}


#[tokio::test]
async fn create_book_invalid_hash_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let book = bookshelf.create_book("title", INVALID_HASH).await;
    book.response.assert_book_file_invalid_hash_format_error();
}


#[tokio::test]
async fn create_book_not_found_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let book = bookshelf.create_book("title", UNKNOWN_HASH).await;
    book.response.assert_book_file_not_found_error();
}

#[tokio::test]
async fn delete_book() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
  
    assert_eq!(bookshelf.create_default_book().await.delete().await.status_code, StatusCode::NO_CONTENT);
    assert_eq!(bookshelf.get_books().await.len(), 0);
  
}


#[tokio::test]
async fn delete_book_invalid_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut book = bookshelf.create_default_book().await;
    book.id = INVALID_UUID.to_owned();
    book.delete().await.assert_book_invalid_id_error();
}

#[tokio::test]
async fn delete_book_not_found_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut book = bookshelf.create_default_book().await;
    book.id = UNKNOWN_UUID.to_owned();
    book.delete().await.assert_book_not_found_error();
}


#[tokio::test]
async fn rename_book() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
  
    let mut book = bookshelf.create_default_book().await;
    book.rename("QAQ").await;
    assert_eq!(book.response.status_code, StatusCode::NO_CONTENT);
    let books = bookshelf.get_books().await;
    assert_eq!(books.iter().find(|item| item.id == book.id).unwrap().title, "QAQ");
}

#[tokio::test]
async fn rename_book_missing_title_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut book = bookshelf.create_default_book().await;
    book.rename(EMPTY_STRING).await;
    book.response.assert_book_missing_title_error();
}

#[tokio::test]
async fn rename_book_invalid_title_format_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut book = bookshelf.create_default_book().await;
    for title in INVALID_BOOK_TITLES {
        book.rename(title).await;
        book.response.assert_book_invalid_title_format_error();
    }
}

#[tokio::test]
async fn get_books_from_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;

    let first = bookshelf.create_book("PDF", &app.upload_sample_book_file(SampleFile::PDF).await.1.to_hex()).await;
    let second = bookshelf.create_book("EPUB", &app.upload_sample_book_file(SampleFile::EPUB).await.1.to_hex()).await;
    let books = bookshelf.get_books().await;
    assert_eq!(books.len(), 2);
    assert_book(books.iter().find(|f| f.id == first.id).unwrap(), &first);
    assert_book(books.iter().find(|f| f.id == second.id).unwrap(), &second);
}

#[tokio::test]
async fn get_books_by_ids_from_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;

    let first = bookshelf.create_book("PDF", &app.upload_sample_book_file(SampleFile::PDF).await.1.to_hex()).await;
    let second = bookshelf.create_book("EPUB", &app.upload_sample_book_file(SampleFile::EPUB).await.1.to_hex()).await;

    let books = bookshelf.get_books_by_ids(&[&first.id, &second.id]).await;
    assert_eq!(books.status_code, StatusCode::OK);
    let books = Book::from_data_array(&app, books);
    assert_eq!(books.len(), 2);
    assert_book(books.iter().find(|book| book.id == first.id).unwrap(), &first);
    assert_book(books.iter().find(|book| book.id == second.id).unwrap(), &second);
}

#[tokio::test]
async fn get_book_cover() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;

    // 屏蔽原因：PDF的封面是渲染的不具有跨平台跨版本稳定性。
    // let hash = app.upload_sample_book_file(SampleFile::PDF).await.1;
    // assert_cover_hash(
    //     bookshelf.create_book("awa", &hash.to_hex()).await,
    //     "d3ceb887c2894a092ee23b815676f06368fbdc50af9a4b62bed62c376a8c0aab",
    // )
    // .await;

    let hash = app.upload_sample_book_file(SampleFile::EPUB).await.1;
    assert_cover_hash(
        bookshelf.create_book("qwq", &hash.to_hex()).await,
        "75e470ff9e488fc6fbd3d122c15f4e4296bcf0ea688ef8c071c5b71c64a03f38",
    )
    .await;
}

async fn assert_cover_hash(book: Book<'_>, expected_hash: &str) {
    let response = book.get_cover().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_bytes(response).await;
    let hash = blake3::hash(&body.to_vec())
        .to_hex()
        .to_string();
    assert_eq!(hash, expected_hash);
}

fn assert_book(a: &Book, b: &Book) {
    assert_eq!(a.id, b.id);
    assert_eq!(a.title, b.title);
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.bookshelf_id, b.bookshelf_id);
    assert_eq!(a.created_at, b.created_at);
    assert_eq!(a.updated_at, b.updated_at);
}
