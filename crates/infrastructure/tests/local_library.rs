use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use futures_util::TryStreamExt;
use shiro_application::library::{
    book::{
        commands::{
            ChangeBookAuthorsCommand, ChangeBookDescriptionCommand, ChangeBookIdentifierCommand,
            ChangeBookLanguageCommand, ChangeBookPublishedDateCommand, ChangeBookPublisherCommand,
            ChangeBookSubtitleCommand, ClearBookAuthorsCommand, ClearBookLanguageCommand,
            ClearBookPublisherCommand, ClearBookSubtitleCommand, CreateBookCommand,
            DeleteBookCommand, GetBookCoverCommand, PermanentlyDeleteBooksCommand,
            RestoreBookIdCommand,
        },
        errors::BookApplicationError,
        file::errors::BookFileApplicationError,
        queries::{GetBookMetadataQuery, GetBooksQuery},
    },
    errors::LibraryApplicationError,
    tag::commands::{BindBookTagCommand, CreateTagDefinitionCommand, UndefineTagCommand},
};
use shiro_domain::library::{
    book::{
        locator::NavigationLocator,
        value_objects::BookFileType,
    },
    tag::value_objects::TagDeleteStrategy,
};
use shiro_infrastructure::library_runtime::LocalLibrary;
use tokio::io::AsyncReadExt;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

async fn upload_bytes(library: &LocalLibrary, bytes: &[u8]) -> String {
    let hash = blake3::hash(bytes).to_hex().to_string();
    library
        .book_file_service()
        .upload_book_file(hash.clone(), bytes.len() as u64, Cursor::new(bytes.to_vec()))
        .await
        .expect("upload valid book file");
    hash
}

async fn read_cover_bytes(library: &LocalLibrary, book_id: &str) -> (Vec<u8>, String) {
    let (mut stream, mime) = library
        .book_service()
        .get_cover(GetBookCoverCommand {
            id: book_id.to_owned(),
        })
        .await
        .expect("read cover");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.try_next().await.expect("cover stream chunk") {
        bytes.extend_from_slice(&chunk);
    }
    (bytes, mime)
}

#[tokio::test]
async fn local_library_operates_without_identity_or_axum_and_persists_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf fixture");
    let epub_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub fixture");

    let library = LocalLibrary::open(dir.path(), None)
        .await
        .expect("open local library");

    // Local mode must NOT create any identity database or per-user directory structure.
    assert!(dir.path().join("library.db").is_file());
    assert!(!dir.path().join("user.db").exists());
    assert!(!dir.path().join("users").exists());

    // New local libraries start with an empty tag tree.
    let initial_tags = library.tag_service().get_tree().await.expect("get tree");
    assert!(initial_tags.is_empty());

    // Upload PDF and EPUB directly via LocalLibrary context.
    let pdf_hash = upload_bytes(&library, &pdf_bytes).await;
    let epub_hash = upload_bytes(&library, &epub_bytes).await;

    let pdf_book = library
        .book_service()
        .create_book(CreateBookCommand {
            title: "Smoke Pattern PDF".into(),
            hash: pdf_hash.clone(),
        })
        .await
        .expect("create pdf book");
    assert_eq!(pdf_book.file_type, "PDF");

    let epub_book = library
        .book_service()
        .create_book(CreateBookCommand {
            title: "Smoke Cover EPUB".into(),
            hash: epub_hash.clone(),
        })
        .await
        .expect("create epub book");
    assert_eq!(epub_book.file_type, "EPUB");

    // Verify EPUB cover is the real embedded 128x192 PNG cover.
    let (epub_cover, epub_mime) = read_cover_bytes(&library, &epub_book.id).await;
    assert_eq!(epub_mime, "image/png");
    let epub_img = image::load_from_memory(&epub_cover)
        .expect("decode epub cover")
        .to_rgba8();
    assert_eq!(epub_img.dimensions(), (128, 192));

    // Without PDFium configured, PDF cover deterministically falls back to a 1000x1600 blank PNG.
    let (pdf_cover, pdf_mime) = read_cover_bytes(&library, &pdf_book.id).await;
    assert_eq!(pdf_mime, "image/png");
    let pdf_img = image::load_from_memory(&pdf_cover)
        .expect("decode pdf cover")
        .to_rgba8();
    assert_eq!(pdf_img.dimensions(), (1000, 1600));
    assert!(pdf_img.pixels().all(|px| px.0 == [255, 255, 255, 255]));

    // Create custom tag and bind it to the PDF book.
    let custom_tag = library
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: "Science".into(),
            parent_id: None,
            color: "#336699".into(),
            allows_value: true,
            value_regex: Some(r"[A-Za-z]+".into()),
        })
        .await
        .expect("define tag");
    library
        .tag_service()
        .bind_book_tag(BindBookTagCommand {
            book_id: pdf_book.id.clone(),
            tag_id: custom_tag.id.clone(),
            value: Some("Physics".into()),
        })
        .await
        .expect("bind tag");

    // Restart library at the same root directory.
    drop(library);
    let reopened = LocalLibrary::open(dir.path(), None)
        .await
        .expect("reopen local library");

    // Verify books, raw bytes, tags, and bindings all survived restart cleanly.
    let books = reopened
        .book_service()
        .list_books(GetBooksQuery { ids: vec![] })
        .await
        .expect("list books after restart");
    assert_eq!(books.len(), 2);

    let parsed_pdf_hash = blake3::Hash::from_hex(&pdf_hash).expect("valid pdf hash hex");
    let mut opened = reopened
        .book_file_service()
        .open_book_file_range(&parsed_pdf_hash, 0, None)
        .await
        .expect("open raw pdf range after restart");
    assert_eq!(opened.total_size, pdf_bytes.len() as u64);
    assert_eq!(opened.offset, 0);
    assert_eq!(opened.length, pdf_bytes.len() as u64);
    let mut restored_pdf = Vec::new();
    opened
        .reader
        .read_to_end(&mut restored_pdf)
        .await
        .expect("read restored pdf");
    assert_eq!(restored_pdf, pdf_bytes);

    let tags_after = reopened
        .tag_service()
        .get_tree()
        .await
        .expect("tag tree after restart");
    assert_eq!(tags_after.len(), 1);
    assert_eq!(tags_after[0].tag.name, "Science");

    let bindings = reopened
        .tag_service()
        .get_book_tags(pdf_book.id.clone())
        .await
        .expect("book tags after restart");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].tag_id, custom_tag.id);
    assert_eq!(bindings[0].value.as_deref(), Some("Physics"));
}

