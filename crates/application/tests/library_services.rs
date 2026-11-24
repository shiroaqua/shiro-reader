use std::{
    collections::{HashMap, HashSet},
    io::{self, Cursor, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use blake3::Hash;
use shiro_application::library::{
    book::{
        commands::{CreateBookCommand, RestoreBookIdCommand},
        errors::BookApplicationError,
        file::{
            errors::BookFileApplicationError,
            service::{BookFileService, UploadTimeoutPolicy},
        },
        queries::GetBooksQuery,
        service::BookService,
    },
    errors::LibraryApplicationError,
    ports::{
        BookChapterError, BookFileStorageError, BookFileStore, BookRepository,
        BookRepositoryError, BookTransaction, CoverByteStream, TagRepository, TagRepositoryError,
        TagTransaction,
    },
    tag::service::TagService,
};
use shiro_domain::library::{
    book::{
        chapter::{Chapter, ChapterId, ChapterLocation},
        entity::{Book, BookChange},
        file::entity::{BookFileChapter, BookFileType},
        value_objects::{BookId, BookTitle},
    },
    tag::{
        entity::{BookTag, BookTagChange, TagDefinition, TagDefinitionChange},
        errors::TagDomainError,
        value_objects::{TagColor, TagId, TagName, TagValueRule},
    },
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, ReadBuf, Take},
    sync::oneshot,
};

struct SaveGate {
    entered: Mutex<Option<oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

#[derive(Default)]
struct MockBookFileStore {
    existing: Mutex<HashSet<Hash>>,
    types: Mutex<HashMap<Hash, BookFileType>>,
    chapters: Mutex<HashMap<Hash, Vec<BookFileChapter>>>,
    save_error: Mutex<Option<ErrorKind>>,
    open_error: Mutex<Option<ErrorKind>>,
    gate: Mutex<Option<Arc<SaveGate>>>,
    after_first_read_gate: Mutex<Option<Arc<SaveGate>>>,
    post_read_gate: Mutex<Option<Arc<SaveGate>>>,
}

impl MockBookFileStore {
    fn set_save_error(&self, kind: ErrorKind) {
        *self.save_error.lock().unwrap() = Some(kind);
    }

    fn set_open_error(&self, kind: ErrorKind) {
        *self.open_error.lock().unwrap() = Some(kind);
    }

    fn insert_existing(&self, hash: Hash, file_type: Option<BookFileType>) {
        self.existing.lock().unwrap().insert(hash);
        if let Some(ft) = file_type {
            self.types.lock().unwrap().insert(hash, ft);
        }
    }

    fn set_chapters(&self, hash: Hash, chapters: Vec<BookFileChapter>) {
        self.chapters.lock().unwrap().insert(hash, chapters);
    }

    fn set_gate(&self, gate: Arc<SaveGate>) {
        *self.gate.lock().unwrap() = Some(gate);
    }

    fn set_after_first_read_gate(&self, gate: Arc<SaveGate>) {
        *self.after_first_read_gate.lock().unwrap() = Some(gate);
    }

    fn set_post_read_gate(&self, gate: Arc<SaveGate>) {
        *self.post_read_gate.lock().unwrap() = Some(gate);
    }
}

impl BookFileStore for MockBookFileStore {
    fn contains(&self, key: &Hash) -> bool {
        self.existing.lock().unwrap().contains(key)
    }

    fn get_type(&self, key: &Hash) -> Option<BookFileType> {
        self.types.lock().unwrap().get(key).copied()
    }

    fn get_chapters(&self, hash: &Hash) -> Vec<BookFileChapter> {
        self.chapters
            .lock()
            .unwrap()
            .get(hash)
            .cloned()
            .unwrap_or_default()
    }

    async fn save<R>(&self, hash: &Hash, mut reader: R) -> Result<(), BookFileStorageError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            if let Some(tx) = gate.entered.lock().unwrap().take() {
                let _ = tx.send(());
            }
            if let Some(rx) = gate.release.lock().await.take() {
                let _ = rx.await;
            }
        }

        if let Some(kind) = self.save_error.lock().unwrap().take() {
            return Err(kind.into());
        }

        let mut buf = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let after_first = self.after_first_read_gate.lock().unwrap().take();
            if let Some(gate) = after_first {
                if let Some(tx) = gate.entered.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                if let Some(rx) = gate.release.lock().await.take() {
                    let _ = rx.await;
                }
            }
        }

        let post_gate = self.post_read_gate.lock().unwrap().take();
        if let Some(gate) = post_gate {
            if let Some(tx) = gate.entered.lock().unwrap().take() {
                let _ = tx.send(());
            }
            if let Some(rx) = gate.release.lock().await.take() {
                let _ = rx.await;
            }
        }

        self.existing.lock().unwrap().insert(*hash);
        self.types.lock().unwrap().insert(*hash, BookFileType::EPUB);
        Ok(())
    }

    async fn open(&self, _hash: &Hash) -> Result<Take<File>, BookFileStorageError> {
        if let Some(kind) = self.open_error.lock().unwrap().take() {
            return Err(kind.into());
        }
        Err(ErrorKind::NotFound.into())
    }

    async fn open_cover(
        &self,
        _hash: Hash,
    ) -> Result<(CoverByteStream, String), BookFileStorageError> {
        Err(ErrorKind::NotFound.into())
    }
}

