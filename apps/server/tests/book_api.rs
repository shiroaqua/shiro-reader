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
        std::slice::from_ref(&pdf.data)
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
    let shared = app.add_book(&reader, "Shared", Epub).await;
    book.delete().await.assert_empty(204);
    assert_eq!(app.books(&reader).await, std::slice::from_ref(&shared.data));
    book.delete().await.assert_error(BookAlreadyDeleted);
    app.download(&reader, &book.data.id)
        .await
        .assert_error(BookAlreadyDeleted);
    app.find_file(&reader, &Epub.hash()).await.assert_empty(204);
    app.download(&reader, &shared.data.id).await.assert_file(&Epub);
}

// 删除书籍后，普通列表隐藏书籍和标签绑定，恢复时重建仍存在的标签关系。
#[tokio::test]
async fn trash_restore_preserves_existing_tag_bindings() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Tagged", Epub).await;
    let tag = app.add_tag(&reader, "Genre", None, false, None).await;
    app.bind_tag(&reader, &book.data.id, &tag.id, None)
        .await
        .assert_empty(204);

    book.delete().await.assert_empty(204);
    assert!(app.books(&reader).await.is_empty());
    let trash = app.trash(&reader).await;
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].book.id, book.data.id);
    book.cover().await.assert_png();
    app.query_book_tags(&reader, &book.data.id)
        .await
        .assert_error(BookNotFound);

    app.restore_book(&reader, &book.data.id)
        .await
        .assert_empty(204);
    app.restore_book(&reader, &book.data.id)
        .await
        .assert_error(BookNotDeleted);
    let restored = app.books_by_ids(&reader, &[&book.data.id]).await;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].title, book.data.title);
    assert_eq!(restored[0].hash, book.data.hash);
    app.assert_book_tags(&reader, &book.data.id, &[tag.binding(None)])
        .await;
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
    app.bind_tag(&reader, &book.data.id, &tag.id, None)
        .await
        .assert_empty(204);
    book.delete().await.assert_empty(204);
    app.delete_tag(&reader, &tag.id, Cascade)
        .await
        .assert_empty(204);

    app.restore_book(&reader, &book.data.id)
        .await
        .assert_empty(204);
    assert!(app.book_tags(&reader, &book.data.id).await.is_empty());
}

// 恢复书籍时若原书名已被占用，自动添加数字后缀避免冲突。
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

    app.restore_book(&reader, &deleted.data.id)
        .await
        .assert_empty(204);
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

    app.delete_books(&reader, &[&first.data.id, UNKNOWN_ID])
        .await
        .assert_error(BookNotFound);
    assert_eq!(app.books(&reader).await.len(), 2);

    app.delete_books(&reader, &[&first.data.id, &second.data.id])
        .await
        .assert_empty(204);
    assert!(app.books(&reader).await.is_empty());
    assert_eq!(app.trash(&reader).await.len(), 2);
    app.permanently_delete_books(&reader, &[&first.data.id, UNKNOWN_ID])
        .await
        .assert_error(BookNotFound);
    assert_eq!(app.trash(&reader).await.len(), 2);
    app.permanently_delete_books(&reader, &[&first.data.id, &second.data.id])
        .await
        .assert_empty(204);
    assert!(app.trash(&reader).await.is_empty());
}

// 验证重命名、删除、恢复、彻底删除和获取封面时拒绝非法或不存在的书籍 ID。
#[tokio::test]
async fn unknown_and_invalid_book_ids_are_rejected() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (id, error) in [(INVALID_ID, InvalidBookId), (UNKNOWN_ID, BookNotFound)] {
        app.rename_book(&reader, id, "NewTitle")
            .await
            .assert_error(error);
        app.delete_book(&reader, id).await.assert_error(error);
        app.restore_book(&reader, id).await.assert_error(error);
        app.permanently_delete_book(&reader, id)
            .await
            .assert_error(error);
        app.cover(&reader, id).await.assert_error(error);
    }
}

