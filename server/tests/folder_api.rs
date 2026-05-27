mod support;

use axum::{
    http::{StatusCode},
};
use serde_json::{Value, json};
use support::{
    assert_error, assert_rfc3339_datetime_string, assert_uuid_string, json_body, TestApp,
    FOLDER_TREE_CHILD_NAME, FOLDER_TREE_GRANDCHILD_NAME, FOLDER_TREE_OTHER_ROOT_NAME,
    FOLDER_TREE_ROOT_NAME, FOLDER_TREE_SECOND_CHILD_NAME, INVALID_FOLDER_NAMES, INVALID_UUID,
    RENAMED_FOLDER_NAME, SAMPLE_CHILD_FOLDER_NAME, SAMPLE_ROOT_FOLDER_NAME, UNKNOWN_UUID,
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

#[tokio::test]
async fn get_folders_default() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let tree = app.create_folder_tree(&bookshelf_id).await;

    let response = app.get(&folders_uri(&bookshelf_id, None)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let folders = folders_data(&body);
    assert_eq!(folders.len(), 2);
    assert!(find_folder(folders, &tree.child_id).is_none());

    let root = find_folder(folders, &tree.root_id).expect("root folder");
    assert_folder_node(root, &tree.root_id, FOLDER_TREE_ROOT_NAME);
    assert!(children(root).is_empty());

    let other_root = find_folder(folders, &tree.other_root_id).expect("other root folder");
    assert_folder_node(other_root, &tree.other_root_id, FOLDER_TREE_OTHER_ROOT_NAME);
    assert!(children(other_root).is_empty());
}

#[tokio::test]
async fn get_folders_returns_empty_list_when_bookshelf_has_no_folders() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    let response = app.get(&folders_uri(&bookshelf_id, None)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(folders_data(&body).is_empty());
}

#[tokio::test]
async fn get_folders_returns_single_folder_by_id_without_descendants() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let tree = app.create_folder_tree(&bookshelf_id).await;

    let response = app
        .get(&folders_uri(&bookshelf_id, Some(&format!("id={}", tree.child_id))))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let folders = folders_data(&body);
    assert_eq!(folders.len(), 1);
    assert_folder_node(&folders[0], &tree.child_id, FOLDER_TREE_CHILD_NAME);
    assert!(children(&folders[0]).is_empty());
}

#[tokio::test]
async fn get_folders_returns_recursive_subtree_by_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let tree = app.create_folder_tree(&bookshelf_id).await;

    let response = app
        .get(&folders_uri(
            &bookshelf_id,
            Some(&format!("recursive=true&id={}", tree.child_id)),
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let folders = folders_data(&body);
    assert_eq!(folders.len(), 1);
    assert!(find_folder(folders, &tree.root_id).is_none());
    assert!(find_folder(folders, &tree.second_child_id).is_none());

    let child = &folders[0];
    assert_folder_node(child, &tree.child_id, FOLDER_TREE_CHILD_NAME);
    assert_eq!(children(child).len(), 1);
    assert_folder_node(
        &children(child)[0],
        &tree.grandchild_id,
        FOLDER_TREE_GRANDCHILD_NAME,
    );
}

#[tokio::test]
async fn get_folders_returns_recursive_tree() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let tree = app.create_folder_tree(&bookshelf_id).await;

    let response = app
        .get(&folders_uri(&bookshelf_id, Some("recursive=true")))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let folders = folders_data(&body);
    assert_eq!(folders.len(), 2);

    let root = find_folder(folders, &tree.root_id).expect("root folder");
    assert_folder_node(root, &tree.root_id, FOLDER_TREE_ROOT_NAME);
    assert_eq!(children(root).len(), 2);

    let child = find_folder(children(root), &tree.child_id).expect("child folder");
    assert_folder_node(child, &tree.child_id, FOLDER_TREE_CHILD_NAME);
    assert_eq!(children(child).len(), 1);
    assert_folder_node(
        &children(child)[0],
        &tree.grandchild_id,
        FOLDER_TREE_GRANDCHILD_NAME,
    );

    let second_child =
        find_folder(children(root), &tree.second_child_id).expect("second child folder");
    assert_folder_node(
        second_child,
        &tree.second_child_id,
        FOLDER_TREE_SECOND_CHILD_NAME,
    );
    assert!(children(second_child).is_empty());

    let other_root = find_folder(folders, &tree.other_root_id).expect("other root folder");
    assert_folder_node(other_root, &tree.other_root_id, FOLDER_TREE_OTHER_ROOT_NAME);
    assert!(children(other_root).is_empty());
}


