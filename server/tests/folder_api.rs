mod support;

use axum::http::StatusCode;

use crate::support::{EMPTY_STRING, INVALID_FOLDER_NAMES, INVALID_UUID, ROOT_FOLDER_ID, TestApp, UNKNOWN_UUID};

#[tokio::test]
async fn create_parentless_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let folder = bookshelf.create_default_folder().await;
    folder.assert_data();
}

#[tokio::test]
async fn create_child_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let folder = bookshelf.create_default_folder().await;
    let child_folder = folder.create_default_folder().await;
    child_folder.assert_data();
}

#[tokio::test]
async fn create_folder_missing_name_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let folder = bookshelf.create_folder(EMPTY_STRING).await;
    folder.response.assert_folder_missing_name_error();
}

#[tokio::test]
async fn create_folder_name_conflict_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.create_default_folder().await;
    bookshelf
        .create_default_folder()
        .await
        .response
        .assert_folder_name_conflict_error();
}

#[tokio::test]
async fn create_folder_invalid_name_format_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    for invalid_name in INVALID_FOLDER_NAMES {
        let folder = bookshelf.create_folder(invalid_name).await;
        folder.response.assert_folder_invalid_name_format_error();
    }
}

#[tokio::test]
async fn create_folder_invalid_parent_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.id = INVALID_UUID.to_owned();
    folder
        .create_default_folder()
        .await
        .response
        .assert_folder_invalid_parent_id_error();
}

#[tokio::test]
async fn create_folder_not_found_parent_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.id = UNKNOWN_UUID.to_owned();
    folder
        .create_default_folder()
        .await
        .response
        .assert_folder_parent_not_found_error();
}

#[tokio::test]
async fn delete_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let parent_folder = bookshelf.create_default_folder().await;
    let child_folder = parent_folder.create_default_folder().await;

    let response = child_folder.delete().await;
    assert_eq!(response.status_code, StatusCode::NO_CONTENT);

    let response = parent_folder.delete().await;
    assert_eq!(response.status_code, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_folder_invalid_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.id = INVALID_UUID.to_owned();
    folder.delete().await.assert_folder_invalid_id_error();
}

#[tokio::test]
async fn delete_folder_not_found_for_unknown_folder_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.id = UNKNOWN_UUID.to_owned();
    folder.delete().await.assert_folder_not_found_error();
}

#[tokio::test]
async fn rename_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.rename("qwq").await;

    assert_eq!(folder.response.status_code, StatusCode::NO_CONTENT);
    assert_eq!(bookshelf.get_folder(&folder.id).await.name, "qwq");
}

#[tokio::test]
async fn rename_folder_rejects_missing_name() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;
    folder.rename(EMPTY_STRING).await;
    folder.response.assert_folder_missing_name_error();
}

#[tokio::test]
async fn rename_folder_rejects_invalid_name_format() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_default_folder().await;

    for invalid_name in INVALID_FOLDER_NAMES {
        folder.rename(invalid_name).await;
        folder.response.assert_folder_invalid_name_format_error();
    }
}

#[tokio::test]
async fn move_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.create_folder_tree().await;

    // move child to root
    let mut tree = bookshelf.get_folders().await;
    let mut folder = tree[0].children.remove(0).folder;
    folder.move_to(ROOT_FOLDER_ID).await;

    let mut tree = bookshelf.get_folders().await;
    assert_eq!(folder.response.status_code, StatusCode::NO_CONTENT);
    assert!(tree.iter().any(|f| f.folder.name == folder.name));

    // move root to child
    let mut folder = tree.remove(0).folder;
    folder.move_to(&tree[0].children[0].folder.id).await;
    
    let tree = bookshelf.get_folders().await;
    assert_eq!(folder.response.status_code, StatusCode::NO_CONTENT);
    assert!(tree[0].children.iter().any(|f| f.children.iter().any(|c| c.folder.name == folder.name)));
}