// 验证 PDF 书籍封面以有效的非空白 PNG 图片返回。
#[tokio::test]
async fn pdf_cover_is_png() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Pdf).await;
    let book = app.add_book(&reader, "PDF", Pdf).await;
    book.cover().await.assert_rendered_png();
}

// 验证回收站重命名、活跃书彻底删除拦截、单本彻底删除及空列表/重复 ID 批处理的原子性。
#[tokio::test]
async fn trash_and_batch_edge_cases() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;

    let active = app.add_book(&reader, "Active", Epub).await;
    let trashed = app.add_book(&reader, "Trashed", Epub).await;
    trashed.delete().await.assert_empty(204);

    // 回收站中的书籍不可重命名。
    trashed
        .rename("RenamedInTrash")
        .await
        .assert_error(BookAlreadyDeleted);

    // 活跃书籍不可直接彻底删除（单个或批量混合）。
    app.permanently_delete_book(&reader, &active.data.id)
        .await
        .assert_error(BookNotFound);
    app.permanently_delete_books(&reader, &[&trashed.data.id, &active.data.id])
        .await
        .assert_error(BookNotFound);
    assert_eq!(app.trash(&reader).await.len(), 1);
    active.assert_unchanged().await;

    // 批量接口传入空数组返回 204 且无副作用。
    app.delete_books(&reader, &[]).await.assert_empty(204);
    app.permanently_delete_books(&reader, &[])
        .await
        .assert_empty(204);

    // 批量删除或彻底删除传入重复 ID 时触发原子回滚。
    app.delete_books(&reader, &[&active.data.id, &active.data.id])
        .await
        .assert_error(BookAlreadyDeleted);
    assert_eq!(app.books(&reader).await, std::slice::from_ref(&active.data));

    app.permanently_delete_books(&reader, &[&trashed.data.id, &trashed.data.id])
        .await
        .assert_error(BookNotFound);
    assert_eq!(app.trash(&reader).await.len(), 1);

    // 单本彻底删除成功后回收站清空，但底层共享文件仍然保留。
    app.permanently_delete_book(&reader, &trashed.data.id)
        .await
        .assert_empty(204);
    assert!(app.trash(&reader).await.is_empty());
    app.find_file(&reader, &Epub.hash()).await.assert_empty(204);
}

// 验证 EPUB 封面按实际字节识别格式并在重启后保留原始内容。
#[tokio::test]
async fn epub_cover_preserves_actual_format_and_bytes_after_restart() {
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;

    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let reader = app.session("Reader").await;
    let mut books = Vec::new();

    for (index, format) in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP]
        .into_iter()
        .enumerate()
    {
        let mut cover = Vec::new();
        DynamicImage::new_rgb8(3, 5)
            .write_to(&mut Cursor::new(&mut cover), format)
            .unwrap();
        // The EPUB still declares an SVG: the actual image bytes determine the format.
        let epub = epub_with_cover(Some(&cover));
        let book = app
            .upload_and_add_book(&reader, &format!("EPUB-{index}"), &epub)
            .await;

        let response = app.cover(&reader, &book.id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
        let path = directory
            .path()
            .join("books/covers")
            .join(&book.hash)
            .with_extension(format.extensions_str()[0]);
        assert_eq!(std::fs::read(&path).unwrap(), cover);
        books.push((book.id, format, cover));
    }
    drop(app);

    let app = TestApp::at(directory.path()).await;
    for (id, format, cover) in books {
        let response = app.cover(&reader, &id).await;
        response.assert_image(format);
        response.assert_cover_bytes(&cover);
    }
}

// 验证 EPUB 封面缺失、为空或无法识别（含 SVG）时，重启前后均返回空白 PNG。
#[tokio::test]
async fn missing_or_unrecognized_epub_cover_uses_blank_png_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let reader = app.session("Reader").await;
    let samples = [
        Epub.bytes(),
        epub_with_cover(None),
        epub_with_cover(Some(b"unrecognized image")),
        epub_with_cover(Some(b"")),
    ];
    let mut ids = Vec::new();

    for (index, epub) in samples.iter().enumerate() {
        let book = app
            .upload_and_add_book(&reader, &format!("EPUB-{index}"), epub)
            .await;
        app.cover(&reader, &book.id).await.assert_blank_cover();
        ids.push(book.id);
    }
    drop(app);

    let app = TestApp::at(directory.path()).await;
    for id in ids {
        app.cover(&reader, &id).await.assert_blank_cover();
    }
}

