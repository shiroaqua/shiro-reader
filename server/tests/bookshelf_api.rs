mod support;
use axum::http::StatusCode;
use crate::support::{EMPTY_STRING, INVALID_BOOKSHELF_NAMES, INVALID_UUID, TestApp, UNKNOWN_UUID};

#[tokio::test]
async fn create_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.assert_data();
}

#[tokio::test]
async fn create_bookshelf_missing_name_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_bookshelf(EMPTY_STRING).await;
    bookshelf.response.assert_bookshelf_missing_name_error();
}

#[tokio::test]
async fn create_bookshelf_invalid_name_format_error() {
    let app = TestApp::new().await;

    for invalid_name in INVALID_BOOKSHELF_NAMES {
        let bookshelf = app.create_bookshelf(invalid_name).await;
        bookshelf
            .response
            .assert_bookshelf_invalid_name_format_error();
    }
}

#[tokio::test]
async fn create_bookshelf_name_conflict_error() {
    let app = TestApp::new().await;
    app.create_default_bookshelf().await;
    app.create_default_bookshelf()
        .await
        .response
        .assert_bookshelf_name_conflict_error();
}

#[tokio::test]
async fn delete_bookshelf() {
    let app = TestApp::new().await;
    let response = app.create_default_bookshelf().await.delete().await;

    assert_eq!(response.status_code, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_bookshelf_invalid_id_error() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;
    bookshelf.id = INVALID_UUID.to_owned();
    bookshelf.delete().await.assert_bookshelf_invalid_id_error();
}

#[tokio::test]
async fn delete_bookshelf_not_found_error() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;
    bookshelf.id = UNKNOWN_UUID.to_owned();
    bookshelf.delete().await.assert_bookshelf_not_found_error();
}

#[tokio::test]
async fn rename_bookshelf() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;

    bookshelf.rename("nya").await;
    assert_eq!(bookshelf.response.status_code, StatusCode::NO_CONTENT);

    let bookshelf = app.get_bookshelf(&bookshelf.id).await;
    assert_eq!(bookshelf.name, "nya");
}

#[tokio::test]
async fn rename_bookshelf_missing_name_error() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;
 
    bookshelf.rename(EMPTY_STRING).await;
    bookshelf.response.assert_bookshelf_missing_name_error();
}

#[tokio::test]
async fn rename_bookshelf_invalid_name_format_error() {
    let app = TestApp::new().await;
    let mut bookshelf_id = app.create_default_bookshelf().await;

    for invalid_name in INVALID_BOOKSHELF_NAMES {
        bookshelf_id.rename(invalid_name).await;
        bookshelf_id
            .response
            .assert_bookshelf_invalid_name_format_error();
    }
}

#[tokio::test]
async fn get_bookshelf_by_id() {
    let app = TestApp::new().await;
    let bookshelf = app.create_bookshelf("qwq").await;

    let bookshelf_id = bookshelf.id;
    let bookshelf = app.get_bookshelf(&bookshelf_id).await;

    bookshelf.assert_data();
    assert_eq!(bookshelf.response.status_code, StatusCode::OK);
    assert_eq!(bookshelf.id, bookshelf_id);
    assert_eq!(bookshelf.name, "qwq");
}

#[tokio::test]
async fn list_bookshelves() {
    let app = TestApp::new().await;
    let mut ids: Vec<(String, i32)> = vec![];

    assert_eq!(app.get_bookshelves().await.len(), 0);

    for i in 0..8 {
        ids.push((app.create_bookshelf(&i.to_string()).await.id, i));
    }

    let bookshelves = app.get_bookshelves().await;
    assert_eq!(bookshelves.len(), ids.len());

    assert_eq!(bookshelves[0].response.status_code, StatusCode::OK);

    for id in ids {
        if let Some(bookshelf) = bookshelves.iter().find(|f| f.id == id.0) {
            assert_eq!(&bookshelf.name, &id.1.to_string());
        }
    }
}

#[tokio::test]
async fn get_bookshelf_invalid_id_error() {
    let app = TestApp::new().await;
    app.get_bookshelf(INVALID_UUID).await.response.assert_bookshelf_invalid_id_error();
}

#[tokio::test]
async fn get_bookshelf_not_found_error() {
    let app = TestApp::new().await;
    app.get_bookshelf(UNKNOWN_UUID).await.response.assert_bookshelf_not_found_error();
}