#[tokio::test]
async fn move_folder_name_conflict_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    
    let root_folder_zero = bookshelf.create_folder("0").await;
    let mut child_folder = root_folder_zero.create_folder("0").await;
    child_folder.move_to(ROOT_FOLDER_ID).await;
    child_folder.response.assert_folder_name_conflict_error();

    
    let mut root_folder_one = bookshelf.create_folder("1").await;
    root_folder_zero.create_folder("1").await;
    root_folder_one.move_to(&root_folder_zero.id).await;
    root_folder_one.response.assert_folder_name_conflict_error();
}

#[tokio::test]
async fn move_folder_cycled_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.create_folder_tree().await;

    // 自指
    let mut tree = bookshelf.get_folders().await;
    let mut folder = tree.remove(0).folder;
    folder.move_to(&folder.id.to_owned()).await;
    folder.response.assert_folder_cycled_error();

    // 向下
    let id = tree[0].children[0].folder.id.to_owned();
    let mut folder = tree.remove(0).folder;
    folder.move_to(&id).await;
    folder.response.assert_folder_cycled_error();
}

#[tokio::test]
async fn move_folder_invalid_folder_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_folder("0").await;
    folder.move_to(INVALID_UUID).await;
}


#[tokio::test]
async fn move_folder_not_found_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let mut folder = bookshelf.create_folder("0").await;
    folder.move_to(UNKNOWN_UUID).await;
}



#[tokio::test]
async fn get_folder() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    let folder_raw = bookshelf.create_default_folder().await;
    let folder_from_get = bookshelf.get_folder(&folder_raw.id).await;

    assert_eq!(folder_from_get.response.status_code, StatusCode::OK);
    assert_eq!(folder_raw.id, folder_from_get.id);
    assert_eq!(folder_raw.name, folder_from_get.name);
    assert_eq!(folder_raw.created_at, folder_from_get.created_at);
    assert_eq!(folder_raw.updated_at, folder_from_get.updated_at);
}

#[tokio::test]
async fn get_folders() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    assert_eq!(bookshelf.get_folders().await.len(), 0);

    bookshelf.create_folder_tree().await;
    let tree = bookshelf.get_folders().await;
    assert_eq!(
        tree.first().unwrap().folder.response.status_code,
        StatusCode::OK
    );

    for i in 0..2 {
        assert!(tree.iter().any(|f| f.folder.name == format!("{}00", i)));
        for j in 0..3 {
            assert!(
                tree.iter()
                    .find(|f| f.folder.name.starts_with(&i.to_string()))
                    .unwrap()
                    .children
                    .iter()
                    .any(|f| f.folder.name == format!("{}{}0", i, j))
            );
            for n in 0..3 {
                assert!(
                    tree.iter()
                        .find(|f| f.folder.name.starts_with(&i.to_string()))
                        .unwrap()
                        .children
                        .iter()
                        .find(|f| f.folder.name.starts_with(&format!("{}{}", i, j)))
                        .unwrap()
                        .children
                        .iter()
                        .any(|f| f.folder.name == format!("{}{}{}", i, j, n))
                );
            }
        }
    }
}


#[tokio::test]
async fn get_folders_is_scoped_to_bookshelf() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;
    let folder = bookshelf.create_default_folder().await;
    bookshelf.id = UNKNOWN_UUID.to_owned();
    let folder = bookshelf.get_folder(&folder.id).await;
    assert_eq!(folder.response.status_code, StatusCode::NOT_FOUND);
    assert_eq!(bookshelf.get_folders().await.len(),0);
}


#[tokio::test]
async fn get_folder_invalid_bookshelf_id_error() {
    let app = TestApp::new().await;
    let mut bookshelf = app.create_default_bookshelf().await;
    bookshelf.id = INVALID_UUID.to_owned();
    bookshelf.get_folder(INVALID_UUID).await.response.assert_bookshelf_invalid_id_error();
}


#[tokio::test]
async fn get_folder_invalid_folder_id_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.get_folder(INVALID_UUID).await.response.assert_folder_invalid_id_error();
}


#[tokio::test]
async fn get_folder_not_found_error() {
    let app = TestApp::new().await;
    let bookshelf = app.create_default_bookshelf().await;
    bookshelf.get_folder(UNKNOWN_UUID).await.response.assert_folder_not_found_error();
}
