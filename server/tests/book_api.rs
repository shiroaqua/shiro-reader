mod support;
use support::*;
use HttpError::*;
use SampleFile::*;

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
    assert_eq!(app.books_by_ids(&reader, &[&pdf.data.id]).await, [pdf.data.clone()]);
    assert_eq!(app.books_by_ids(&reader, &[&pdf.data.id, &epub.data.id]).await.len(), 2);
    assert!(app.books_by_ids(&reader, &[UNKNOWN_ID]).await.is_empty());
    app.find_books(&reader, &[INVALID_ID]).await.assert_error(InvalidBookId);
}

#[tokio::test]
async fn create_rejects_invalid_title_and_file() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.create_book(&reader, "", UNKNOWN_HASH).await.assert_error(MissingTitle);
    for title in INVALID_TITLES {
        app.create_book(&reader, title, UNKNOWN_HASH).await.assert_error(InvalidTitle);
    }
    app.create_book(&reader, "Book", INVALID_HASH).await.assert_error(InvalidHash);
    app.create_book(&reader, "Book", UNKNOWN_HASH).await.assert_error(FileNotFound);
    assert!(app.books(&reader).await.is_empty());
}

#[tokio::test]
async fn duplicate_titles_and_invalid_renames_leave_books_unchanged() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let first = app.add_book(&reader, "First", Epub).await;
    let second = app.add_book(&reader, "Second", Epub).await;
    app.create_book(&reader, "First", &Epub.hash()).await.assert_error(TitleConflict);
    for (title, error) in [("", MissingTitle), ("First", UnchangedTitle), ("Second", TitleConflict)] {
        first.rename(title).await.assert_error(error);
    }
    for title in INVALID_TITLES { first.rename(title).await.assert_error(InvalidTitle); }
    first.assert_unchanged().await;
    second.assert_unchanged().await;
}

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
    book.delete().await.assert_error(BookNotFound);
    app.find_file(&reader, &Epub.hash()).await.assert_empty(204);
    app.download(&reader, &Epub.hash()).await.assert_file(&Epub);
}

#[tokio::test]
async fn unknown_and_invalid_book_ids_are_rejected() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (id, error) in [(INVALID_ID, InvalidBookId), (UNKNOWN_ID, BookNotFound)] {
        app.rename_book(&reader, id, "NewTitle").await.assert_error(error);
        app.delete_book(&reader, id).await.assert_error(error);
        app.cover(&reader, id).await.assert_error(error);
    }
}

#[tokio::test]
async fn pdf_cover_is_png() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Pdf).await;
    let book = app.add_book(&reader, "PDF", Pdf).await;
    book.cover().await.assert_png();
}

#[tokio::test]
async fn epub_cover_with_unknown_format_falls_back_to_blank_png() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "EPUB", Epub).await;
    book.cover().await.assert_blank_cover();
}

#[tokio::test]
async fn epub_cover_preserves_actual_format_and_bytes_after_restart() {
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;

    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
        let directory = tempfile::tempdir().unwrap();
        let app = TestApp::at(directory.path()).await;
        let reader = app.session("Reader").await;
        let mut cover = Vec::new();
        DynamicImage::new_rgb8(3, 5).write_to(&mut Cursor::new(&mut cover), format).unwrap();
        // The EPUB still declares an SVG: the actual image bytes determine the format.
        let epub = epub_with_cover(Some(&cover));
        let hash = blake3::hash(&epub).to_hex().to_string();
        app.upload_parts(Some(&reader), &[&hash], Some(&epub)).await.assert_empty(201);
        let created = app.create_book(&reader, "EPUB", &hash).await;
        created.assert_status(201);
        let id = app.books(&reader).await.pop().unwrap().id;

        let response = app.cover(&reader, &id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
        let path = directory.path().join("books/covers").join(&hash)
            .with_extension(format.extensions_str()[0]);
        assert_eq!(std::fs::read(&path).unwrap(), cover);
        drop(app);

        if format == ImageFormat::Jpeg {
            // Older versions stored JPEG bytes under a PNG extension.
            std::fs::rename(&path, path.with_extension("png")).unwrap();
        }
        let app = TestApp::at(directory.path()).await;
        let response = app.cover(&reader, &id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
    }
}

#[tokio::test]
async fn missing_or_unrecognized_epub_cover_uses_blank_png_after_restart() {
    for cover in [None, Some(b"unrecognized image".as_slice()), Some(b"".as_slice())] {
        let directory = tempfile::tempdir().unwrap();
        let app = TestApp::at(directory.path()).await;
        let reader = app.session("Reader").await;
        let epub = epub_with_cover(cover);
        let hash = blake3::hash(&epub).to_hex().to_string();
        app.upload_parts(Some(&reader), &[&hash], Some(&epub)).await.assert_empty(201);
        let created = app.create_book(&reader, "EPUB", &hash).await;
        created.assert_status(201);
        let id = app.books(&reader).await.pop().unwrap().id;
        app.cover(&reader, &id).await.assert_blank_cover();
        drop(app);

        let app = TestApp::at(directory.path()).await;
        app.cover(&reader, &id).await.assert_blank_cover();
    }
}

#[tokio::test]
async fn legacy_unrecognized_cover_and_missing_cover_use_blank_png() {
    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let id = app.add_book(&reader, "EPUB", Epub).await.data.id;
    let path = directory.path().join("books/covers").join(Epub.hash()).with_extension("png");
    std::fs::write(&path, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
    drop(app);

    let app = TestApp::at(directory.path()).await;
    app.cover(&reader, &id).await.assert_blank_cover();
    std::fs::remove_file(path).unwrap();
    drop(app);

    let app = TestApp::at(directory.path()).await;
    app.cover(&reader, &id).await.assert_blank_cover();
}