#[tokio::test]
async fn get_folders_is_scoped_to_bookshelf() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;
    let other_bookshelf_id = app.create_another_sample_bookshelf().await;
    let root_id = app
        .create_folder(&bookshelf_id, None, FOLDER_TREE_ROOT_NAME)
        .await;
    let other_root_id = app
        .create_folder(&other_bookshelf_id, None, FOLDER_TREE_ROOT_NAME)
        .await;

    let response = app
        .get(&folders_uri(&bookshelf_id, Some("recursive=true")))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let folders = folders_data(&body);
    assert_eq!(folders.len(), 1);
    assert!(find_folder(folders, &other_root_id).is_none());
    assert_folder_node(&folders[0], &root_id, FOLDER_TREE_ROOT_NAME);
}

#[tokio::test]
async fn get_folders_rejects_invalid_bookshelf_id() {
    let app = TestApp::new().await;

    assert_error(
        app.get(&folders_uri(INVALID_UUID, None)).await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.invalid_id",
    )
    .await;
}

#[tokio::test]
async fn get_folders_rejects_invalid_folder_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.get(&folders_uri(
            &bookshelf_id,
            Some(&format!("id={INVALID_UUID}")),
        ))
        .await,
        StatusCode::BAD_REQUEST,
        "library.bookshelf.folder.invalid_id",
    )
    .await;
}

#[tokio::test]
async fn get_folders_returns_not_found_for_unknown_folder_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.get(&folders_uri(
            &bookshelf_id,
            Some(&format!("id={UNKNOWN_UUID}")),
        ))
        .await,
        StatusCode::NOT_FOUND,
        "library.bookshelf.folder.not_found",
    )
    .await;
}

#[tokio::test]
async fn get_folders_recursive_returns_not_found_for_unknown_folder_id() {
    let app = TestApp::new().await;
    let bookshelf_id = app.create_sample_bookshelf().await;

    assert_error(
        app.get(&folders_uri(
            &bookshelf_id,
            Some(&format!("recursive=true&id={UNKNOWN_UUID}")),
        ))
        .await,
        StatusCode::NOT_FOUND,
        "library.bookshelf.folder.not_found",
    )
    .await;
}

fn folders_data(body: &Value) -> &[Value] {
    body["data"].as_array().expect("folders data").as_slice()
}

fn folders_uri(bookshelf_id: &str, query: Option<&str>) -> String {
    let uri = format!("/api/v1/library/bookshelves/{bookshelf_id}/folders");
    match query {
        Some(query) => format!("{uri}?{query}"),
        None => uri,
    }
}

fn children(node: &Value) -> &[Value] {
    node["children"]
        .as_array()
        .expect("folder node children")
        .as_slice()
}

fn find_folder<'a>(folders: &'a [Value], folder_id: &str) -> Option<&'a Value> {
    folders
        .iter()
        .find(|folder| folder["id"].as_str() == Some(folder_id))
}

fn assert_folder_node(node: &Value, expected_id: &str, expected_name: &str) {
    assert_eq!(node["id"], expected_id);
    assert_uuid_string(&node["id"]);
    assert_eq!(node["name"], expected_name);
    assert_rfc3339_datetime_string(&node["created_at"]);
    assert_rfc3339_datetime_string(&node["updated_at"]);
    assert!(node["children"].is_array());
}