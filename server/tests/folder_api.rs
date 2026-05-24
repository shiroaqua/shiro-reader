mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{
    assert_error, assert_rfc3339_datetime_string, assert_uuid_string, json_body, TestApp,
    INVALID_FOLDER_NAMES, INVALID_UUID, RENAMED_FOLDER_NAME,
    SAMPLE_CHILD_FOLDER_NAME, SAMPLE_ROOT_FOLDER_NAME, UNKNOWN_UUID,
};

use crate::support::EMPTY_STRING;



#[tokio::test]
async fn create_root_folder() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    let response = app
        .post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": null, "name": SAMPLE_ROOT_FOLDER_NAME }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_uuid_string(&body["data"]["id"]);
    assert_rfc3339_datetime_string(&body["data"]["created_at"]);
}

#[tokio::test]
async fn create_child_folder() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let root_id = app.create_sample_root_folder(&bookshelf_id).await;

    let response = app
        .post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": root_id, "name": SAMPLE_CHILD_FOLDER_NAME }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_uuid_string(&body["data"]["id"]);
    assert_rfc3339_datetime_string(&body["data"]["created_at"]);
}

#[tokio::test]
async fn rename_folder() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let folder_id = app.create_sample_root_folder(&bookshelf_id).await;

    let response = app
        .patch_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders/{folder_id}"),
            json!({ "name": RENAMED_FOLDER_NAME }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_folder() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let folder_id = app.create_sample_root_folder(&bookshelf_id).await;

    let response = app
        .delete(&format!(
            "/api/v1/library/bookshelves/{bookshelf_id}/folders/{folder_id}"
        ))
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn create_folder_rejects_missing_name() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": null, "name": EMPTY_STRING }),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.folder.missing_name",
    )
    .await;
}

#[tokio::test]
async fn create_folder_rejects_invalid_name_format() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    for invalid_name in INVALID_FOLDER_NAMES {
        assert_error(
            app.post_json(
                &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
                json!({ "parent_id": null, "name": invalid_name }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "library.bookshelf.folder.invalid_name_format",
        )
        .await;
    }
}

#[tokio::test]
async fn create_folder_rejects_invalid_parent_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": INVALID_UUID, "name": SAMPLE_CHILD_FOLDER_NAME }),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.folder.invalid_parent_id",
    )
    .await;
}

#[tokio::test]
async fn create_folder_returns_not_found_for_unknown_parent_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": UNKNOWN_UUID, "name": SAMPLE_CHILD_FOLDER_NAME }),
        )
        .await,
        StatusCode::NOT_FOUND,
        "library.bookshelf.folder.parent_not_found",
    )
    .await;
}

#[tokio::test]
async fn create_folder_returns_not_found_for_unknown_bookshelf_id() {
    let app = TestApp::new().await;

    assert_error(
        app.post_json(
            &format!("/api/v1/library/bookshelves/{UNKNOWN_UUID}/folders"),
            json!({ "parent_id": null, "name": SAMPLE_ROOT_FOLDER_NAME }),
        )
        .await,
        StatusCode::NOT_FOUND,
        "library.bookshelf.folder.parent_not_found",
    )
    .await;
}

#[tokio::test]
async fn create_folder_rejects_duplicate_sibling_name() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    app.create_sample_root_folder(&bookshelf_id).await;

    assert_error(
        app.post_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders"),
            json!({ "parent_id": null, "name": SAMPLE_ROOT_FOLDER_NAME }),
        )
        .await,
        StatusCode::CONFLICT,
        "library.bookshelf.folder.name_conflict",
    )
    .await;
}


#[tokio::test]
async fn delete_folder_rejects_invalid_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.delete(&format!(
            "/api/v1/library/bookshelves/{bookshelf_id}/folders/{INVALID_UUID}"
        ))
        .await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.folder.invalid_id",
    )
    .await;
}

#[tokio::test]
async fn delete_folder_returns_not_found_for_unknown_folder_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.delete(&format!(
            "/api/v1/library/bookshelves/{bookshelf_id}/folders/{UNKNOWN_UUID}"
        ))
        .await,
        StatusCode::NOT_FOUND,
        "library.bookshelf.folder.not_found",
    )
    .await;
}


#[tokio::test]
async fn rename_folder_rejects_missing_name() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let folder_id = app.create_sample_root_folder(&bookshelf_id).await;

    assert_error(
        app.patch_json(
            &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders/{folder_id}"),
            json!({ "name": EMPTY_STRING }),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.folder.missing_name",
    )
    .await;
}

#[tokio::test]
async fn rename_folder_rejects_invalid_name_format() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let folder_id = app.create_sample_root_folder(&bookshelf_id).await;

    for invalid_name in INVALID_FOLDER_NAMES {
        assert_error(
            app.patch_json(
                &format!("/api/v1/library/bookshelves/{bookshelf_id}/folders/{folder_id}"),
                json!({ "name": invalid_name }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "library.bookshelf.folder.invalid_name_format",
        )
        .await;
    }
}