#[tokio::test]
async fn local_library_rejects_corrupted_file_and_non_directory_root_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();

    // Case 1: root path points to a regular file -> must fail explicitly.
    let file_as_root = dir.path().join("not-a-dir");
    fs::write(&file_as_root, b"hello").unwrap();
    let err = LocalLibrary::open(&file_as_root, None)
        .await
        .err()
        .expect("regular file root must fail");
    assert!(err.to_string().contains("not a directory"), "{err}");

    // Case 2: corrupted library.db -> must fail explicitly and preserve original bytes.
    let corrupt_root = dir.path().join("corrupt-lib");
    fs::create_dir_all(&corrupt_root).unwrap();
    let corrupt_db = corrupt_root.join("library.db");
    let sentinel = b"NOT A REAL SQLITE DATABASE FILE!!";
    fs::write(&corrupt_db, sentinel).unwrap();
    let err = LocalLibrary::open(&corrupt_root, None)
        .await
        .err()
        .expect("corrupted sqlite file must fail");
    assert!(err.to_string().contains("corrupted"), "{err}");
    assert_eq!(
        fs::read(&corrupt_db).unwrap(),
        sentinel,
        "corrupted file must not be overwritten"
    );
}

#[tokio::test]
async fn local_library_wal_checkpoint_keeps_database_self_contained_for_offline_copy() {
    let source = tempfile::tempdir().unwrap();
    let epub_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub fixture");

    let library = LocalLibrary::open(source.path(), None)
        .await
        .expect("open local library with WAL enabled");
    let hash = upload_bytes(&library, &epub_bytes).await;
    let book = library
        .book_service()
        .create_book(CreateBookCommand {
            title: "WAL Checkpoint Book".into(),
            hash,
        })
        .await
        .expect("create book");

    let db_path = library.db_path().to_path_buf();
    drop(library);
    shiro_infrastructure::db::pool::checkpoint_library_wal(&db_path)
        .await
        .expect("checkpoint WAL after close");

    // Copy only library.db and books/ (excluding any -wal/-shm sidecars) to verify self-consistency.
    let copy_dir = tempfile::tempdir().unwrap();
    fs::copy(
        source.path().join("library.db"),
        copy_dir.path().join("library.db"),
    )
    .expect("copy library.db");
    copy_dir_recursive(&source.path().join("books"), &copy_dir.path().join("books"));

    let reopened = LocalLibrary::open(copy_dir.path(), None)
        .await
        .expect("reopen copied library");
    let books = reopened
        .book_service()
        .list_books(GetBooksQuery { ids: vec![] })
        .await
        .expect("list books in copied library");
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].id, book.id);
    assert_eq!(books[0].title, "WAL Checkpoint Book");
}

