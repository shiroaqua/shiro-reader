mod support;

use SampleFile::{Epub, Pdf};
use HttpError::*;
use shiro_application::library::book::dto::ChapterOutput;
use shiro_domain::library::book::chapter::ChapterLocation;
use support::*;

#[tokio::test]
async fn chapters_support_pdf_and_epub_locations_and_ordered_crud() {
    let app = TestApp::new().await;
    let reader = app.session("ChapterReader").await;
    app.upload(&reader, Pdf).await;
    app.upload(&reader, Epub).await;
    let pdf = app.add_book(&reader, "PDF book", Pdf).await;
    let epub = app.add_book(&reader, "EPUB book", Epub).await;

    let pdf_defaults: Vec<ChapterOutput> = app
        .chapters(&reader, &pdf.data.id)
        .await
        .json_as();
    assert_eq!(
        pdf_defaults
            .iter()
            .map(|chapter| chapter.position)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
    let epub_defaults: Vec<ChapterOutput> = app
        .chapters(&reader, &epub.data.id)
        .await
        .json_as();
    assert_eq!(
        epub_defaults
            .iter()
            .map(|chapter| chapter.position)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );

    let pdf_chapter = app
        .create_chapter(
            &reader,
            &pdf.data.id,
            7,
            "Later section",
            serde_json::json!({
                "format": "pdf",
                "start_page": 12,
                "end_page": 17,
            }),
        )
        .await;
    pdf_chapter.assert_status(201);
    let pdf_chapter: ChapterOutput = pdf_chapter.json_as();
    assert_eq!(pdf_chapter.book_id, pdf.data.id);
    assert_eq!(pdf_chapter.position, 7);
    assert_eq!(
        pdf_chapter.location,
        ChapterLocation::Pdf {
            start_page: 12,
            end_page: 17,
        }
    );

    let earlier_chapter = app
        .create_chapter(
            &reader,
            &pdf.data.id,
            6,
            "Earlier section",
            serde_json::json!({
                "format": "pdf",
                "start_page": 1,
                "end_page": 11,
            }),
        )
        .await;
    earlier_chapter.assert_status(201);
    let earlier_chapter: ChapterOutput = earlier_chapter.json_as();

    let epub_chapter = app
        .create_chapter(
            &reader,
            &epub.data.id,
            6,
            "EPUB section",
            serde_json::json!({
                "format": "epub",
                "href": "Text/chapter-1.xhtml",
                "cfi": "epubcfi(/6/4[chapter1]!/4/2)",
            }),
        )
        .await;
    epub_chapter.assert_status(201);
    let epub_chapter: ChapterOutput = epub_chapter.json_as();
    assert_eq!(
        epub_chapter.location,
        ChapterLocation::Epub {
            href: "Text/chapter-1.xhtml".to_owned(),
            cfi: Some("epubcfi(/6/4[chapter1]!/4/2)".to_owned()),
        }
    );

    app.create_chapter(
        &reader,
        &pdf.data.id,
        6,
        "Duplicate order",
        serde_json::json!({
            "format": "pdf",
            "start_page": 18,
            "end_page": 20,
        }),
    )
    .await
    .assert_error(PositionConflict);

    let pdf_chapters: Vec<ChapterOutput> = app
        .chapters(&reader, &pdf.data.id)
        .await
        .json_as();
    assert_eq!(
        pdf_chapters
            .iter()
            .map(|chapter| chapter.position)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6, 7]
    );

    app.delete_chapter(&reader, &pdf.data.id, &earlier_chapter.id)
        .await
        .assert_empty(204);
    let pdf_chapters: Vec<ChapterOutput> = app
        .chapters(&reader, &pdf.data.id)
        .await
        .json_as();
    let mut expected_pdf_chapters = pdf_defaults;
    expected_pdf_chapters.push(pdf_chapter);
    assert_eq!(pdf_chapters, expected_pdf_chapters);

    let epub_chapters: Vec<ChapterOutput> = app
        .chapters(&reader, &epub.data.id)
        .await
        .json_as();
    let mut expected_epub_chapters = epub_defaults;
    expected_epub_chapters.push(epub_chapter);
    assert_eq!(epub_chapters, expected_epub_chapters);
}

#[tokio::test]
async fn chapters_reject_invalid_locations_and_report_missing_books_or_chapters() {
    let app = TestApp::new().await;
    let reader = app.session("ChapterReader").await;
    app.upload(&reader, Pdf).await;

    app.create_chapter(
        &reader,
        UNKNOWN_ID,
        0,
        "Chapter",
        serde_json::json!({
            "format": "epub",
            "href": "Text/chapter.xhtml",
        }),
    )
    .await
    .assert_error(BookNotFound);
    app.create_chapter(
        &reader,
        "invalid-id",
        0,
        "Chapter",
        serde_json::json!({
            "format": "epub",
            "href": "Text/chapter.xhtml",
        }),
    )
    .await
    .assert_error(InvalidBookId);

    let pdf = app.add_book(&reader, "PDF book", Pdf).await;
    app.create_chapter(
        &reader,
        &pdf.data.id,
        0,
        "Chapter",
        serde_json::json!({
            "format": "pdf",
            "start_page": 0,
            "end_page": 4,
        }),
    )
    .await
    .assert_error(InvalidChapterLocation);
    app.create_chapter(
        &reader,
        &pdf.data.id,
        0,
        " ",
        serde_json::json!({
            "format": "pdf",
            "start_page": 1,
            "end_page": 4,
        }),
    )
    .await
    .assert_error(InvalidChapterTitle);
    app.create_chapter(
        &reader,
        &pdf.data.id,
        0,
        "Chapter",
        serde_json::json!({
            "format": "epub",
            "href": " ",
        }),
    )
    .await
    .assert_error(InvalidChapterLocation);

    app.delete_chapter(&reader, &pdf.data.id, UNKNOWN_ID)
        .await
        .assert_error(BookNotFound);
    app.chapters(&reader, UNKNOWN_ID)
        .await
        .assert_error(BookNotFound);
}
