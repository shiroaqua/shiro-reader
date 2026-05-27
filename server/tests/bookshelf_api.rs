mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{
    assert_bookshelf_invalid_id_error, assert_bookshelf_invalid_name_format_error,
    assert_bookshelf_missing_name_error, assert_bookshelf_name_conflict_error,
    assert_bookshelf_not_found_error, assert_rfc3339_datetime_string, assert_uuid_string,
    json_body, TestApp, ANOTHER_SAMPLE_BOOKSHELF_NAME, INVALID_BOOKSHELF_NAMES, INVALID_UUID,
    RENAMED_BOOKSHELF_NAME, SAMPLE_BOOKSHELF_NAME, UNKNOWN_UUID,
};

use crate::support::EMPTY_STRING;



#[tokio::test]
async fn create_bookshelf() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/api/v1/library/bookshelves",
            json!({ "name": SAMPLE_BOOKSHELF_NAME }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_uuid_string(&body["data"]["id"]);
    assert_rfc3339_datetime_string(&body["data"]["created_at"]);
}

#[tokio::test]
async fn list_bookshelves() {
    let app = TestApp::new().await;
    let bookshelf_id_first = app.create_sample_bookshelf().await;
    let bookshelf_id_second = app.create_another_sample_bookshelf().await;

    let response = app.get("/api/v1/library/bookshelves").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
   
    assert_eq!(body["data"][0]["id"], bookshelf_id_first);
    assert_eq!(body["data"][0]["name"], SAMPLE_BOOKSHELF_NAME);
    
    assert_eq!(body["data"][1]["id"], bookshelf_id_second);
    assert_eq!(body["data"][1]["name"], ANOTHER_SAMPLE_BOOKSHELF_NAME);
}

#[tokio::test]
async fn get_bookshelf_by_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    let response = app
        .get(&format!("/api/v1/library/bookshelves?id={bookshelf_id}"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], bookshelf_id);
    assert_eq!(body["data"]["name"], SAMPLE_BOOKSHELF_NAME);
}

#[tokio::test]
async fn rename_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    let response = app
        .patch_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}"),
            json!({ "name": RENAMED_BOOKSHELF_NAME }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let fetched = app
        .get(&format!("/api/v1/library/bookshelves?id={bookshelf_id}"))
        .await;
    let body = json_body(fetched).await;
    assert_eq!(body["data"]["name"], RENAMED_BOOKSHELF_NAME);
}

#[tokio::test]
async fn delete_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    let response = app
        .delete(&format!("/api/v1/library/bookshelves/{bookshelf_id}"))
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_bookshelf_not_found_error(
        app.get(&format!("/api/v1/library/bookshelves?id={bookshelf_id}"))
            .await,
    )
    .await;
}

#[tokio::test]
async fn create_bookshelf_rejects_missing_name() {
    let app = TestApp::new().await;

    assert_bookshelf_missing_name_error(
        app.post_json(
            "/api/v1/library/bookshelves",
            json!({ "name": EMPTY_STRING }),
        )
            .await,
    )
    .await;
}

#[tokio::test]
async fn create_bookshelf_rejects_invalid_name_format() {
    let app = TestApp::new().await;

    for invalid_name in INVALID_BOOKSHELF_NAMES {
        assert_bookshelf_invalid_name_format_error(
            app.post_json(
                "/api/v1/library/bookshelves",
                json!({ "name": invalid_name }),
            )
            .await,
        )
        .await;
    }
}

#[tokio::test]
async fn create_bookshelf_rejects_duplicate_name() {
    let app = TestApp::new().await;
    app.create_sample_bookshelf().await;

    assert_bookshelf_name_conflict_error(
        app.post_json(
            "/api/v1/library/bookshelves",
            json!({ "name": SAMPLE_BOOKSHELF_NAME }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn get_bookshelf_rejects_invalid_id() {
    let app = TestApp::new().await;

    assert_bookshelf_invalid_id_error(
        app.get(&format!("/api/v1/library/bookshelves?id={INVALID_UUID}"))
            .await,
    )
    .await;
}

#[tokio::test]
async fn get_bookshelf_returns_not_found_for_unknown_id() {
    let app = TestApp::new().await;

    assert_bookshelf_not_found_error(
        app.get(&format!("/api/v1/library/bookshelves?id={UNKNOWN_UUID}"))
            .await,
    )
    .await;
}


#[tokio::test]
async fn delete_bookshelf_rejects_invalid_id() {
    let app = TestApp::new().await;

    assert_bookshelf_invalid_id_error(
        app.delete(&format!("/api/v1/library/bookshelves/{INVALID_UUID}"))
            .await,
    )
    .await;
}


#[tokio::test]
async fn delete_bookshelf_returns_not_found_for_unknown_id() {
    let app = TestApp::new().await;

    assert_bookshelf_not_found_error(
        app.delete(&format!("/api/v1/library/bookshelves/{UNKNOWN_UUID}"))
            .await,
    )
    .await;
}


#[tokio::test]
async fn rename_bookshelf_rejects_missing_name() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_bookshelf_missing_name_error(
        app.patch_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}"),
            json!({ "name": EMPTY_STRING }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn rename_bookshelf_rejects_invalid_name_format() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    for invalid_name in INVALID_BOOKSHELF_NAMES {
        assert_bookshelf_invalid_name_format_error(
            app.patch_json(
                &format!("/api/v1/library/bookshelves/{bookshelf_id}"),
                json!({ "name": invalid_name }),
            )
            .await,
        )
        .await;
    }
}
