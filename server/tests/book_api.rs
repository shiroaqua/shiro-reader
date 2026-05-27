mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{
    assert_book_file_invalid_hash_format_error, assert_book_file_not_found_error,
    assert_book_invalid_id_error, assert_book_invalid_title_format_error,
    assert_book_missing_title_error, assert_book_not_found_error, assert_book_title_conflict_error,
    assert_bookshelf_not_found_error, assert_folder_not_found_error,
    assert_rfc3339_datetime_string, assert_uuid_string, hash_for, json_body, TestApp,
    INVALID_BOOK_HASH, INVALID_BOOK_TITLES, INVALID_UUID, MISSING_SAMPLE_BOOK_BYTES,
    RENAMED_BOOK_TITLE, SAMPLE_BOOK_BYTES, SAMPLE_BOOK_TITLE, UNKNOWN_UUID,
};

use crate::support::EMPTY_STRING;

#[tokio::test]
async fn create_book() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_sample_book_file().await;

    let response = app
        .post_json(
            "/api/v1/library/books",
            json!({
                "title": SAMPLE_BOOK_TITLE,
                "hash": hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": null,
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_uuid_string(&body["data"]["id"]);
    assert_rfc3339_datetime_string(&body["data"]["created_at"]);
}

#[tokio::test]
async fn get_book_by_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let folder_id = app.create_sample_root_folder(&bookshelf_id).await;
    let hash = app.upload_sample_book_file().await;
    let book_id = app
        .create_book(&bookshelf_id, Some(&folder_id), SAMPLE_BOOK_TITLE, &hash)
        .await;

    let response = app
        .get(&format!("/api/v1/library/books?id={book_id}"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], book_id);
    assert_eq!(body["data"]["title"], SAMPLE_BOOK_TITLE);
    assert_eq!(body["data"]["hash"], hash);
    assert_eq!(body["data"]["bookshelf_id"], bookshelf_id);
    assert_eq!(body["data"]["folder_id"], folder_id);
}

#[tokio::test]
async fn rename_book_title() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_sample_book_file().await;
    let book_id = app
        .create_book(&bookshelf_id, None, SAMPLE_BOOK_TITLE, &hash)
        .await;

    let response = app
        .patch_json(
            &format!("/api/v1/library/books/{book_id}"),
            json!({ "title": RENAMED_BOOK_TITLE }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let fetched = app
        .get(&format!("/api/v1/library/books?id={book_id}"))
        .await;
    let body = json_body(fetched).await;
    assert_eq!(body["data"]["title"], RENAMED_BOOK_TITLE);
}

#[tokio::test]
async fn delete_book() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_sample_book_file().await;
    let book_id = app
        .create_book(&bookshelf_id, None, SAMPLE_BOOK_TITLE, &hash)
        .await;

    let response = app.delete(&format!("/api/v1/library/books/{book_id}")).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_book_not_found_error(
        app.get(&format!("/api/v1/library/books?id={book_id}"))
            .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_rejects_missing_title() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = hash_for(SAMPLE_BOOK_BYTES);

    assert_book_missing_title_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": EMPTY_STRING,
                "hash": hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": null,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_rejects_invalid_title_format() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = hash_for(SAMPLE_BOOK_BYTES);

    for invalid_title in INVALID_BOOK_TITLES {
        assert_book_invalid_title_format_error(
            app.post_json(
                "/api/v1/library/books",
                json!({
                    "title": invalid_title,
                    "hash": hash,
                    "bookshelf_id": bookshelf_id,
                    "folder_id": null,
                }),
            )
            .await,
        )
        .await;
    }
}

#[tokio::test]
async fn create_book_rejects_invalid_hash() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_book_file_invalid_hash_format_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": RENAMED_BOOK_TITLE,
                "hash": INVALID_BOOK_HASH,
                "bookshelf_id": bookshelf_id,
                "folder_id": null,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_returns_not_found_when_book_file_is_missing() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_book_file_not_found_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": RENAMED_BOOK_TITLE,
                "hash": hash_for(MISSING_SAMPLE_BOOK_BYTES),
                "bookshelf_id": bookshelf_id,
                "folder_id": null,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_returns_not_found_for_unknown_bookshelf_id() {
    let app = TestApp::new().await;
    let hash = app.upload_another_sample_book_file().await;

    assert_bookshelf_not_found_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": RENAMED_BOOK_TITLE,
                "hash": hash,
                "bookshelf_id": UNKNOWN_UUID,
                "folder_id": null,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_returns_not_found_for_unknown_folder_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_another_sample_book_file().await;

    assert_folder_not_found_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": RENAMED_BOOK_TITLE,
                "hash": hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": UNKNOWN_UUID,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_rejects_folder_from_another_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let other_bookshelf_id = app.create_another_sample_bookshelf().await;
    let other_folder_id = app
        .create_other_sample_root_folder(&other_bookshelf_id)
        .await;
    let hash = app.upload_another_sample_book_file().await;

    assert_folder_not_found_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": RENAMED_BOOK_TITLE,
                "hash": hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": other_folder_id,
            }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn create_book_rejects_duplicate_title_in_same_location() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let first_hash = app.upload_sample_book_file().await;
    app.create_book(&bookshelf_id, None, SAMPLE_BOOK_TITLE, &first_hash)
        .await;
    let second_hash = app.upload_third_sample_book_file().await;

    assert_book_title_conflict_error(
        app.post_json(
            "/api/v1/library/books",
            json!({
                "title": SAMPLE_BOOK_TITLE,
                "hash": second_hash,
                "bookshelf_id": bookshelf_id,
                "folder_id": null,
            }),
        )
        .await,
    )
    .await;
}


#[tokio::test]
async fn get_book_rejects_invalid_id() {
    let app = TestApp::new().await;

    assert_book_invalid_id_error(
        app.get(&format!("/api/v1/library/books?id={INVALID_UUID}"))
            .await,
    )
    .await;
}

#[tokio::test]
async fn get_book_returns_not_found_for_unkown_id() {
    let app = TestApp::new().await;

    assert_book_not_found_error(
        app.get(&format!("/api/v1/library/books?id={UNKNOWN_UUID}"))
            .await,
    )
    .await
}


#[tokio::test]
async fn delete_book_rejects_invalid_id() {
    let app = TestApp::new().await;

    assert_book_invalid_id_error(
        app.delete(&format!("/api/v1/library/books/{INVALID_UUID}"))
            .await,
    )
    .await;

}

#[tokio::test]
async fn delete_book_returns_not_found_for_unknown_id() {
    let app = TestApp::new().await;

    assert_book_not_found_error(
        app.delete(&format!("/api/v1/library/books/{UNKNOWN_UUID}"))
            .await,
    )
    .await;
}


#[tokio::test]
async fn rename_book_rejects_missing_name() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_sample_book_file().await;
    let book_id = app
        .create_book(&bookshelf_id, None, SAMPLE_BOOK_TITLE, &hash)
        .await;

    assert_book_missing_title_error(
        app.patch_json(
            &format!("/api/v1/library/books/{book_id}"),
            json!({ "title": EMPTY_STRING }),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn rename_book_rejects_invalid_title_format() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let hash = app.upload_sample_book_file().await;
    let book_id = app
        .create_book(&bookshelf_id, None, SAMPLE_BOOK_TITLE, &hash)
        .await;

    for invalid_title in INVALID_BOOK_TITLES {
        assert_book_invalid_title_format_error(
            app.patch_json(
                &format!("/api/v1/library/books/{book_id}"),
                json!({ "title": invalid_title }),
            )
            .await,
        )
        .await;
    }
}
