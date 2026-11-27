use std::{
    collections::{HashMap, HashSet},
    io::{self, Cursor, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use blake3::Hash;
use bytes::Bytes;
use shiro_application::library::{
    book::{
        commands::{
            GetBookFileMetaCommand, OpenBookFileRangeCommand, RestoreBookIdCommand,
        },
        dto::{
            BookFileDescriptor, BookNavigationOutput, BookmarkLocatorDto, EpubNavTargetDto,
            EpubPointTargetDto, EpubRangeTargetDto, HighlightLocatorDto, NavigationLocatorDto,
            NormalizedRectDto, PdfDestDto, PdfPageRectsDto, PdfSelectionTargetDto,
        },
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
        BookFileMetadata, BookFileStorageError, BookFileStore, BookRepository,
        BookRepositoryError, BookTransaction, CoverByteStream, OpenedBookFile, TagRepository,
        TagRepositoryError, TagTransaction,
    },
    tag::service::TagService,
};
use shiro_domain::library::{
    book::{
        entity::{Book, BookChange},
        errors::BookDomainError,
        file::entity::BookFileType,
        locator::{
            BookAnchor, EpubAnchor, EpubFragment, EpubNavTarget, EpubPointTarget,
            EpubResourcePath, NavigationLocator, ReadingUnitOffset,
        },
        navigation::{BookNavigation, ReadingUnitDraft, TocEntryDraft},
        value_objects::{BookHash, BookId, BookTitle, TocEntryFlags},
    },
    tag::{
        entity::{BookTag, BookTagChange, TagDefinition, TagDefinitionChange},
        errors::TagDomainError,
        value_objects::{TagColor, TagId, TagName, TagValueRule},
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, ReadBuf},
    sync::oneshot,
};

struct SaveGate {
    entered: Mutex<Option<oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

#[derive(Default)]
struct MockBookFileStore {
    files: Mutex<HashMap<Hash, (BookFileMetadata, Bytes)>>,
    navigations: Mutex<HashMap<Hash, BookNavigation>>,
    save_error: Mutex<Option<BookFileStorageError>>,
    open_error: Mutex<Option<BookFileStorageError>>,
    gate: Mutex<Option<Arc<SaveGate>>>,
    after_first_read_gate: Mutex<Option<Arc<SaveGate>>>,
    post_read_gate: Mutex<Option<Arc<SaveGate>>>,
}

impl MockBookFileStore {
    fn set_save_error(&self, err: BookFileStorageError) {
        *self.save_error.lock().unwrap() = Some(err);
    }

    fn set_open_error(&self, err: BookFileStorageError) {
        *self.open_error.lock().unwrap() = Some(err);
    }

    fn set_navigation(&self, hash: Hash, navigation: BookNavigation) {
        self.navigations.lock().unwrap().insert(hash, navigation);
    }

    fn insert_existing(&self, hash: Hash, file_type: Option<BookFileType>) {
        if let Some(ft) = file_type {
            self.files.lock().unwrap().insert(
                hash,
                (
                    BookFileMetadata {
                        file_type: ft,
                        size: 0,
                    },
                    Bytes::new(),
                ),
            );
        }
    }

    fn insert_book_file(&self, hash: Hash, file_type: BookFileType, data: Bytes) {
        let meta = BookFileMetadata {
            file_type,
            size: data.len() as u64,
        };
        self.files.lock().unwrap().insert(hash, (meta, data));
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
    type Reader = Cursor<Bytes>;

    fn get_meta(&self, key: &Hash) -> Option<BookFileMetadata> {
        self.files.lock().unwrap().get(key).map(|(meta, _)| *meta)
    }

    async fn import<R>(&self, hash: &Hash, mut reader: R) -> Result<(), BookFileStorageError>
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

        if let Some(err) = self.save_error.lock().unwrap().take() {
            return Err(err);
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

        let data = Bytes::from(buf);
        self.files.lock().unwrap().insert(
            *hash,
            (
                BookFileMetadata {
                    file_type: BookFileType::EPUB,
                    size: data.len() as u64,
                },
                data,
            ),
        );
        Ok(())
    }

    async fn open_range(
        &self,
        hash: &Hash,
        offset: u64,
        length: Option<u64>,
    ) -> Result<OpenedBookFile<Self::Reader>, BookFileStorageError> {
        if let Some(err) = self.open_error.lock().unwrap().take() {
            return Err(err);
        }
        let Some((meta, data)) = self.files.lock().unwrap().get(hash).cloned() else {
            return Err(BookFileStorageError::NotFound);
        };
        let is_empty_req =
            length == Some(0) || (meta.size == 0 && offset == 0 && length.is_none());
        let in_bounds = if is_empty_req {
            offset <= meta.size
        } else {
            offset < meta.size
        };
        if !in_bounds {
            return Err(BookFileStorageError::RangeOutOfBounds {
                offset,
                len: length,
                size: meta.size,
            });
        }
        let remaining = meta.size - offset;
        let actual_len = length.map_or(remaining, |n| n.min(remaining));
        let start = usize::try_from(offset).unwrap();
        let end = start + usize::try_from(actual_len).unwrap();
        Ok(OpenedBookFile {
            reader: Cursor::new(data.slice(start..end)),
            total_size: meta.size,
            offset,
            length: actual_len,
        })
    }

    async fn read_exact_at(
        &self,
        hash: &Hash,
        offset: u64,
        len: usize,
    ) -> Result<Bytes, BookFileStorageError> {
        if let Some(err) = self.open_error.lock().unwrap().take() {
            return Err(err);
        }
        let Some((meta, data)) = self.files.lock().unwrap().get(hash).cloned() else {
            return Err(BookFileStorageError::NotFound);
        };
        if len == 0 {
            return if offset <= meta.size {
                Ok(Bytes::new())
            } else {
                Err(BookFileStorageError::RangeOutOfBounds {
                    offset,
                    len: Some(0),
                    size: meta.size,
                })
            };
        }
        let len_u64 = u64::try_from(len).map_err(|_| BookFileStorageError::RangeOutOfBounds {
            offset,
            len: Some(u64::MAX),
            size: meta.size,
        })?;
        let Some(end) = offset.checked_add(len_u64) else {
            return Err(BookFileStorageError::RangeOutOfBounds {
                offset,
                len: Some(len_u64),
                size: meta.size,
            });
        };
        if end > meta.size {
            return Err(BookFileStorageError::RangeOutOfBounds {
                offset,
                len: Some(len_u64),
                size: meta.size,
            });
        }
        Ok(data.slice(offset as usize..end as usize))
    }

    async fn open_cover(
        &self,
        _hash: Hash,
    ) -> Result<(CoverByteStream, String), BookFileStorageError> {
        Err(BookFileStorageError::NotFound)
    }

    async fn extract_navigation(&self, hash: &Hash) -> BookNavigation {
        self.navigations
            .lock()
            .unwrap()
            .get(hash)
            .cloned()
            .unwrap_or_else(BookNavigation::empty)
    }
}

#[derive(Clone, Default)]
struct MockBookRepository {
    books: Arc<Mutex<Vec<Book>>>,
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

    async fn begin_transaction(&mut self) -> Result<Self::Tx<'_>, BookRepositoryError> {
        Ok(MockBookTransaction {
            books: Arc::clone(&self.books),
            occupied_titles: Arc::clone(&self.occupied_titles),
            pending_books: Vec::new(),
        })
    }
}

struct MockBookTransaction {
    books: Arc<Mutex<Vec<Book>>>,
    occupied_titles: Arc<Mutex<HashSet<String>>>,
    pending_books: Vec<Book>,
}

impl BookTransaction for MockBookTransaction {
    async fn create_book(&mut self, book: Book) -> Result<Book, BookRepositoryError> {
        self.pending_books.push(book.clone());
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
    empty.set_save_error(BookFileStorageError::AlreadyExists);
    assert!(matches!(
        BookFileService::new(empty)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::AlreadyExists
        ))
    ));

    let mismatch = MockBookFileStore::default();
    mismatch.set_save_error(BookFileStorageError::HashMismatch);
    assert!(matches!(
        BookFileService::new(mismatch)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::HashMismatch
        ))
    ));

    let unsupported = MockBookFileStore::default();
    unsupported.set_save_error(BookFileStorageError::Unsupported);
    assert!(matches!(
        BookFileService::new(unsupported)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::Unsupported
        ))
    ));

    let io_err = MockBookFileStore::default();
    io_err.set_save_error(BookFileStorageError::Io(io::Error::other("disk")));
    assert!(matches!(
        BookFileService::new(io_err)
            .upload_book_file(hash.clone(), 1, Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::Storage(_)
        ))
    ));

    // open_book_file_range and read_book_file_exact_at map NotFound, RangeOutOfBounds, and Io.
    assert!(matches!(
        service.open_book_file_range(&hash_obj, 0, None).await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
    assert!(matches!(
        service.read_book_file_exact_at(&hash_obj, 0, 1).await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
    let missing_on_disk = MockBookFileStore::default();
    missing_on_disk.insert_book_file(hash_obj, BookFileType::EPUB, Bytes::from_static(b"x"));
    missing_on_disk.set_open_error(BookFileStorageError::NotFound);
    assert!(matches!(
        BookFileService::new(missing_on_disk)
            .open_book_file_range(&hash_obj, 0, None)
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));

    let range_oob = MockBookFileStore::default();
    range_oob.insert_book_file(hash_obj, BookFileType::EPUB, Bytes::from_static(b"x"));
    assert!(matches!(
        BookFileService::new(range_oob)
            .open_book_file_range(&hash_obj, 1, Some(1))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::RangeNotSatisfiable { size: 1 }
        ))
    ));

    let open_io_err = MockBookFileStore::default();
    open_io_err.insert_book_file(hash_obj, BookFileType::EPUB, Bytes::from_static(b"x"));
    open_io_err.set_open_error(BookFileStorageError::Io(io::Error::from(
        ErrorKind::PermissionDenied,
    )));
    let err = BookFileService::new(open_io_err)
        .open_book_file_range(&hash_obj, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LibraryApplicationError::BookFile(BookFileApplicationError::Storage(_))
    ));
}