fn build_custom_navigation_epub() -> Vec<u8> {
    use std::io::{Cursor, Write};

    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    output.start_file("mimetype", stored).unwrap();
    output.write_all(b"application/epub+zip").unwrap();

    output.start_file("META-INF/container.xml", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

    output.start_file("EPUB/package.opf", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:11111111-1111-1111-1111-111111111111</dc:identifier>
    <dc:title>Custom Nav</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="cover" linear="no"/>
    <itemref idref="ch1" linear="yes"/>
    <itemref idref="ch2" linear="yes"/>
  </spine>
</package>"#,
        )
        .unwrap();

    output.start_file("EPUB/nav.xhtml", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>TOC</title></head>
<body>
<nav epub:type="toc" id="toc">
  <ol>
    <li>
      <span>Part 1</span>
      <ol>
        <li>
          <a href="chapter1.xhtml">Chapter 1</a>
          <ol>
            <li><a href="chapter1.xhtml#section-2">Section 1.2</a></li>
          </ol>
        </li>
      </ol>
    </li>
    <li><a href="chapter2.xhtml">Chapter 2</a></li>
    <li><a href="https://example.com/docs">External Site</a></li>
  </ol>
</nav>
</body>
</html>"#,
        )
        .unwrap();

    output.start_file("EPUB/cover.xhtml", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Cover page text</p></body></html>"#,
        )
        .unwrap();

    output.start_file("EPUB/chapter1.xhtml", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 1</title><style>p { color: black; }</style></head>
<body>
  <h1 id="top">Opening Title</h1>
  <p>First paragraph of chapter one with enough text to advance offset meaningfully.</p>
  <h2 id="section-2">Second Section</h2>
  <p>Trailing paragraph in section two.</p>
</body>
</html>"#,
        )
        .unwrap();

    output.start_file("EPUB/chapter2.xhtml", stored).unwrap();
    output
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><h1>Chapter Two</h1><p>Second chapter body text.</p></body>
</html>"#,
        )
        .unwrap();

    output.finish().unwrap().into_inner()
}

