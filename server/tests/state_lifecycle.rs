mod support;
use support::*;

#[tokio::test]
async fn restart_preserves_users_independent_books_files_and_existing_sessions() {
    let restart = Restart::new();
    restart.seed_users_and_books().await;
    restart.assert_users_books_and_tokens_restored().await;
}

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
