mod support;
use SampleFile::*;
use support::*;

// 验证注册不同用户名会创建身份独立且书架为空的用户。
#[tokio::test]
async fn registration_creates_distinct_users_with_empty_bookshelves() {
    let app = TestApp::new().await;
    let first = app.session("读者１２３").await;
    let second = app.session("Reader123").await;
    assert_ne!(first.user_id, second.user_id);
    assert!(app.books(&first).await.is_empty());
    assert!(app.books(&second).await.is_empty());
}

// 验证注册时拒绝空用户名和非法格式，接受合法用户名。
#[tokio::test]
async fn registration_rejects_invalid_names() {
    let app = TestApp::new().await;
    app.register("")
        .await
        .assert_error(UserError::MissingUserName);
    for name in [
        " Reader",
        "Reader ",
        "User Name",
        "User_1",
        "User-1",
        "😀",
        "A\n",
        "A\t",
    ] {
        app.register(name)
            .await
            .assert_error(UserError::InvalidUserNameFormat);
    }
    app.register("Valid").await.assert_ok();
}

// 验证重复注册被拒绝，且现有用户的登录和书籍不受影响。
#[tokio::test]
async fn duplicate_registration_preserves_existing_login_and_books() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Existing", Epub).await;
    app.register("Reader")
        .await
        .assert_error(UserError::NameConflict);
    assert_eq!(app.login("Reader", PASSWORD).await.user_id, reader.user_id);
    book.assert_unchanged().await;
}

// 验证修改用户名后需使用新名称登录，且用户身份和书籍保持不变。
#[tokio::test]
async fn rename_changes_login_name_but_preserves_identity_and_books() {
    let app = TestApp::new().await;
    let original = app.session("Original").await;
    app.upload(&original, Epub).await;
    let book = app.add_book(&original, "Existing", Epub).await;
    app.rename_user(&original.user_id, "新名字")
        .await
        .assert_ok();
    app.attempt_login("Original", PASSWORD)
        .await
        .assert_error(HttpError::InvalidCredentials);
    let renamed = app.login("新名字", PASSWORD).await;
    assert_eq!(renamed.user_id, original.user_id);
    assert_eq!(app.books(&renamed).await, std::slice::from_ref(&book.data));
    book.assert_unchanged().await;
}

// 验证修改密码后旧密码失效、新密码可重复设置，且用户身份和书籍保持不变。
#[tokio::test]
async fn password_change_rejects_old_password_and_preserves_identity() {
    let app = TestApp::new().await;
    let original = app.session("Reader").await;
    app.upload(&original, Epub).await;
    let book = app.add_book(&original, "Existing", Epub).await;
    app.change_password(&original.user_id, "new-password")
        .await
        .assert_ok();
    app.attempt_login("Reader", PASSWORD)
        .await
        .assert_error(HttpError::InvalidCredentials);
    let changed = app.login("Reader", "new-password").await;
    assert_eq!(changed.user_id, original.user_id);
    assert_eq!(app.books(&changed).await, std::slice::from_ref(&book.data));
    app.change_password(&original.user_id, "new-password")
        .await
        .assert_ok();
    app.login("Reader", "new-password").await;
}

// 验证非法、未变化或冲突的用户名修改被拒绝，且原用户名仍可登录。
#[tokio::test]
async fn invalid_renames_preserve_current_name() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.register("Existing").await.assert_ok();
    for (name, error) in [
        ("", UserError::MissingUserName),
        ("Bad Name", UserError::InvalidUserNameFormat),
        ("Reader", UserError::NameUnchanged),
        ("Existing", UserError::NameConflict),
    ] {
        app.rename_user(&reader.user_id, name)
            .await
            .assert_error(error);
        assert_eq!(app.login("Reader", PASSWORD).await.user_id, reader.user_id);
    }
}

// 验证修改用户名、修改密码和注销时拒绝非法或不存在的用户 ID。
#[tokio::test]
async fn mutations_reject_invalid_and_unknown_users() {
    let app = TestApp::new().await;
    for id in [INVALID_ID, UNKNOWN_ID] {
        let expected = || {
            if id == INVALID_ID {
                UserError::InvalidId
            } else {
                UserError::NotFound
            }
        };
        app.rename_user(id, "Valid").await.assert_error(expected());
        app.change_password(id, "new-password")
            .await
            .assert_error(expected());
        app.unregister(id).await.assert_error(expected());
    }
}

// 验证注销账户后登录、刷新和现有访问均失效，其他用户不受影响且重复注销报错。
#[tokio::test]
async fn unregister_invalidates_login_refresh_and_existing_access() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let other = app.session("Other").await;
    app.unregister(&reader.user_id).await.assert_ok();
    app.attempt_login("Reader", PASSWORD)
        .await
        .assert_error(HttpError::InvalidCredentials);
    app.attempt_refresh(&reader)
        .await
        .assert_error(HttpError::InvalidToken);
    app.access_books(&reader)
        .await
        .assert_error(HttpError::InvalidToken);
    app.access_books(&other).await.assert_status(200);
    app.unregister(&reader.user_id)
        .await
        .assert_error(UserError::NotFound);
}