#[derive(Clone, Default)]
struct MockBookRepository {
    books: Arc<Mutex<Vec<Book>>>,
    chapters: Arc<Mutex<Vec<Chapter>>>,
    occupied_titles: Arc<Mutex<HashSet<String>>>,
}

impl BookRepository for MockBookRepository {
    type Tx<'a> = MockBookTransaction;

    async fn create(&mut self, book: Book) -> Result<Book, BookRepositoryError> {
        self.books.lock().unwrap().push(book.clone());
        Ok(book)
    }

    async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        self.books
            .lock()
            .unwrap()
            .iter()
            .find(|book| book.id() == id)
            .cloned()
            .ok_or(BookRepositoryError::NotFound)
    }

    async fn list_active(&mut self, _ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        Ok(self.books.lock().unwrap().clone())
    }

    async fn list_trashed(&mut self) -> Result<Vec<Book>, BookRepositoryError> {
        Ok(self.books.lock().unwrap().clone())
    }

    async fn create_chapter(&mut self, chapter: Chapter) -> Result<Chapter, BookChapterError> {
        if !self
            .books
            .lock()
            .unwrap()
            .iter()
            .any(|book| book.id() == chapter.book_id())
        {
            return Err(BookChapterError::BookNotFound);
        }
        self.chapters.lock().unwrap().push(chapter.clone());
        Ok(chapter)
    }

    async fn delete_chapter(
        &mut self,
        book_id: &BookId,
        chapter_id: &ChapterId,
    ) -> Result<(), BookChapterError> {
        if !self
            .books
            .lock()
            .unwrap()
            .iter()
            .any(|book| book.id() == book_id)
        {
            return Err(BookChapterError::BookNotFound);
        }
        let mut chapters = self.chapters.lock().unwrap();
        let Some(index) = chapters
            .iter()
            .position(|chapter| chapter.book_id() == book_id && chapter.id() == chapter_id)
        else {
            return Err(BookChapterError::ChapterNotFound);
        };
        chapters.remove(index);
        Ok(())
    }

    async fn list_chapters(
        &mut self,
        book_id: &BookId,
    ) -> Result<Vec<Chapter>, BookChapterError> {
        if !self
            .books
            .lock()
            .unwrap()
            .iter()
            .any(|book| book.id() == book_id)
        {
            return Err(BookChapterError::BookNotFound);
        }
        Ok(self
            .chapters
            .lock()
            .unwrap()
            .iter()
            .filter(|chapter| chapter.book_id() == book_id)
            .cloned()
            .collect())
    }

    async fn begin_transaction(&mut self) -> Result<Self::Tx<'_>, BookRepositoryError> {
        Ok(MockBookTransaction {
            books: Arc::clone(&self.books),
            chapters: Arc::clone(&self.chapters),
            occupied_titles: Arc::clone(&self.occupied_titles),
            pending_books: Vec::new(),
            pending_chapters: Vec::new(),
        })
    }
}

struct MockBookTransaction {
    books: Arc<Mutex<Vec<Book>>>,
    chapters: Arc<Mutex<Vec<Chapter>>>,
    occupied_titles: Arc<Mutex<HashSet<String>>>,
    pending_books: Vec<Book>,
    pending_chapters: Vec<Chapter>,
}