#[tokio::test]
async fn book_navigation_returns_epub_and_pdf_chapters_and_enforces_contracts() {
    use shiro_reader_server::application::library::book::dto::{
        BookNavigationOutput, EpubNavTargetDto, NavigationLocatorDto, PdfDestDto,
    };

    let app = TestApp::new().await;
    let reader = app.session("NavReader").await;
    let other = app.session("OtherReader").await;

    app.upload(&reader, Epub).await;
    app.upload(&reader, Pdf).await;
    let epub = app.add_book(&reader, "Sample EPUB", Epub).await;
    let pdf = app.add_book(&reader, "Sample PDF", Pdf).await;

    // 1. Sample EPUB navigation
    let epub_resp = app.book_navigation(Some(&reader), &epub.data.id).await;
    epub_resp.assert_status(200);
    let epub_nav: BookNavigationOutput = epub_resp.json_as();
    assert_eq!(epub_nav.toc.len(), 6);
    assert!(!epub_nav.linear_units.is_empty());
    assert!(epub_nav.total_weight > 0);

    let mut prev_prog = -1.0f64;
    for (i, node) in epub_nav.toc.iter().enumerate() {
        assert_eq!(node.depth, 0);
        assert_eq!(node.parent, None);
        assert_eq!(node.title, "sample");
        let prog = node.start_progression.expect("epub chapter progression");
        assert!(prog > prev_prog, "expected progression to increase at {i}");
        prev_prog = prog;
        let expected_href = format!("EPUB/chapter{}.xhtml", i + 1);
        assert!(matches!(
            &node.locator,
            Some(NavigationLocatorDto::Epub {
                spine_index: Some(_),
                href,
                target: EpubNavTargetDto::Start,
            }) if href == &expected_href
        ));
    }

    // 2. Sample PDF navigation (8 pages, 6 bookmarks pointing to 0-based pages 2..=7 with FitPage)
    let pdf_resp = app.book_navigation(Some(&reader), &pdf.data.id).await;
    pdf_resp.assert_status(200);
    let pdf_nav: BookNavigationOutput = pdf_resp.json_as();
    assert_eq!(pdf_nav.linear_units.len(), 8);
    assert_eq!(pdf_nav.total_weight, 8);
    assert_eq!(pdf_nav.toc.len(), 6);
    for (i, node) in pdf_nav.toc.iter().enumerate() {
        let expected_page = (i + 2) as u32;
        assert_eq!(node.unit_index, Some(expected_page));
        assert_eq!(node.start_progression, Some(expected_page as f64 / 8.0));
        assert!(matches!(
            node.locator,
            Some(NavigationLocatorDto::Pdf {
                page_index,
                target: PdfDestDto::FitPage
            }) if page_index == expected_page
        ));
    }

    // 3. Auth, isolation, invalid/missing ID, and deleted book contracts
    app.book_navigation(None, &epub.data.id)
        .await
        .assert_error(MissingBearer);
    app.book_navigation(Some(&other), &epub.data.id)
        .await
        .assert_error(BookNotFound);
    app.book_navigation(Some(&reader), INVALID_ID)
        .await
        .assert_error(InvalidBookId);
    app.book_navigation(Some(&reader), UNKNOWN_ID)
        .await
        .assert_error(BookNotFound);

    epub.delete().await.assert_empty(204);
    app.book_navigation(Some(&reader), &epub.data.id)
        .await
        .assert_error(BookAlreadyDeleted);
}

#[tokio::test]
async fn book_navigation_supports_nested_fragments_nonlinear_units_and_failure_fallback() {
    use shiro_reader_server::application::library::book::dto::{
        BookNavigationOutput, EpubNavTargetDto, NavigationLocatorDto,
    };

    let app = TestApp::new().await;
    let reader = app.session("CustomNavReader").await;

    // 1. Custom EPUB with grouping parent, nested child, fragment offset, non-linear unit, external link
    let custom_epub = build_custom_navigation_epub();
    let custom_book = app
        .upload_and_add_book(&reader, "Custom Nav EPUB", &custom_epub)
        .await;
    let resp = app.book_navigation(Some(&reader), &custom_book.id).await;
    resp.assert_status(200);
    let nav: BookNavigationOutput = resp.json_as();

    assert_eq!(nav.linear_units.len(), 3);
    assert!(!nav.linear_units[0].is_linear);
    assert!(nav.linear_units[1].is_linear);
    assert!(nav.linear_units[2].is_linear);
    assert_eq!(
        nav.total_weight,
        nav.linear_units[1].weight + nav.linear_units[2].weight
    );
    assert_eq!(nav.toc.len(), 5);

    // [0]: grouping parent "Part 1"
    assert_eq!(nav.toc[0].title, "Part 1");
    assert_eq!(nav.toc[0].parent, None);
    assert_eq!(nav.toc[0].depth, 0);
    assert_eq!(nav.toc[0].subtree_end, 3);
    assert_eq!(nav.toc[0].locator, None);
    assert_eq!(nav.toc[0].start_progression, None);

    // [1]: "Chapter 1"
    assert_eq!(nav.toc[1].title, "Chapter 1");
    assert_eq!(nav.toc[1].parent, Some(0));
    assert_eq!(nav.toc[1].depth, 1);
    assert_eq!(nav.toc[1].subtree_end, 3);
    assert_eq!(nav.toc[1].unit_index, Some(1));
    assert_eq!(nav.toc[1].unit_offset, 0.0);

    // [2]: "Section 1.2" (#section-2)
    assert_eq!(nav.toc[2].title, "Section 1.2");
    assert_eq!(nav.toc[2].parent, Some(1));
    assert_eq!(nav.toc[2].depth, 2);
    assert_eq!(nav.toc[2].subtree_end, 3);
    assert_eq!(nav.toc[2].unit_index, Some(1));
    assert!(nav.toc[2].unit_offset > 0.0);
    assert!(nav.toc[2].start_progression > nav.toc[1].start_progression);
    assert!(matches!(
        &nav.toc[2].locator,
        Some(NavigationLocatorDto::Epub {
            spine_index: Some(1),
            href,
            target: EpubNavTargetDto::Fragment { fragment: frag },
        }) if href == "EPUB/chapter1.xhtml" && frag == "section-2"
    ));

    // [3]: "Chapter 2"
    assert_eq!(nav.toc[3].title, "Chapter 2");
    assert_eq!(nav.toc[3].parent, None);
    assert_eq!(nav.toc[3].depth, 0);
    assert_eq!(nav.toc[3].unit_index, Some(2));

    // [4]: "External Site"
    assert_eq!(nav.toc[4].title, "External Site");
    assert_eq!(nav.toc[4].flags, 2);
    assert_eq!(nav.toc[4].locator, None);

    // 2. Broken PDF that passes %PDF- magic detection on upload returns empty navigation on GET
    let broken_pdf = b"%PDF-1.4\nbroken-pdf-body";
    let broken_book = app
        .upload_and_add_book(&reader, "Broken PDF", broken_pdf)
        .await;
    let fallback_resp = app.book_navigation(Some(&reader), &broken_book.id).await;
    fallback_resp.assert_status(200);
    let fallback_nav: BookNavigationOutput = fallback_resp.json_as();
    assert_eq!(
        fallback_nav,
        BookNavigationOutput {
            toc: Vec::new(),
            linear_units: Vec::new(),
            total_weight: 0,
        }
    );
}

