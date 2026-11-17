mod support;
use DeleteStrategy::*;
use HttpError::*;
use SampleFile::*;
use support::*;

// 验证创建 PDF 和 EPUB 书籍后可列出书籍并按 ID 筛选。
#[tokio::test]
async fn create_and_filter_books() {
    let app = TestApp::new().await;
    let reader = app.session("读者A").await;
    assert!(app.books(&reader).await.is_empty());
    app.upload(&reader, Pdf).await;
    app.upload(&reader, Epub).await;
    let pdf = app.add_book(&reader, "PDF", Pdf).await;
    let epub = app.add_book(&reader, "EPUB", Epub).await;
    assert_eq!(app.books(&reader).await.len(), 2);
    assert_eq!(
        app.books_by_ids(&reader, &[&pdf.data.id]).await,
        [pdf.data.clone()]
    );
    assert_eq!(
        app.books_by_ids(&reader, &[&pdf.data.id, &epub.data.id])
            .await
            .len(),
        2
    );
    assert!(app.books_by_ids(&reader, &[UNKNOWN_ID]).await.is_empty());
    app.find_books(&reader, &[INVALID_ID])
        .await
        .assert_error(InvalidBookId);
}

// 验证创建书籍时拒绝缺失或非法的标题、非法文件哈希及不存在的文件。
#[tokio::test]
async fn create_rejects_invalid_title_and_file() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.create_book(&reader, "", UNKNOWN_HASH)
        .await
        .assert_error(MissingTitle);
    for title in INVALID_TITLES {
        app.create_book(&reader, title, UNKNOWN_HASH)
            .await
            .assert_error(InvalidTitle);
    }
    app.create_book(&reader, "Book", INVALID_HASH)
        .await
        .assert_error(InvalidHash);
    app.create_book(&reader, "Book", UNKNOWN_HASH)
        .await
        .assert_error(FileNotFound);
    assert!(app.books(&reader).await.is_empty());
}

// 验证重复标题和非法重命名被拒绝，且现有书籍保持不变。
#[tokio::test]
async fn duplicate_titles_and_invalid_renames_leave_books_unchanged() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let first = app.add_book(&reader, "First", Epub).await;
    let second = app.add_book(&reader, "Second", Epub).await;
    app.create_book(&reader, "First", &Epub.hash())
        .await
        .assert_error(TitleConflict);
    for (title, error) in [
        ("", MissingTitle),
        ("First", UnchangedTitle),
        ("Second", TitleConflict),
    ] {
        first.rename(title).await.assert_error(error);
    }
    for title in INVALID_TITLES {
        first.rename(title).await.assert_error(InvalidTitle);
    }
    first.assert_unchanged().await;
    second.assert_unchanged().await;
}

// 验证书籍可重命名和删除，重复删除报错且共享文件仍可下载。
#[tokio::test]
async fn rename_and_delete_preserve_shared_file() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Original", Epub).await;
    book.rename("Renamed").await.assert_empty(204);
    book.assert_renamed("Renamed").await;
    book.delete().await.assert_empty(204);
    assert!(app.books(&reader).await.is_empty());
    book.delete().await.assert_error(BookAlreadyDeleted);
    app.find_file(&reader, &Epub.hash()).await.assert_empty(204);
    app.download(&reader, &Epub.hash()).await.assert_file(&Epub);
}

// 删除书籍后，普通列表隐藏书籍和标签绑定，恢复时重建仍存在的标签关系。
#[tokio::test]
async fn trash_restore_preserves_existing_tag_bindings() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Tagged", Epub).await;
    let tag = app.add_tag(&reader, "Genre", None, false, None).await;
    app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_empty(204);

    book.delete().await.assert_empty(204);
    assert!(app.books(&reader).await.is_empty());
    let trash = app.trash(&reader).await;
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].book.id, book.data.id);
    book.cover().await.assert_png();
    app.query_book_tags(&reader, &book.data.id).await.assert_error(BookNotFound);

    app.restore_book(&reader, &book.data.id).await.assert_empty(204);
    app.restore_book(&reader, &book.data.id).await.assert_error(BookNotDeleted);
    let restored = app.books_by_ids(&reader, &[&book.data.id]).await;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].title, book.data.title);
    assert_eq!(restored[0].hash, book.data.hash);
    app.assert_book_tags(&reader, &book.data.id, &[tag.binding(None)]).await;
    assert!(app.trash(&reader).await.is_empty());
}

// 恢复时跳过已经删除的标签定义。
#[tokio::test]
async fn restoring_book_skips_tag_definitions_that_were_deleted() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Tagged", Epub).await;
    let tag = app.add_tag(&reader, "Genre", None, false, None).await;
    app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_empty(204);
    book.delete().await.assert_empty(204);
    app.delete_tag(&reader, &tag.id, Cascade).await.assert_empty(204);

    app.restore_book(&reader, &book.data.id).await.assert_empty(204);
    assert!(app.book_tags(&reader, &book.data.id).await.is_empty());
}