#[tokio::test]
async fn book_service_get_file_meta_and_open_range_enforce_active_status_and_existence() {
    let payload = Bytes::from_static(b"0123456789abcdef");
    let hash = blake3::hash(&payload);

    let active = Book::new(
        BookId::new(),
        BookTitle::parse("Active Book").unwrap(),
        BookHash::new(hash),
    );
    let active_id = active.id().to_string();

    let mut trashed = Book::new(
        BookId::new(),
        BookTitle::parse("Trashed Book").unwrap(),
        BookHash::new(hash),
    );
    let trashed_id = trashed.id().to_string();
    let _ = trashed.mark_deleted().unwrap();

    let missing_file_hash = blake3::hash(b"missing-from-store");
    let orphan_book = Book::new(
        BookId::new(),
        BookTitle::parse("Missing File Book").unwrap(),
        BookHash::new(missing_file_hash),
    );
    let orphan_id = orphan_book.id().to_string();

    let repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![active, trashed, orphan_book])),
        occupied_titles: Arc::new(Mutex::new(HashSet::new())),
    };
    let store = MockBookFileStore::default();
    store.insert_book_file(hash, BookFileType::EPUB, payload.clone());

    let mut service = BookService::new(repo, Arc::new(BookFileService::new(store)));

    let descriptor = service
        .get_book_file_meta(GetBookFileMetaCommand {
            id: active_id.clone(),
        })
        .await
        .expect("active book meta");
    assert_eq!(
        descriptor,
        BookFileDescriptor {
            hash,
            file_type: BookFileType::EPUB,
            size: payload.len() as u64,
        }
    );

    let (opened_desc, mut opened) = service
        .open_book_file_range(OpenBookFileRangeCommand {
            id: active_id,
            offset: 4,
            length: Some(6),
        })
        .await
        .expect("open active book range");
    assert_eq!(opened_desc, descriptor);
    assert_eq!(opened.total_size, payload.len() as u64);
    assert_eq!(opened.offset, 4);
    assert_eq!(opened.length, 6);
    let mut slice = Vec::new();
    opened.reader.read_to_end(&mut slice).await.unwrap();
    assert_eq!(slice.as_slice(), b"456789");

    for trashed_res in [
        service
            .get_book_file_meta(GetBookFileMetaCommand {
                id: trashed_id.clone(),
            })
            .await
            .map(|_| ()),
        service
            .open_book_file_range(OpenBookFileRangeCommand {
                id: trashed_id,
                offset: 0,
                length: None,
            })
            .await
            .map(|_| ()),
    ] {
        assert!(matches!(
            trashed_res,
            Err(LibraryApplicationError::Book(
                BookApplicationError::AlreadyDeleted
            ))
        ));
    }

    let missing_book_err = service
        .get_book_file_meta(GetBookFileMetaCommand {
            id: BookId::new().to_string(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        missing_book_err,
        LibraryApplicationError::Book(BookApplicationError::NotFound)
    ));

    let missing_file_err = service
        .open_book_file_range(OpenBookFileRangeCommand {
            id: orphan_id,
            offset: 0,
            length: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        missing_file_err,
        LibraryApplicationError::BookFile(BookFileApplicationError::NotFound)
    ));
}