#[tokio::test]
async fn undefine_tag_promote_allows_child_with_same_name_as_deleted_parent() {
    let dir = tempfile::tempdir().unwrap();
    let library = LocalLibrary::open(dir.path(), None)
        .await
        .expect("open local library");

    // Case 1: Root tag "Same" -> child tag "Same" promoted to root.
    let root = library
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: "Same".into(),
            parent_id: None,
            color: "#112233".into(),
            allows_value: false,
            value_regex: None,
        })
        .await
        .expect("define root Same");
    let child = library
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: "Same".into(),
            parent_id: Some(root.id.clone()),
            color: "#445566".into(),
            allows_value: false,
            value_regex: None,
        })
        .await
        .expect("define child Same");

    library
        .tag_service()
        .undefine_tag(UndefineTagCommand {
            id: root.id.clone(),
            strategy: TagDeleteStrategy::Promote,
        })
        .await
        .expect("promoting same-named child to root must succeed");

    let tree = library
        .tag_service()
        .get_tree()
        .await
        .expect("list tag tree");
    assert!(tree.iter().all(|node| node.tag.id != root.id));
    let promoted_root = tree
        .iter()
        .find(|node| node.tag.id == child.id)
        .expect("promoted child should be a root tag");
    assert_eq!(promoted_root.tag.name, "Same");
    assert_eq!(promoted_root.tag.parent_id, None);
    assert_eq!(promoted_root.tag.color, "#445566");

    // Case 2: Nested Grandparent -> Parent("Nested") -> Child("Nested") promoted under Grandparent.
    let parent = library
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: "Nested".into(),
            parent_id: Some(child.id.clone()),
            color: "#223344".into(),
            allows_value: false,
            value_regex: None,
        })
        .await
        .expect("define parent Nested");
    let grandchild = library
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: "Nested".into(),
            parent_id: Some(parent.id.clone()),
            color: "#778899".into(),
            allows_value: false,
            value_regex: None,
        })
        .await
        .expect("define grandchild Nested");

    library
        .tag_service()
        .undefine_tag(UndefineTagCommand {
            id: parent.id.clone(),
            strategy: TagDeleteStrategy::Promote,
        })
        .await
        .expect("promoting same-named grandchild under grandparent must succeed");

    let tree = library
        .tag_service()
        .get_tree()
        .await
        .expect("list tag tree after nested promote");
    let grandparent = tree
        .iter()
        .find(|node| node.tag.id == child.id)
        .expect("grandparent tag should still exist");
    assert_eq!(grandparent.children.len(), 1);
    assert_eq!(grandparent.children[0].tag.id, grandchild.id);
    assert_eq!(grandparent.children[0].tag.name, "Nested");
    assert_eq!(
        grandparent.children[0].tag.parent_id.as_deref(),
        Some(child.id.as_str())
    );
    assert_eq!(grandparent.children[0].tag.color, "#778899");
}