impl BookTransaction for MockBookTransaction {
    async fn create_book(&mut self, book: Book) -> Result<Book, BookRepositoryError> {
        self.pending_books.push(book.clone());
        Ok(book)
    }

    async fn create_chapter(
        &mut self,
        chapter: Chapter,
    ) -> Result<Chapter, BookChapterError> {
        if self
            .pending_chapters
            .iter()
            .any(|existing| {
                existing.book_id() == chapter.book_id()
                    && existing.position() == chapter.position()
            })
        {
            return Err(BookChapterError::PositionConflict);
        }
        self.pending_chapters.push(chapter.clone());
        Ok(chapter)
    }

    async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        self.books
            .lock()
            .unwrap()
            .iter()
            .find(|book| book.id() == id)
            .cloned()
            .ok_or(BookRepositoryError::NotFound)
    }

    async fn find_by_ids(&mut self, _ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        Ok(Vec::new())
    }

    async fn list_active_titles(&mut self) -> Result<HashSet<String>, BookRepositoryError> {
        Ok(self.occupied_titles.lock().unwrap().clone())
    }

    async fn apply_book_change(&mut self, _change: BookChange<'_>) -> Result<(), BookRepositoryError> {
        Ok(())
    }

    async fn delete_books(&mut self, _books: Vec<Book>) -> Result<(), BookRepositoryError> {
        Ok(())
    }

    async fn list_tag_bindings(
        &mut self,
        _book_ids: &[BookId],
    ) -> Result<Vec<BookTag>, BookRepositoryError> {
        Ok(Vec::new())
    }

    async fn apply_tag_binding_change(
        &mut self,
        _change: BookTagChange<'_>,
    ) -> Result<(), BookRepositoryError> {
        Ok(())
    }

    async fn commit(self) -> Result<(), BookRepositoryError> {
        self.books.lock().unwrap().extend(self.pending_books);
        self.chapters.lock().unwrap().extend(self.pending_chapters);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockTagRepository {
    definitions: Arc<Mutex<Vec<TagDefinition>>>,
}

impl TagRepository for MockTagRepository {
    type Tx<'a> = MockTagTransaction;

    async fn create(
        &mut self,
        definition: TagDefinition,
    ) -> Result<TagDefinition, TagRepositoryError> {
        Ok(definition)
    }

    async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        Ok(self.definitions.lock().unwrap().clone())
    }

    async fn active_book_exists(&mut self, _book_id: &BookId) -> Result<bool, TagRepositoryError> {
        Ok(true)
    }

    async fn list_active_book_tags(
        &mut self,
        _book_id: &BookId,
    ) -> Result<Vec<BookTag>, TagRepositoryError> {
        Ok(Vec::new())
    }

    async fn begin_transaction(&mut self) -> Result<Self::Tx<'_>, TagRepositoryError> {
        Ok(MockTagTransaction)
    }
}

struct MockTagTransaction;

impl TagTransaction for MockTagTransaction {
    async fn find_definition_by_id(
        &mut self,
        _id: &TagId,
    ) -> Result<TagDefinition, TagRepositoryError> {
        Err(TagRepositoryError::NotFound)
    }

    async fn definition_exists(&mut self, _id: &TagId) -> Result<bool, TagRepositoryError> {
        Ok(false)
    }

    async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        Ok(Vec::new())
    }

    async fn apply_definition_change(
        &mut self,
        _change: TagDefinitionChange<'_>,
    ) -> Result<(), TagRepositoryError> {
        Ok(())
    }

    async fn delete_definitions(
        &mut self,
        _definitions: Vec<TagDefinition>,
    ) -> Result<(), TagRepositoryError> {
        Ok(())
    }

    async fn active_book_exists(&mut self, _book_id: &BookId) -> Result<bool, TagRepositoryError> {
        Ok(false)
    }

    async fn find_binding(
        &mut self,
        _book_id: &BookId,
        _tag_id: &TagId,
    ) -> Result<Option<BookTag>, TagRepositoryError> {
        Ok(None)
    }

    async fn create_binding(&mut self, _binding: BookTag) -> Result<(), TagRepositoryError> {
        Ok(())
    }

    async fn apply_binding_change(
        &mut self,
        _change: BookTagChange<'_>,
    ) -> Result<(), TagRepositoryError> {
        Ok(())
    }

    async fn delete_binding(&mut self, _binding: BookTag) -> Result<(), TagRepositoryError> {
        Ok(())
    }

    async fn commit(self) -> Result<(), TagRepositoryError> {
        Ok(())
    }
}