#[tokio::test]
async fn book_service_restore_exhausted_suffixes_and_missing_file_type_map_to_application_errors() {
    let hash = blake3::hash(b"book-bytes");
    let mut trashed = Book::new(
        BookId::new(),
        BookTitle::parse("Title").unwrap(),
        BookHash::new(hash),
    );
    let trashed_id = trashed.id().to_string();
    let _ = trashed.mark_deleted().unwrap();

    let mut occupied = HashSet::new();
    occupied.insert("Title".to_owned());
    for suffix in 1..=1024 {
        occupied.insert(format!("Title ({suffix})"));
    }

    let repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![trashed.clone()])),
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
        BookHash::new(hash),
    );
    let active_repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![active])),
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

#[tokio::test]
async fn book_service_get_book_navigation_returns_navigation_and_enforces_invariants() {
    use shiro_application::library::book::queries::GetBookNavigationQuery;

    let payload = Bytes::from_static(b"navigation-book-bytes");
    let hash = blake3::hash(&payload);

    let active = Book::new(
        BookId::new(),
        BookTitle::parse("Active Book").unwrap(),
        BookHash::new(hash),
    );
    let active_id = active.id().to_string();

    let mut trashed = Book::new(
        BookId::new(),
        BookTitle::parse("Trashed Book").unwrap(),
        BookHash::new(hash),
    );
    let trashed_id = trashed.id().to_string();
    let _ = trashed.mark_deleted().unwrap();

    let missing_file_hash = blake3::hash(b"missing-nav-file");
    let orphan_book = Book::new(
        BookId::new(),
        BookTitle::parse("Missing File Book").unwrap(),
        BookHash::new(missing_file_hash),
    );
    let orphan_id = orphan_book.id().to_string();

    let nav = BookNavigation::build(
        vec![TocEntryDraft {
            title: "  Chapter   One ".to_owned(),
            locator: Some(
                NavigationLocator::epub(
                    Some(0),
                    EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap(),
                    EpubNavTarget::Fragment(EpubFragment::parse("sec-1").unwrap()),
                )
                .unwrap(),
            ),
            unit_offset: ReadingUnitOffset::new(0.25).unwrap(),
            flags: TocEntryFlags::empty(),
            children: vec![],
        }],
        vec![ReadingUnitDraft {
            is_linear: true,
            weight: 100,
            display_label: Some("EPUB/ch1.xhtml".to_owned()),
        }],
    )
    .unwrap();

    let repo = MockBookRepository {
        books: Arc::new(Mutex::new(vec![active, trashed, orphan_book])),
        occupied_titles: Arc::new(Mutex::new(HashSet::new())),
    };
    let store = MockBookFileStore::default();
    store.insert_book_file(hash, BookFileType::EPUB, payload);
    store.set_navigation(hash, nav);

    let mut service = BookService::new(repo, Arc::new(BookFileService::new(store)));

    let output = service
        .get_book_navigation(GetBookNavigationQuery {
            id: active_id.clone(),
        })
        .await
        .expect("active book navigation");
    assert_eq!(output.total_weight, 100);
    assert_eq!(output.linear_units.len(), 1);
    assert_eq!(output.toc.len(), 1);
    assert_eq!(output.toc[0].title, "Chapter One");
    assert_eq!(output.toc[0].start_progression, Some(0.25));
    assert!(matches!(
        output.toc[0].locator,
        Some(NavigationLocatorDto::Epub {
            spine_index: Some(0),
            ref href,
            target: EpubNavTargetDto::Fragment {
                fragment: ref frag,
            },
        }) if href == "EPUB/ch1.xhtml" && frag == "sec-1"
    ));

    assert!(matches!(
        service
            .get_book_navigation(GetBookNavigationQuery {
                id: "not-a-uuid".to_owned()
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::InvalidId
        ))
    ));
    assert!(matches!(
        service
            .get_book_navigation(GetBookNavigationQuery {
                id: BookId::new().to_string()
            })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::NotFound
        ))
    ));
    assert!(matches!(
        service
            .get_book_navigation(GetBookNavigationQuery { id: trashed_id })
            .await,
        Err(LibraryApplicationError::Book(
            BookApplicationError::AlreadyDeleted
        ))
    ));
    assert!(matches!(
        service
            .get_book_navigation(GetBookNavigationQuery { id: orphan_id })
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::NotFound
        ))
    ));
}