#[tokio::test]
async fn restoring_book_renames_it_when_its_original_title_is_in_use() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let deleted = app.add_book(&reader, "Shared title", Epub).await;
    deleted.delete().await.assert_empty(204);
    app.add_book(&reader, "Shared title", Epub).await;
    app.add_book(&reader, "Shared title (1)", Epub).await;
    app.add_book(&reader, "Shared title (2)", Epub).await;

    app.restore_book(&reader, &deleted.data.id).await.assert_empty(204);
    let restored = app.books_by_ids(&reader, &[&deleted.data.id]).await;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].title, "Shared title (3)");
}

// 批量移入回收站和彻底删除均为原子操作。
#[tokio::test]
async fn batch_trash_and_permanent_delete_are_atomic() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let first = app.add_book(&reader, "First", Epub).await;
    let second = app.add_book(&reader, "Second", Epub).await;

    app.delete_books(&reader, &[&first.data.id, UNKNOWN_ID]).await.assert_error(BookNotFound);
    assert_eq!(app.books(&reader).await.len(), 2);

    app.delete_books(&reader, &[&first.data.id, &second.data.id]).await.assert_empty(204);
    assert!(app.books(&reader).await.is_empty());
    assert_eq!(app.trash(&reader).await.len(), 2);
    app.permanently_delete_books(&reader, &[&first.data.id, UNKNOWN_ID]).await.assert_error(BookNotFound);
    assert_eq!(app.trash(&reader).await.len(), 2);
    app.permanently_delete_books(&reader, &[&first.data.id, &second.data.id]).await.assert_empty(204);
    assert!(app.trash(&reader).await.is_empty());
}

// 验证重命名、删除和获取封面时拒绝非法或不存在的书籍 ID。
#[tokio::test]
async fn unknown_and_invalid_book_ids_are_rejected() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (id, error) in [(INVALID_ID, InvalidBookId), (UNKNOWN_ID, BookNotFound)] {
        app.rename_book(&reader, id, "NewTitle")
            .await
            .assert_error(error);
        app.delete_book(&reader, id).await.assert_error(error);
        app.cover(&reader, id).await.assert_error(error);
    }
}

// 验证 PDF 书籍封面以有效的 PNG 图片返回。
#[tokio::test]
async fn pdf_cover_is_png() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Pdf).await;
    let book = app.add_book(&reader, "PDF", Pdf).await;
    book.cover().await.assert_png();
}

// 验证 EPUB 封面格式无法识别时返回空白 PNG 封面。
#[tokio::test]
async fn epub_cover_with_unknown_format_falls_back_to_blank_png() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "EPUB", Epub).await;
    book.cover().await.assert_blank_cover();
}

// 验证 EPUB 封面按实际字节识别格式并在重启后保留原始内容
#[tokio::test]
async fn epub_cover_preserves_actual_format_and_bytes_after_restart() {
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;

    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
        let directory = tempfile::tempdir().unwrap();
        let app = TestApp::at(directory.path()).await;
        let reader = app.session("Reader").await;
        let mut cover = Vec::new();
        DynamicImage::new_rgb8(3, 5)
            .write_to(&mut Cursor::new(&mut cover), format)
            .unwrap();
        // The EPUB still declares an SVG: the actual image bytes determine the format.
        let epub = epub_with_cover(Some(&cover));
        let hash = blake3::hash(&epub).to_hex().to_string();
        app.upload_parts(Some(&reader), &[&hash], Some(&epub))
            .await
            .assert_empty(201);
        let created = app.create_book(&reader, "EPUB", &hash).await;
        created.assert_status(201);
        let id = app.books(&reader).await.pop().unwrap().id;

        let response = app.cover(&reader, &id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
        let path = directory
            .path()
            .join("books/covers")
            .join(&hash)
            .with_extension(format.extensions_str()[0]);
        assert_eq!(std::fs::read(&path).unwrap(), cover);
        drop(app);

        let app = TestApp::at(directory.path()).await;
        let response = app.cover(&reader, &id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
    }
}

// 验证 EPUB 封面缺失、为空或无法识别时，重启前后均返回空白 PNG。
#[tokio::test]
async fn missing_or_unrecognized_epub_cover_uses_blank_png_after_restart() {
    for cover in [
        None,
        Some(b"unrecognized image".as_slice()),
        Some(b"".as_slice()),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let app = TestApp::at(directory.path()).await;
        let reader = app.session("Reader").await;
        let epub = epub_with_cover(cover);
        let hash = blake3::hash(&epub).to_hex().to_string();
        app.upload_parts(Some(&reader), &[&hash], Some(&epub))
            .await
            .assert_empty(201);
        let created = app.create_book(&reader, "EPUB", &hash).await;
        created.assert_status(201);
        let id = app.books(&reader).await.pop().unwrap().id;
        app.cover(&reader, &id).await.assert_blank_cover();
        drop(app);

        let app = TestApp::at(directory.path()).await;
        app.cover(&reader, &id).await.assert_blank_cover();
    }
}