#[tokio::test]
async fn book_file_service_upload_guard_blocks_concurrent_upload_until_release() {
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let gate = Arc::new(SaveGate {
        entered: Mutex::new(Some(entered_tx)),
        release: tokio::sync::Mutex::new(Some(release_rx)),
    });

    let store = MockBookFileStore::default();
    store.set_gate(gate);
    let service = Arc::new(BookFileService::new(store));

    let payload = b"sample-content".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    let svc = Arc::clone(&service);
    let upload_hash = hash.clone();
    let upload_data = payload.clone();
    let task = tokio::spawn(async move {
        let len = upload_data.len() as u64;
        svc.upload_book_file(upload_hash, len, Cursor::new(upload_data))
            .await
    });

    entered_rx.await.unwrap();

    // Probe also returns UploadConflict while an upload is in progress.
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    let dup = service
        .upload_book_file(hash.clone(), payload.len() as u64, Cursor::new(payload.clone()))
        .await;
    assert!(matches!(
        dup,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    release_tx.send(()).unwrap();
    task.await.unwrap().expect("first upload completes");

    // Once stored, probe returns Ok and subsequent uploads see AlreadyExists.
    assert!(service.probe_book_file(&hash).is_ok());
    let after = service
        .upload_book_file(hash, payload.len() as u64, Cursor::new(payload))
        .await;
    assert!(matches!(
        after,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::AlreadyExists
        ))
    ));
}

#[tokio::test]
async fn book_file_service_maps_storage_errors_to_application_errors() {
    let store = MockBookFileStore::default();
    let service = BookFileService::new(store);
    let hash_obj = blake3::hash(b"x");
    let hash = hash_obj.to_hex().to_string();

    let empty = MockBookFileStore::default();
    empty.set_save_error(ErrorKind::AlreadyExists);
    assert!(matches!(
        BookFileService::new(empty)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::AlreadyExists
        ))
    ));

    let mismatch = MockBookFileStore::default();
    mismatch.set_save_error(ErrorKind::InvalidData);
    assert!(matches!(
        BookFileService::new(mismatch)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::HashMismatch
        ))
    ));

    let unsupported = MockBookFileStore::default();
    unsupported.set_save_error(ErrorKind::Unsupported);
    assert!(matches!(
        BookFileService::new(unsupported)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::Unsupported
        ))
    ));

    let io_err = MockBookFileStore::default();
    io_err.set_save_error(ErrorKind::Other);
    assert!(matches!(
        BookFileService::new(io_err)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::Storage(_)
        ))
    ));

    // download_book_file returns NotFound when absent or when open returns NotFound.
    assert!(matches!(
        service.download_book_file(&hash).await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
    let missing_on_disk = MockBookFileStore::default();
    missing_on_disk.insert_existing(hash_obj, Some(BookFileType::EPUB));
    missing_on_disk.set_open_error(ErrorKind::NotFound);
    assert!(matches!(
        BookFileService::new(missing_on_disk)
            .download_book_file(&hash)
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));

    let open_io_err = MockBookFileStore::default();
    open_io_err.insert_existing(hash_obj, Some(BookFileType::EPUB));
    open_io_err.set_open_error(ErrorKind::PermissionDenied);
    let err = BookFileService::new(open_io_err)
        .download_book_file(&hash)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LibraryApplicationError::BookFile(BookFileApplicationError::Storage(_))
    ));
}

#[tokio::test]
async fn book_creation_rolls_back_when_default_chapter_creation_fails() {
    let hash = blake3::hash(b"book-with-duplicate-chapters");
    let store = MockBookFileStore::default();
    store.insert_existing(hash, Some(BookFileType::EPUB));
    store.set_chapters(
        hash,
        ["First", "Duplicate"]
            .into_iter()
            .map(|title| BookFileChapter {
                position: 0,
                title: title.to_owned(),
                location: ChapterLocation::Epub {
                    href: "Text/chapter.xhtml".to_owned(),
                    cfi: None,
                },
            })
            .collect(),
    );

    let repository = MockBookRepository::default();
    let mut service = BookService::new(
        repository.clone(),
        Arc::new(BookFileService::new(store)),
    );
    let error = service
        .create_book(CreateBookCommand {
            title: "Book".to_owned(),
            hash: hash.to_hex().to_string(),
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        LibraryApplicationError::Book(BookApplicationError::PositionConflict)
    ));
    assert!(repository.books.lock().unwrap().is_empty());
    assert!(repository.chapters.lock().unwrap().is_empty());
}