#[test]
fn book_anchor_dtos_round_trip_normalize_paths_and_reject_invalid_payloads() {
    // 1. NavigationLocatorDto (EPUB + PDF)
    let epub_nav_dto = NavigationLocatorDto::Epub {
        spine_index: Some(2),
        href: "/OEBPS/./ch2.xhtml".to_owned(),
        target: EpubNavTargetDto::Fragment {
            fragment: "sec-2".to_owned(),
        },
    };
    let epub_nav_domain = epub_nav_dto.try_into_domain().expect("valid epub nav");
    assert_eq!(epub_nav_domain.epub_href().unwrap().as_str(), "OEBPS/ch2.xhtml");
    let epub_nav_back = NavigationLocatorDto::from(&epub_nav_domain);
    let epub_nav_json = serde_json::to_value(&epub_nav_back).unwrap();
    assert_eq!(
        epub_nav_json,
        serde_json::json!({
            "kind": "epub",
            "spine_index": 2,
            "href": "OEBPS/ch2.xhtml",
            "target": {
                "kind": "fragment",
                "fragment": "sec-2"
            }
        })
    );
    assert_eq!(
        serde_json::from_value::<NavigationLocatorDto>(epub_nav_json).unwrap(),
        epub_nav_back
    );

    let pdf_nav_dto = NavigationLocatorDto::Pdf {
        page_index: 5,
        target: PdfDestDto::Xyz {
            left: Some(10.0),
            top: Some(200.0),
            zoom: Some(1.5),
            normalized_top: Some(0.25),
        },
    };
    let pdf_nav_domain = pdf_nav_dto.clone().try_into_domain().expect("valid pdf nav");
    let pdf_nav_back = NavigationLocatorDto::from(&pdf_nav_domain);
    assert_eq!(pdf_nav_back, pdf_nav_dto);

    // 2. BookmarkLocatorDto (EPUB point CFI + unit_offset, PDF FitPage)
    let epub_bm_dto = BookmarkLocatorDto::Epub {
        spine_index: Some(0),
        href: "OEBPS/ch1.xhtml".to_owned(),
        target: EpubPointTargetDto::Cfi {
            cfi: "epubcfi(/6/4[ch1]!/4/2/1:8)".to_owned(),
        },
    };
    let epub_bm_domain = epub_bm_dto.clone().try_into_domain().expect("valid epub bm");
    assert_eq!(BookmarkLocatorDto::from(&epub_bm_domain), epub_bm_dto);

    let epub_offset_bm_dto = BookmarkLocatorDto::Epub {
        spine_index: Some(0),
        href: "OEBPS/cover.xhtml".to_owned(),
        target: EpubPointTargetDto::UnitOffset { offset: 0.5 },
    };
    let epub_offset_domain = epub_offset_bm_dto
        .clone()
        .try_into_domain()
        .expect("valid offset bm");
    assert_eq!(
        BookmarkLocatorDto::from(&epub_offset_domain),
        epub_offset_bm_dto
    );

    let pdf_bm_dto = BookmarkLocatorDto::Pdf {
        page_index: 1,
        target: PdfDestDto::FitPage,
    };
    let pdf_bm_domain = pdf_bm_dto.clone().try_into_domain().expect("valid pdf bm");
    assert_eq!(BookmarkLocatorDto::from(&pdf_bm_domain), pdf_bm_dto);

    // 3. HighlightLocatorDto (EPUB range CFI + PDF selection rects)
    let epub_hl_dto = HighlightLocatorDto::Epub {
        spine_index: Some(1),
        href: "OEBPS/../OEBPS/ch1.xhtml".to_owned(),
        target: EpubRangeTargetDto {
            cfi: "epubcfi(/6/4[ch1]!/4/2,/1:0,/1:12)".to_owned(),
        },
    };
    let epub_hl_domain = epub_hl_dto.try_into_domain().expect("valid epub highlight");
    assert_eq!(epub_hl_domain.epub_href().unwrap().as_str(), "OEBPS/ch1.xhtml");
    let epub_hl_json = serde_json::to_value(HighlightLocatorDto::from(&epub_hl_domain)).unwrap();
    assert_eq!(
        epub_hl_json,
        serde_json::json!({
            "kind": "epub",
            "spine_index": 1,
            "href": "OEBPS/ch1.xhtml",
            "target": {
                "cfi": "epubcfi(/6/4[ch1]!/4/2,/1:0,/1:12)"
            }
        })
    );

    let pdf_hl_dto = HighlightLocatorDto::Pdf {
        page_index: 3,
        target: PdfSelectionTargetDto {
            rects: vec![NormalizedRectDto {
                left: 0.1,
                top: 0.2,
                width: 0.6,
                height: 0.05,
            }],
            extra_pages: vec![PdfPageRectsDto {
                page_index: 4,
                rects: vec![NormalizedRectDto {
                    left: 0.1,
                    top: 0.05,
                    width: 0.4,
                    height: 0.05,
                }],
            }],
        },
    };
    let pdf_hl_domain = pdf_hl_dto.clone().try_into_domain().expect("valid pdf hl");
    assert_eq!(HighlightLocatorDto::from(&pdf_hl_domain), pdf_hl_dto);
    assert_eq!(
        serde_json::from_value::<HighlightLocatorDto>(
            serde_json::to_value(&pdf_hl_dto).unwrap()
        )
        .unwrap(),
        pdf_hl_dto
    );

    // 4. Reject invalid payloads in try_into_domain and serde(deny_unknown_fields).
    assert_eq!(
        HighlightLocatorDto::Epub {
            spine_index: Some(0),
            href: "ch1.xhtml".to_owned(),
            target: EpubRangeTargetDto {
                cfi: "epubcfi(/6/4!/4/2/1:0)".to_owned(),
            },
        }
        .try_into_domain()
        .unwrap_err(),
        BookDomainError::InvalidEpubCfi
    );
    assert_eq!(
        BookmarkLocatorDto::Epub {
            spine_index: Some(0),
            href: "../../escape.xhtml".to_owned(),
            target: EpubPointTargetDto::UnitOffset { offset: 0.0 },
        }
        .try_into_domain()
        .unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        HighlightLocatorDto::Pdf {
            page_index: 2,
            target: PdfSelectionTargetDto {
                rects: vec![],
                extra_pages: vec![],
            },
        }
        .try_into_domain()
        .unwrap_err(),
        BookDomainError::InvalidPdfSelection
    );

    // Verify BookAnchor / EpubAnchor / EpubPointTarget direct construction works as expected.
    let direct_bm = BookAnchor::Epub(EpubAnchor {
        spine_index: Some(0),
        href: EpubResourcePath::parse("ch1.xhtml").unwrap(),
        target: EpubPointTarget::UnitOffset(ReadingUnitOffset::ZERO),
    });
    assert!(matches!(
        BookmarkLocatorDto::from(&direct_bm),
        BookmarkLocatorDto::Epub {
            target: EpubPointTargetDto::UnitOffset { offset },
            ..
        } if offset == 0.0
    ));
}