#[tokio::test]
async fn local_book_file_storage_supports_meta_open_range_and_positional_read_within_container_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let epub_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub fixture");
    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf fixture");

    let library = LocalLibrary::open(dir.path(), None)
        .await
        .expect("open local library");

    let epub_hash_hex = upload_bytes(&library, &epub_bytes).await;
    let pdf_hash_hex = upload_bytes(&library, &pdf_bytes).await;
    let epub_hash = blake3::Hash::from_hex(&epub_hash_hex).expect("epub hash");
    let pdf_hash = blake3::Hash::from_hex(&pdf_hash_hex).expect("pdf hash");

    let epub_size = epub_bytes.len() as u64;
    let pdf_size = pdf_bytes.len() as u64;

    let epub_meta = library
        .book_file_service()
        .get_book_meta(&epub_hash)
        .expect("epub metadata");
    assert_eq!(epub_meta.file_type, BookFileType::EPUB);
    assert_eq!(epub_meta.size, epub_size);

    let pdf_meta = library
        .book_file_service()
        .get_book_meta(&pdf_hash)
        .expect("pdf metadata");
    assert_eq!(pdf_meta.file_type, BookFileType::PDF);
    assert_eq!(pdf_meta.size, pdf_size);

    // open_range across head, middle, clamped tail, and zero-length at EOF.
    for (offset, req_len, expected_slice) in [
        (0u64, Some(32u64), &epub_bytes[..32]),
        (128u64, Some(256u64), &epub_bytes[128..384]),
        (epub_size - 64, Some(9999u64), &epub_bytes[epub_bytes.len() - 64..]),
        (epub_size, Some(0u64), &[][..]),
    ] {
        let mut opened = library
            .book_file_service()
            .open_book_file_range(&epub_hash, offset, req_len)
            .await
            .expect("open_range in bounds");
        assert_eq!(opened.total_size, epub_size);
        assert_eq!(opened.offset, offset);
        assert_eq!(opened.length, expected_slice.len() as u64);
        let mut actual = Vec::new();
        opened.reader.read_to_end(&mut actual).await.unwrap();
        assert_eq!(actual.as_slice(), expected_slice);
    }

    // open_range rejects offset == size for non-zero reads, offset > size, and u64::MAX.
    for (offset, req_len) in [
        (epub_size, None),
        (epub_size, Some(1)),
        (epub_size + 1, Some(0)),
        (u64::MAX, Some(1)),
    ] {
        let err = library
            .book_file_service()
            .open_book_file_range(&epub_hash, offset, req_len)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LibraryApplicationError::BookFile(BookFileApplicationError::RangeNotSatisfiable {
                size
            }) if size == epub_size
        ));
    }

    // read_exact_at across head, middle, tail, and zero-length at EOF without leaking storage metadata.
    let head = library
        .book_file_service()
        .read_book_file_exact_at(&epub_hash, 0, 30)
        .await
        .expect("read head exact");
    assert_eq!(head.as_ref(), &epub_bytes[..30]);

    let mid = library
        .book_file_service()
        .read_book_file_exact_at(&epub_hash, 128, 300)
        .await
        .expect("read mid exact");
    assert_eq!(mid.as_ref(), &epub_bytes[128..428]);

    let tail = library
        .book_file_service()
        .read_book_file_exact_at(&epub_hash, epub_size - 22, 22)
        .await
        .expect("read tail exact");
    assert_eq!(tail.as_ref(), &epub_bytes[epub_bytes.len() - 22..]);

    let empty_eof = library
        .book_file_service()
        .read_book_file_exact_at(&epub_hash, epub_size, 0)
        .await
        .expect("read zero bytes at EOF");
    assert!(empty_eof.is_empty());

    let pdf_full = library
        .book_file_service()
        .read_book_file_exact_at(&pdf_hash, 0, pdf_bytes.len())
        .await
        .expect("read full pdf exact");
    assert_eq!(pdf_full.as_ref(), pdf_bytes.as_slice());

    for (offset, len) in [
        (epub_size, 1usize),
        (epub_size - 4, 5usize),
        (epub_size + 1, 0usize),
        (u64::MAX - 2, 10usize),
    ] {
        let err = library
            .book_file_service()
            .read_book_file_exact_at(&epub_hash, offset, len)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LibraryApplicationError::BookFile(BookFileApplicationError::RangeNotSatisfiable {
                size
            }) if size == epub_size
        ));
    }
}

#[tokio::test]
async fn local_library_extract_navigation_handles_valid_epub_and_returns_empty_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let epub_bytes = fs::read(workspace_root().join("apps/server/tests/samples/sample_document.epub"))
        .expect("sample_document.epub fixture");
    let broken_pdf_bytes = b"%PDF-1.4\nbroken-content-without-xref".to_vec();

    let library = LocalLibrary::open(dir.path(), None)
        .await
        .expect("open local library");

    // 1. Missing hash returns empty navigation without error.
    let unknown_hash = blake3::hash(b"non-existent-book-hash");
    let missing_nav = library
        .book_file_service()
        .extract_book_navigation(&unknown_hash)
        .await;
    assert!(missing_nav.is_empty());

    // 2. Valid EPUB returns populated TOC and linear spine units.
    let epub_hash_hex = upload_bytes(&library, &epub_bytes).await;
    let epub_hash = blake3::Hash::from_hex(&epub_hash_hex).unwrap();
    let epub_nav = library
        .book_file_service()
        .extract_book_navigation(&epub_hash)
        .await;
    assert!(!epub_nav.is_empty());
    assert_eq!(epub_nav.toc().len(), 6);
    assert!(epub_nav.total_weight() > 0);
    assert!(!epub_nav.reading_units().is_empty());
    assert!(matches!(
        epub_nav.toc()[0].locator(),
        Some(NavigationLocator::Epub(anchor)) if anchor.spine_index.is_some()
    ));

    // 3. PDF when Pdfium is not loaded returns empty navigation cleanly.
    let broken_pdf_hash_hex = upload_bytes(&library, &broken_pdf_bytes).await;
    let broken_pdf_hash = blake3::Hash::from_hex(&broken_pdf_hash_hex).unwrap();
    let pdf_nav = library
        .book_file_service()
        .extract_book_navigation(&broken_pdf_hash)
        .await;
    assert!(pdf_nav.is_empty());
}