#[tokio::test]
async fn book_service_restore_exhausted_suffixes_and_missing_file_type_map_to_application_errors() {
    let hash = blake3::hash(b"book-bytes");
    let mut trashed = Book::new(
        BookId::new(),
        BookTitle::parse("Title").unwrap(),
        hash,
    )
    .unwrap();
    let trashed_id = trashed.id().to_string();
    let _ = trashed.mark_deleted().unwrap();

    let mut occupied = HashSet::new();
    occupied.insert("Title".to_owned());
    for suffix in 1..=1024 {
        occupied.insert(format!("Title ({suffix})"));
    }

    let repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![trashed.clone()])),
        chapters: Arc::new(Mutex::new(Vec::new())),
        occupied_titles: Arc::new(Mutex::new(occupied)),
    };
    let store = MockBookFileStore::default();
    // Intentionally leave file type missing for hash -> get_trash and get_books reflect Unsupported.
    store.insert_existing(hash, None);
    let mut service = BookService::new(repo.clone(), Arc::new(BookFileService::new(store)));

    let restore_err = service
        .restore_book(RestoreBookIdCommand { id: trashed_id })
        .await
        .unwrap_err();
    assert!(matches!(
        restore_err,
        LibraryApplicationError::Book(BookApplicationError::TitleConflict)
    ));

    let trash_err = service.list_trashed_book().await.unwrap_err();
    assert!(matches!(
        trash_err,
        LibraryApplicationError::BookFile(BookFileApplicationError::Unsupported)
    ));

    let active = Book::new(
        BookId::new(),
        BookTitle::parse("ActiveNoType").unwrap(),
        hash,
    )
    .unwrap();
    let active_repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![active])),
        chapters: Arc::new(Mutex::new(Vec::new())),
        occupied_titles: Arc::new(Mutex::new(HashSet::new())),
    };
    let mut active_service = BookService::new(
        active_repo,
        Arc::new(BookFileService::new(MockBookFileStore::default())),
    );
    let books_err = active_service
        .list_books(GetBooksQuery { ids: vec![] })
        .await
        .unwrap_err();
    assert!(matches!(
        books_err,
        LibraryApplicationError::BookFile(BookFileApplicationError::Unsupported)
    ));
}

#[tokio::test]
async fn tag_service_reflects_corrupted_tag_tree_from_repository_as_application_error() {
    let orphan = TagDefinition::new(
        TagName::parse("Orphan").unwrap(),
        Some(TagId::new()),
        TagColor::parse("#112233").unwrap(),
        TagValueRule::new(false, None).unwrap(),
    );
    let repo = MockTagRepository {
        definitions: Arc::new(Mutex::new(vec![orphan])),
    };
    let mut service = TagService::new(repo);
    let err = service.get_tree().await.unwrap_err();
    assert!(matches!(
        err,
        LibraryApplicationError::Tag(shiro_application::library::tag::errors::TagApplicationError::Domain(
            TagDomainError::InvalidTree
        ))
    ));
}

struct PendingReader;

impl AsyncRead for PendingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

#[test]
fn upload_timeout_policy_scales_safely_without_overflow() {
    let policy = UploadTimeoutPolicy::default();
    assert_eq!(policy.timeout_for_size(0), Duration::from_secs(30));
    assert_eq!(
        policy.timeout_for_size(1024 * 1024),
        Duration::from_secs(46)
    );
    assert_eq!(
        policy.timeout_for_size(64 * 1024 * 1024),
        Duration::from_secs(30 + 1024)
    );
    assert_eq!(
        policy.timeout_for_size(512 * 1024 * 1024),
        Duration::from_secs(30 + 8192)
    );
    assert_eq!(
        policy.timeout_for_size(u64::MAX),
        Duration::from_secs(86_400)
    );

    let zero_rate = UploadTimeoutPolicy {
        min_bytes_per_sec: 0,
        ..UploadTimeoutPolicy::default()
    };
    assert_eq!(
        zero_rate.timeout_for_size(u64::MAX),
        Duration::from_secs(86_400)
    );
}

