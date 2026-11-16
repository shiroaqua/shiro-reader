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
    app.download(&bob, &Epub.hash()).await.assert_file(&Epub);
}
