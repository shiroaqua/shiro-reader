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

    let bookshelves = app.get_bookshelves_by_ids(&[&bookshelf.id]).await;
    assert_eq!(bookshelves.status_code, StatusCode::OK);
    assert_eq!(bookshelves.as_array()[0]["name"], "nya");
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
async fn get_bookshelves_by_ids() {
    let app = TestApp::new().await;
    let first = app.create_bookshelf("first").await;
    let second = app.create_bookshelf("second").await;

    let bookshelves = app.get_bookshelves_by_ids(&[&first.id, &second.id]).await;
    assert_eq!(bookshelves.status_code, StatusCode::OK);
    let bookshelves = bookshelves.as_array();
    assert_eq!(bookshelves.len(), 2);
    assert_eq!(bookshelves.iter().find(|bookshelf| bookshelf["id"] == first.id).unwrap()["name"], "first");
    assert_eq!(bookshelves.iter().find(|bookshelf| bookshelf["id"] == second.id).unwrap()["name"], "second");
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