#[tokio::test]
async fn book_file_service_stalled_stream_times_out_and_releases_lease() {
    let policy = UploadTimeoutPolicy {
        base_timeout: Duration::from_millis(60),
        min_bytes_per_sec: 1_000_000,
        min_timeout: Duration::from_millis(60),
        max_timeout: Duration::from_secs(5),
    };
    let service = BookFileService::with_timeout_policy(MockBookFileStore::default(), policy);
    let payload = b"retry-after-timeout".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    let err = service
        .upload_book_file(hash.clone(), payload.len() as u64, PendingReader)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LibraryApplicationError::BookFile(BookFileApplicationError::UploadTimeout)
    ));

    // Lease has been released, so probe sees NotFound and a retry succeeds.
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
    service
        .upload_book_file(hash.clone(), payload.len() as u64, Cursor::new(payload))
        .await
        .expect("retry upload after timeout succeeds");
    assert!(service.probe_book_file(&hash).is_ok());
}

#[tokio::test]
async fn book_file_service_finalizing_phase_is_not_preempted_by_stream_timeout() {
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let post_gate = Arc::new(SaveGate {
        entered: Mutex::new(Some(entered_tx)),
        release: tokio::sync::Mutex::new(Some(release_rx)),
    });

    let store = MockBookFileStore::default();
    store.set_post_read_gate(post_gate);

    let policy = UploadTimeoutPolicy {
        base_timeout: Duration::from_millis(40),
        min_bytes_per_sec: 1_000_000,
        min_timeout: Duration::from_millis(40),
        max_timeout: Duration::from_secs(5),
    };
    let service = Arc::new(BookFileService::with_timeout_policy(store, policy));

    let payload = b"finalize-protected".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    let svc = Arc::clone(&service);
    let h = hash.clone();
    let data = payload.clone();
    let task = tokio::spawn(async move {
        let len = data.len() as u64;
        svc.upload_book_file(h, len, Cursor::new(data)).await
    });

    // Wait until stream EOF has transitioned the lease to Finalizing.
    entered_rx.await.unwrap();
    // Sleep well beyond the 40ms streaming deadline.
    tokio::time::sleep(Duration::from_millis(120)).await;

    // Probe and concurrent upload must still get UploadConflict (not preempted).
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));
    assert!(matches!(
        service
            .upload_book_file(hash.clone(), payload.len() as u64, Cursor::new(payload))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    release_tx.send(()).unwrap();
    task.await
        .unwrap()
        .expect("finalizing upload finishes cleanly");
    assert!(service.probe_book_file(&hash).is_ok());
}

#[tokio::test]
async fn book_file_service_rejects_shorter_and_longer_streams_as_size_mismatch() {
    let service = BookFileService::new(MockBookFileStore::default());
    let payload = b"exact-size-payload".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    // Actual shorter than declared.
    let shorter = service
        .upload_book_file(
            hash.clone(),
            payload.len() as u64 + 5,
            Cursor::new(payload.clone()),
        )
        .await;
    assert!(matches!(
        shorter,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::SizeMismatch
        ))
    ));

    // Actual longer than declared.
    let longer = service
        .upload_book_file(
            hash.clone(),
            payload.len() as u64 - 1,
            Cursor::new(payload.clone()),
        )
        .await;
    assert!(matches!(
        longer,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::SizeMismatch
        ))
    ));

    // Invalid hash rejected by probe.
    assert!(matches!(
        service.probe_book_file("not-a-valid-hash"),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::InvalidHashFormat
        ))
    ));

    // Exact match still succeeds cleanly afterwards.
    service
        .upload_book_file(hash, payload.len() as u64, Cursor::new(payload))
        .await
        .expect("exact size upload succeeds");
}

