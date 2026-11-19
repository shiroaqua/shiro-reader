mod support;
use support::*;

// 验证重启后保留用户、各自的书架和共享文件，且原有会话仍可访问和刷新。
#[tokio::test]
async fn restart_preserves_users_independent_books_files_and_existing_sessions() {
    let root = tempfile::tempdir().unwrap();
    let app = TestApp::at(root.path()).await;
    let alice = app.session("Alice").await;
    let bob = app.session("Bob").await;
    app.upload(&alice, SampleFile::Epub).await;
    let alice_book = app.add_book(&alice, "Abook", SampleFile::Epub).await.data;
    let bob_book = app.add_book(&bob, "Bbook", SampleFile::Epub).await.data;
    drop(app);

    let app = TestApp::at(root.path()).await;
    for (name, session, book) in [
        ("Alice", alice, alice_book),
        ("Bob", bob, bob_book),
    ] {
        let login = app.login(name, PASSWORD).await;
        assert_eq!(login.user_id, session.user_id);
        assert_eq!(app.books(&login).await, [book.clone()]);
        assert_eq!(app.books(&session).await, [book.clone()]);
        let refreshed = app.refresh(&session).await;
        assert_eq!(app.books(&refreshed).await, [book]);
        app.download(&login, &SampleFile::Epub.hash())
            .await
            .assert_file(&SampleFile::Epub);
    }
}

// 验证多个应用状态可共用 Pdfium 并保持书库独立，销毁后重建仍可生成 PDF 封面。
#[tokio::test]
async fn multiple_appstates_share_pdfium_but_keep_separate_libraries() {
    let first = TestApp::new().await;
    let second = TestApp::new().await;
    let alice = first.session("Reader").await;
    let bob = second.session("Reader").await;
    first.upload(&alice, SampleFile::Pdf).await;
    second.upload(&bob, SampleFile::Pdf).await;
    let a = first.add_book(&alice, "First", SampleFile::Pdf).await;
    let b = second.add_book(&bob, "Second", SampleFile::Pdf).await;
    a.cover().await.assert_png();
    b.cover().await.assert_png();
    assert_eq!(first.books(&alice).await, [a.data.clone()]);
    assert_eq!(second.books(&bob).await, [b.data.clone()]);
    drop(first);
    drop(second);

    let rebuilt = TestApp::new().await;
    let reader = rebuilt.session("Reader").await;
    rebuilt.upload(&reader, SampleFile::Pdf).await;
    let book = rebuilt.add_book(&reader, "Rebuilt", SampleFile::Pdf).await;
    book.cover().await.assert_png();
}
