use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use futures_util::TryStreamExt;
use shiro_application::library::{
    book::{
        commands::{CreateBookCommand, GetBookCoverCommand},
        queries::GetBooksQuery,
    },
    tag::commands::{BindBookTagCommand, CreateTagDefinitionCommand, UndefineTagCommand},
};
use shiro_domain::library::tag::value_objects::TagDeleteStrategy;
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
        .upload_book_file(hash.clone(), Cursor::new(bytes.to_vec()))
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

    // Default tags are seeded on first initialization.
    let initial_tags = library.tag_service().get_tree().await.expect("get tree");
    let mut default_names: Vec<&str> = initial_tags.iter().map(|n| n.tag.name.as_str()).collect();
    default_names.sort();
    assert_eq!(
        default_names,
        [
            "tag.author",
            "tag.edition",
            "tag.issnIsbn",
            "tag.language",
            "tag.publisher",
        ]
    );

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

    // Delete all default tags so we can verify restart does NOT re-seed them.
    for node in initial_tags {
        library
            .tag_service()
            .undefine_tag(UndefineTagCommand {
                id: node.tag.id,
                strategy: TagDeleteStrategy::Cascade,
            })
            .await
            .expect("delete default tag");
    }

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

    let mut raw_file = reopened
        .book_file_service()
        .download_book_file(&pdf_hash)
        .await
        .expect("download raw pdf after restart");
    let mut restored_pdf = Vec::new();
    raw_file
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
