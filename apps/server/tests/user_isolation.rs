mod support;
use HttpError::*;
use SampleFile::*;
use support::*;

// 验证用户无法查询、重命名、删除其他用户的私有书籍或获取其封面。
#[tokio::test]
async fn another_user_cannot_read_or_change_private_books() {
    let app = TestApp::new().await;
    let owner = app.session("Owner").await;
    let other = app.session("Other").await;
    app.upload(&owner, Epub).await;
    let book = app.add_book(&owner, "PrivateBook", Epub).await;
    assert!(app.books(&other).await.is_empty());
    assert!(app.books_by_ids(&other, &[&book.data.id]).await.is_empty());
    app.rename_book(&other, &book.data.id, "Hacked")
        .await
        .assert_error(BookNotFound);
    app.delete_book(&other, &book.data.id)
        .await
        .assert_error(BookNotFound);
    app.cover(&other, &book.data.id)
        .await
        .assert_error(BookNotFound);
    app.download(&other, &book.data.id)
        .await
        .assert_error(BookNotFound);
    app.head_book_file(&other, &book.data.id)
        .await
        .assert_status(404);
    book.assert_unchanged().await;
}

// 验证用户共享电子书文件，但同名书籍、重命名和删除操作在各自书架中独立。
#[tokio::test]
async fn users_share_files_but_keep_independent_bookshelves() {
    let app = TestApp::new().await;
    let alice = app.session("Alice").await;
    let bob = app.session("Bob").await;
    app.upload(&alice, Epub).await;
    let first = app.add_book(&alice, "SameTitle", Epub).await;
    let second = app.add_book(&bob, "SameTitle", Epub).await;
    assert_ne!(first.data.id, second.data.id);
    first.rename("Renamed").await.assert_empty(204);
    second.assert_unchanged().await;
    first.delete().await.assert_empty(204);
    assert!(app.books(&alice).await.is_empty());
    second.assert_unchanged().await;
    app.upload_again(&bob, Epub)
        .await
        .assert_error(DuplicateFile);
    app.download(&bob, &second.data.id).await.assert_file(&Epub);
}

// 验证用户无法查看或操作其他用户的回收站书籍、标签树及书籍标签绑定。
#[tokio::test]
async fn another_user_cannot_read_or_mutate_private_trash_and_tags() {
    let app = TestApp::new().await;
    let owner = app.session("Owner").await;
    let other = app.session("Other").await;
    app.upload(&owner, Epub).await;

    let active_book = app.add_book(&owner, "OwnerActive", Epub).await;
    let trashed_book = app.add_book(&owner, "OwnerTrashed", Epub).await;
    trashed_book.delete().await.assert_empty(204);

    let owner_tag = app.add_tag(&owner, "OwnerTag", None, false, None).await;
    app.bind_tag(&owner, &active_book.data.id, &owner_tag.id, None)
        .await
        .assert_empty(204);
    let other_tag = app.add_tag(&other, "OtherTag", None, false, None).await;

    assert!(app.trash(&other).await.is_empty());
    assert_eq!(app.tag_tree(&other).await, [other_tag.node(vec![])]);

    app.restore_book(&other, &trashed_book.data.id)
        .await
        .assert_error(BookNotFound);
    app.permanently_delete_book(&other, &trashed_book.data.id)
        .await
        .assert_error(BookNotFound);
    app.permanently_delete_books(&other, &[&trashed_book.data.id])
        .await
        .assert_error(BookNotFound);
    app.delete_books(&other, &[&active_book.data.id])
        .await
        .assert_error(BookNotFound);
    app.query_book_tags(&other, &active_book.data.id)
        .await
        .assert_error(BookNotFound);
    app.bind_tag(&other, &active_book.data.id, &other_tag.id, None)
        .await
        .assert_error(BookNotFound);
    app.unbind_tag(&other, &active_book.data.id, &other_tag.id)
        .await
        .assert_error(BookNotFound);

    app.rename_tag(&other, &owner_tag.id, "Hacked")
        .await
        .assert_tag_error(TagError::NotFound);
    app.change_tag_color(&other, &owner_tag.id, "#123456")
        .await
        .assert_tag_error(TagError::NotFound);
    app.move_tag(&other, &owner_tag.id, Some(&other_tag.id))
        .await
        .assert_tag_error(TagError::NotFound);
    app.move_tag(&other, &other_tag.id, Some(&owner_tag.id))
        .await
        .assert_tag_error(TagError::ParentNotFound);
    for strategy in [DeleteStrategy::Cascade, DeleteStrategy::Promote] {
        app.delete_tag(&other, &owner_tag.id, strategy)
            .await
            .assert_tag_error(TagError::NotFound);
    }
    app.bind_tag(&other, &active_book.data.id, &owner_tag.id, None)
        .await
        .assert_tag_error(TagError::NotFound);
    app.unbind_tag(&other, &active_book.data.id, &owner_tag.id)
        .await
        .assert_tag_error(TagError::NotFound);

    active_book.assert_unchanged().await;
    assert_eq!(app.trash(&owner).await.len(), 1);
    assert_eq!(app.tag_tree(&owner).await, [owner_tag.node(vec![])]);
    app.assert_book_tags(&owner, &active_book.data.id, &[owner_tag.binding(None)])
        .await;
}