#[test]
fn book_navigation_output_conversion_is_zero_copy() {
    let raw_href = String::from("OEBPS/ch1.xhtml");
    let raw_href_ptr = raw_href.as_ptr();
    let raw_fragment = String::from("sec-1");
    let raw_fragment_ptr = raw_fragment.as_ptr();

    let nav = BookNavigation::build(
        vec![TocEntryDraft {
            title: "Chapter 1".to_owned(),
            locator: Some(
                NavigationLocator::epub(
                    Some(0),
                    EpubResourcePath::parse(raw_href).unwrap(),
                    EpubNavTarget::Fragment(EpubFragment::parse(raw_fragment).unwrap()),
                )
                .unwrap(),
            ),
            unit_offset: ReadingUnitOffset::ZERO,
            flags: TocEntryFlags::empty(),
            children: vec![],
        }],
        vec![ReadingUnitDraft {
            is_linear: true,
            weight: 42,
            display_label: Some("OEBPS/ch1.xhtml".to_owned()),
        }],
    )
    .unwrap();
    let title_ptr = nav.toc()[0].title().as_ptr();
    let label_ptr = nav.reading_units()[0].display_label().unwrap().as_ptr();

    let output = BookNavigationOutput::from(nav);
    assert_eq!(output.toc[0].title.as_ptr(), title_ptr);
    assert_eq!(
        output.linear_units[0]
            .display_label
            .as_ref()
            .unwrap()
            .as_ptr(),
        label_ptr
    );
    match &output.toc[0].locator {
        Some(NavigationLocatorDto::Epub {
            href,
            target: EpubNavTargetDto::Fragment { fragment },
            ..
        }) => {
            assert_eq!(href.as_ptr(), raw_href_ptr);
            assert_eq!(fragment.as_ptr(), raw_fragment_ptr);
        }
        other => panic!("unexpected locator dto: {other:?}"),
    }
}