#[tokio::test]
async fn book_metadata_get_update_clear_and_error_contracts() {
    use shiro_reader_server::application::library::book::dto::BookMetadataOutput;

    let app = TestApp::new().await;
    let reader = app.session("MetaReader").await;
    let other = app.session("OtherMetaReader").await;

    app.upload(&reader, Epub).await;
    app.upload(&reader, Pdf).await;
    let epub = app.add_book(&reader, "Meta EPUB", Epub).await;
    let pdf = app.add_book(&reader, "Meta PDF", Pdf).await;

    // 1. GET metadata for EPUB and PDF
    let epub_resp = app.book_metadata(Some(&reader), &epub.data.id).await;
    epub_resp.assert_status(200);
    let epub_meta: BookMetadataOutput = epub_resp.json_as();
    assert_eq!(epub_meta.book_id, epub.data.id);
    assert_eq!(epub_meta.file_type, "EPUB");
    assert_eq!(epub_meta.size, SampleFile::Epub.bytes().len() as u64);
    assert_eq!(epub_meta.page_count, None);
    assert_eq!(epub_meta.created_at, epub.data.created_at.to_string());
    let initial_updated_at: jiff::Timestamp = epub_meta.updated_at.parse().unwrap();
    assert!(initial_updated_at >= epub.data.updated_at);

    let pdf_resp = app.book_metadata(Some(&reader), &pdf.data.id).await;
    pdf_resp.assert_status(200);
    let pdf_meta: BookMetadataOutput = pdf_resp.json_as();
    assert_eq!(pdf_meta.book_id, pdf.data.id);
    assert_eq!(pdf_meta.file_type, "PDF");
    assert_eq!(pdf_meta.size, SampleFile::Pdf.bytes().len() as u64);
    assert_eq!(pdf_meta.page_count, Some(8));

    // Ensure sample metadata starts clear for fields we mutate below or normalize baseline
    if epub_meta.subtitle.is_some() {
        app.clear_book_subtitle(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if !epub_meta.authors.is_empty() {
        app.clear_book_authors(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if epub_meta.publisher.is_some() {
        app.clear_book_publisher(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if epub_meta.published_date.is_some() {
        app.clear_book_published_date(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if epub_meta.language.is_some() {
        app.clear_book_language(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if epub_meta.description.is_some() {
        app.clear_book_description(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }
    if epub_meta.identifier.is_some() {
        app.clear_book_identifier(Some(&reader), &epub.data.id)
            .await
            .assert_empty(204);
    }

    // 1b. Subtitle: update, unchanged, invalid
    app.change_book_subtitle(Some(&reader), &epub.data.id, "The Annotated Edition")
        .await
        .assert_empty(204);
    app.change_book_subtitle(Some(&reader), &epub.data.id, "The Annotated Edition")
        .await
        .assert_error(SubtitleUnchanged);
    for bad in ["", " Subtitle", "Subtitle "] {
        app.change_book_subtitle(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidSubtitle);
    }

    // 1c. Authors: update, reorder, unchanged, invalid
    app.change_book_authors(Some(&reader), &epub.data.id, &["Alice", "Bob"])
        .await
        .assert_empty(204);
    app.change_book_authors(Some(&reader), &epub.data.id, &["Bob", "Alice"])
        .await
        .assert_empty(204);
    app.change_book_authors(Some(&reader), &epub.data.id, &["Bob", "Alice"])
        .await
        .assert_error(AuthorsUnchanged);
    for bad in [
        Vec::<&str>::new(),
        vec![""],
        vec![" Alice"],
        vec!["Alice", "Alice"],
    ] {
        app.change_book_authors(Some(&reader), &epub.data.id, &bad)
            .await
            .assert_error(InvalidAuthor);
    }

    // 2. Publisher: update, unchanged, invalid, clear, clear again
    app.change_book_publisher(Some(&reader), &epub.data.id, "Shiro Press")
        .await
        .assert_empty(204);
    app.change_book_publisher(Some(&reader), &epub.data.id, "Shiro Press")
        .await
        .assert_error(PublisherUnchanged);
    for bad in ["", " Shiro Press", "Shiro Press "] {
        app.change_book_publisher(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidPublisher);
    }

    // 3. Published date: Year, YearMonth, Full Date, invalid, unchanged
    app.change_book_published_date(Some(&reader), &epub.data.id, "2024")
        .await
        .assert_empty(204);
    app.change_book_published_date(Some(&reader), &epub.data.id, "2024-08")
        .await
        .assert_empty(204);
    app.change_book_published_date(Some(&reader), &epub.data.id, "2024-08-15")
        .await
        .assert_empty(204);
    app.change_book_published_date(Some(&reader), &epub.data.id, "2024-08-15")
        .await
        .assert_error(PublishedDateUnchanged);
    for bad in ["", "2024-02-31", "2024/08/15", " 2024"] {
        app.change_book_published_date(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidPublishedDate);
    }

    // 4. Language: canonicalization, invalid, unchanged
    app.change_book_language(Some(&reader), &epub.data.id, "zh-tw")
        .await
        .assert_empty(204);
    app.change_book_language(Some(&reader), &epub.data.id, "zh-TW")
        .await
        .assert_error(LanguageUnchanged);
    for bad in ["", "zh_TW", "toolongprimary", " en"] {
        app.change_book_language(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidLanguage);
    }

    // 5. Description: multiline, invalid, unchanged
    app.change_book_description(Some(&reader), &epub.data.id, "Line 1\nLine 2")
        .await
        .assert_empty(204);
    app.change_book_description(Some(&reader), &epub.data.id, "Line 1\nLine 2")
        .await
        .assert_error(DescriptionUnchanged);
    for bad in ["", " Line 1", "Line 1\0"] {
        app.change_book_description(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidDescription);
    }

    // 6. Identifier: canonicalization, invalid, unchanged
    app.change_book_identifier(Some(&reader), &epub.data.id, "978-0-306-40615-7")
        .await
        .assert_empty(204);
    app.change_book_identifier(Some(&reader), &epub.data.id, "9780306406157")
        .await
        .assert_error(IdentifierUnchanged);
    for bad in ["", "not-an-isbn", " 9780306406157"] {
        app.change_book_identifier(Some(&reader), &epub.data.id, bad)
            .await
            .assert_error(InvalidIdentifier);
    }

    // Verify persisted values via GET
    let updated: BookMetadataOutput = app
        .book_metadata(Some(&reader), &epub.data.id)
        .await
        .json_as();
    assert_eq!(updated.subtitle.as_deref(), Some("The Annotated Edition"));
    assert_eq!(updated.authors, vec!["Bob", "Alice"]);
    assert_eq!(updated.publisher.as_deref(), Some("Shiro Press"));
    assert_eq!(updated.published_date.as_deref(), Some("2024-08-15"));
    assert_eq!(updated.language.as_deref(), Some("zh-TW"));
    assert_eq!(updated.description.as_deref(), Some("Line 1\nLine 2"));
    assert_eq!(updated.identifier.as_deref(), Some("9780306406157"));
    let updated_at: jiff::Timestamp = updated.updated_at.parse().unwrap();
    assert!(updated_at >= initial_updated_at);
    assert!(updated_at > epub.data.updated_at);

    // 7. Clear all 7 fields and verify idempotency rejection
    app.clear_book_subtitle(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_subtitle(Some(&reader), &epub.data.id)
        .await
        .assert_error(SubtitleUnchanged);

    app.clear_book_authors(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_authors(Some(&reader), &epub.data.id)
        .await
        .assert_error(AuthorsUnchanged);

    app.clear_book_publisher(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_publisher(Some(&reader), &epub.data.id)
        .await
        .assert_error(PublisherUnchanged);

    app.clear_book_published_date(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_published_date(Some(&reader), &epub.data.id)
        .await
        .assert_error(PublishedDateUnchanged);

    app.clear_book_language(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_language(Some(&reader), &epub.data.id)
        .await
        .assert_error(LanguageUnchanged);

    app.clear_book_description(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_description(Some(&reader), &epub.data.id)
        .await
        .assert_error(DescriptionUnchanged);

    app.clear_book_identifier(Some(&reader), &epub.data.id)
        .await
        .assert_empty(204);
    app.clear_book_identifier(Some(&reader), &epub.data.id)
        .await
        .assert_error(IdentifierUnchanged);

    let cleared: BookMetadataOutput = app
        .book_metadata(Some(&reader), &epub.data.id)
        .await
        .json_as();
    assert_eq!(cleared.subtitle, None);
    assert!(cleared.authors.is_empty());
    assert_eq!(cleared.publisher, None);
    assert_eq!(cleared.published_date, None);
    assert_eq!(cleared.language, None);
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.identifier, None);

    // 8. Auth, cross-user isolation, invalid/missing ID, and soft-deleted book contracts
    app.book_metadata(None, &epub.data.id)
        .await
        .assert_error(MissingBearer);
    app.change_book_publisher(None, &epub.data.id, "Anon")
        .await
        .assert_error(MissingBearer);
    app.clear_book_publisher(None, &epub.data.id)
        .await
        .assert_error(MissingBearer);

    app.book_metadata(Some(&other), &epub.data.id)
        .await
        .assert_error(BookNotFound);
    app.change_book_publisher(Some(&other), &epub.data.id, "Hacked")
        .await
        .assert_error(BookNotFound);
    app.clear_book_identifier(Some(&other), &epub.data.id)
        .await
        .assert_error(BookNotFound);

    app.book_metadata(Some(&reader), INVALID_ID)
        .await
        .assert_error(InvalidBookId);
    app.book_metadata(Some(&reader), UNKNOWN_ID)
        .await
        .assert_error(BookNotFound);

    epub.delete().await.assert_empty(204);
    app.book_metadata(Some(&reader), &epub.data.id)
        .await
        .assert_error(BookAlreadyDeleted);
    app.change_book_publisher(Some(&reader), &epub.data.id, "Deleted")
        .await
        .assert_error(BookAlreadyDeleted);
    app.clear_book_publisher(Some(&reader), &epub.data.id)
        .await
        .assert_error(BookAlreadyDeleted);
}