#[tokio::test]
async fn local_library_book_metadata_extracts_persists_updates_and_cascades_on_permanent_delete() {
    let dir = tempfile::tempdir().unwrap();
    let epub_bytes = fs::read(workspace_root().join("apps/server/tests/samples/sample_document.epub"))
        .expect("sample_document.epub fixture");

    let library = LocalLibrary::open(dir.path(), None)
        .await
        .expect("open local library");

    let hash = upload_bytes(&library, &epub_bytes).await;
    let created = library
        .book_service()
        .create_book(CreateBookCommand {
            title: "Sample Metadata Book".into(),
            hash,
        })
        .await
        .expect("create book");

    let initial_meta = library
        .book_service()
        .get_book_metadata(GetBookMetadataQuery {
            id: created.id.clone(),
        })
        .await
        .expect("get initial metadata");
    assert_eq!(initial_meta.book_id, created.id);
    assert_eq!(initial_meta.file_type, "EPUB");
    assert_eq!(initial_meta.size, epub_bytes.len() as u64);
    assert_eq!(initial_meta.page_count, None);
    assert_eq!(initial_meta.created_at, created.created_at);
    let initial_updated_at: jiff::Timestamp = initial_meta.updated_at.parse().unwrap();
    let created_updated_at: jiff::Timestamp = created.updated_at.parse().unwrap();
    assert!(initial_updated_at >= created_updated_at);

    // Update all 7 metadata fields.
    library
        .book_service()
        .change_book_subtitle(ChangeBookSubtitleCommand {
            id: created.id.clone(),
            subtitle: "Second Edition".into(),
        })
        .await
        .expect("update subtitle");
    library
        .book_service()
        .change_book_authors(ChangeBookAuthorsCommand {
            id: created.id.clone(),
            authors: vec!["Ada Lovelace".into(), "Grace Hopper".into()],
        })
        .await
        .expect("update authors");
    library
        .book_service()
        .change_book_publisher(ChangeBookPublisherCommand {
            id: created.id.clone(),
            publisher: "Shiro Press".into(),
        })
        .await
        .expect("update publisher");
    library
        .book_service()
        .change_book_published_date(ChangeBookPublishedDateCommand {
            id: created.id.clone(),
            published_date: "2025-08".into(),
        })
        .await
        .expect("update published_date");
    library
        .book_service()
        .change_book_language(ChangeBookLanguageCommand {
            id: created.id.clone(),
            language: "zh-hans-cn".into(),
        })
        .await
        .expect("update language");
    library
        .book_service()
        .change_book_description(ChangeBookDescriptionCommand {
            id: created.id.clone(),
            description: "First paragraph.\nSecond paragraph.".into(),
        })
        .await
        .expect("update description");
    library
        .book_service()
        .change_book_identifier(ChangeBookIdentifierCommand {
            id: created.id.clone(),
            identifier: "CN 11-0106".into(),
        })
        .await
        .expect("update identifier");

    // Restart library and verify persisted values & canonicalization.
    drop(library);
    let reopened = LocalLibrary::open(dir.path(), None)
        .await
        .expect("reopen local library");

    let persisted = reopened
        .book_service()
        .get_book_metadata(GetBookMetadataQuery {
            id: created.id.clone(),
        })
        .await
        .expect("get persisted metadata");
    assert_eq!(persisted.subtitle.as_deref(), Some("Second Edition"));
    assert_eq!(persisted.authors, vec!["Ada Lovelace", "Grace Hopper"]);
    assert_eq!(persisted.publisher.as_deref(), Some("Shiro Press"));
    assert_eq!(persisted.published_date.as_deref(), Some("2025-08"));
    assert_eq!(persisted.language.as_deref(), Some("zh-Hans-CN"));
    assert_eq!(
        persisted.description.as_deref(),
        Some("First paragraph.\nSecond paragraph.")
    );
    assert_eq!(persisted.identifier.as_deref(), Some("CN:11-0106"));
    assert_eq!(persisted.page_count, None);
    assert_eq!(persisted.size, epub_bytes.len() as u64);
    assert_eq!(persisted.created_at, created.created_at);
    let persisted_updated_at: jiff::Timestamp = persisted.updated_at.parse().unwrap();
    assert!(persisted_updated_at >= initial_updated_at);
    assert!(persisted_updated_at > created_updated_at);

    // Clearing fields to None/empty works and persists; clearing again is rejected.
    reopened
        .book_service()
        .clear_book_subtitle(ClearBookSubtitleCommand {
            id: created.id.clone(),
        })
        .await
        .expect("clear subtitle");
    reopened
        .book_service()
        .clear_book_authors(ClearBookAuthorsCommand {
            id: created.id.clone(),
        })
        .await
        .expect("clear authors");
    reopened
        .book_service()
        .clear_book_publisher(ClearBookPublisherCommand {
            id: created.id.clone(),
        })
        .await
        .expect("clear publisher");
    let after_clear = reopened
        .book_service()
        .get_book_metadata(GetBookMetadataQuery {
            id: created.id.clone(),
        })
        .await
        .expect("get metadata after clear");
    assert_eq!(after_clear.subtitle, None);
    assert!(after_clear.authors.is_empty());
    assert_eq!(after_clear.publisher, None);
    assert!(matches!(
        reopened
            .book_service()
            .clear_book_subtitle(ClearBookSubtitleCommand {
                id: created.id.clone(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::SubtitleUnchanged
        ))
    ));
    assert!(matches!(
        reopened
            .book_service()
            .clear_book_authors(ClearBookAuthorsCommand {
                id: created.id.clone(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::AuthorsUnchanged
        ))
    ));
    assert!(matches!(
        reopened
            .book_service()
            .clear_book_publisher(ClearBookPublisherCommand {
                id: created.id.clone(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::PublisherUnchanged
        ))
    ));

    // Soft-deleting blocks metadata access; restoring brings it back intact.
    reopened
        .book_service()
        .delete_book(DeleteBookCommand {
            id: created.id.clone(),
        })
        .await
        .expect("soft delete book");
    assert!(matches!(
        reopened
            .book_service()
            .get_book_metadata(GetBookMetadataQuery {
                id: created.id.clone(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::AlreadyDeleted
        ))
    ));
    assert!(matches!(
        reopened
            .book_service()
            .change_book_language(ChangeBookLanguageCommand {
                id: created.id.clone(),
                language: "en".into(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::AlreadyDeleted
        ))
    ));
    assert!(matches!(
        reopened
            .book_service()
            .clear_book_language(ClearBookLanguageCommand {
                id: created.id.clone(),
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::AlreadyDeleted
        ))
    ));

    reopened
        .book_service()
        .restore_book(RestoreBookIdCommand {
            id: created.id.clone(),
        })
        .await
        .expect("restore book");
    let after_restore = reopened
        .book_service()
        .get_book_metadata(GetBookMetadataQuery {
            id: created.id.clone(),
        })
        .await
        .expect("metadata preserved after restore");
    assert_eq!(after_restore.identifier.as_deref(), Some("CN:11-0106"));

    reopened
        .book_service()
        .change_book_language(ChangeBookLanguageCommand {
            id: created.id.clone(),
            language: "en".into(),
        })
        .await
        .expect("metadata change succeeds again after restore");

    // Permanent delete cascades removal of book_metadata row.
    reopened
        .book_service()
        .delete_book(DeleteBookCommand {
            id: created.id.clone(),
        })
        .await
        .expect("soft delete before permanent delete");
    reopened
        .book_service()
        .permanently_delete_books(PermanentlyDeleteBooksCommand {
            ids: vec![created.id.clone()],
        })
        .await
        .expect("permanently delete book");

    assert!(matches!(
        reopened
            .book_service()
            .get_book_metadata(GetBookMetadataQuery { id: created.id })
            .await,
        Err(LibraryApplicationError::Book(BookApplicationError::NotFound))
    ));
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create target dir");
    for entry in fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let ty = entry.file_type().expect("file type");
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy file");
        }
    }
}