#[tokio::test]
async fn book_file_service_expired_streaming_lease_takeover_does_not_suffer_aba_removal() {
    // Simulate a stuck save gate that holds an expired streaming lease until after another upload takes over.
    let (entered_1_tx, entered_1_rx) = oneshot::channel();
    let (release_1_tx, release_1_rx) = oneshot::channel();
    let gate1 = Arc::new(SaveGate {
        entered: Mutex::new(Some(entered_1_tx)),
        release: tokio::sync::Mutex::new(Some(release_1_rx)),
    });

    let store = MockBookFileStore::default();
    store.set_gate(gate1);

    let policy = UploadTimeoutPolicy {
        base_timeout: Duration::from_millis(50),
        min_bytes_per_sec: 1_000_000,
        min_timeout: Duration::from_millis(50),
        max_timeout: Duration::from_secs(5),
    };
    let service = Arc::new(BookFileService::with_timeout_policy(store, policy));

    let payload = b"aba-test-payload".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    // Task 1 enters save() and blocks before reading.
    let svc1 = Arc::clone(&service);
    let h1 = hash.clone();
    let d1 = payload.clone();
    let task1 = tokio::spawn(async move {
        let len = d1.len() as u64;
        svc1.upload_book_file(h1, len, Cursor::new(d1)).await
    });
    entered_1_rx.await.unwrap();

    // Wait for Task 1's streaming lease deadline to expire.
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Install gate2 isn't accessible directly once moved into service, so instead use a
    // second upload with PendingReader? No, we need Task 2 to stay in flight while Task 1 drops!
    // PendingReader stays in flight for 50ms — let's release Task 1 right after Task 2 starts.
    let svc2 = Arc::clone(&service);
    let h2 = hash.clone();
    let len = payload.len() as u64;
    let task2 = tokio::spawn(async move {
        svc2.upload_book_file(h2, len, PendingReader).await
    });

    // Give Task 2 a moment to take over the lease.
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Release Task 1: its ExactSizeReader sees Instant::now() >= deadline and returns UploadTimeout,
    // dropping Task 1's UploadGuard. Because lease_id differs, Task 2's lease MUST remain intact!
    release_1_tx.send(()).unwrap();
    let res1 = task1.await.unwrap();
    assert!(matches!(
        res1,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadTimeout
        ))
    ));

    // Task 2's lease must STILL be active (not removed by Task 1's Drop)!
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    // Cleanly wait for Task 2 to time out.
    let res2 = task2.await.unwrap();
    assert!(matches!(
        res2,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadTimeout
        ))
    ));
}

#[tokio::test]
async fn book_file_service_eof_after_deadline_expiration_cannot_enter_finalizing() {
    let (bytes_read_tx, bytes_read_rx) = oneshot::channel();
    let (eof_release_tx, eof_release_rx) = oneshot::channel();
    let after_read_gate = Arc::new(SaveGate {
        entered: Mutex::new(Some(bytes_read_tx)),
        release: tokio::sync::Mutex::new(Some(eof_release_rx)),
    });

    let store = MockBookFileStore::default();
    store.set_after_first_read_gate(after_read_gate);

    let policy = UploadTimeoutPolicy {
        base_timeout: Duration::from_millis(50),
        min_bytes_per_sec: 1_000_000,
        min_timeout: Duration::from_millis(50),
        max_timeout: Duration::from_secs(5),
    };
    let service = Arc::new(BookFileService::with_timeout_policy(store, policy));

    let payload = b"eof-after-deadline-payload".to_vec();
    let hash = blake3::hash(&payload).to_hex().to_string();

    // Task 1 reads all payload bytes before deadline, then pauses before reading EOF (simulating async disk flush).
    let svc1 = Arc::clone(&service);
    let h1 = hash.clone();
    let d1 = payload.clone();
    let task1 = tokio::spawn(async move {
        let len = d1.len() as u64;
        svc1.upload_book_file(h1, len, Cursor::new(d1)).await
    });
    bytes_read_rx.await.unwrap();

    // Expire Task 1's streaming deadline, then start Task 2 which steals the expired lease.
    tokio::time::sleep(Duration::from_millis(80)).await;
    let svc2 = Arc::clone(&service);
    let h2 = hash.clone();
    let len = payload.len() as u64;
    let task2 = tokio::spawn(async move {
        svc2.upload_book_file(h2, len, PendingReader).await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Release Task 1 to poll EOF: it must fail with UploadTimeout and NEVER persist or touch Task 2's lease.
    eof_release_tx.send(()).unwrap();
    let res1 = task1.await.unwrap();
    assert!(matches!(
        res1,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadTimeout
        ))
    ));
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    let res2 = task2.await.unwrap();
    assert!(matches!(
        res2,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadTimeout
        ))
    ));
    assert!(matches!(
        service.probe_book_file(&hash),
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
}
