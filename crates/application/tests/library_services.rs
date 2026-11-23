use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, ErrorKind},
    sync::{Arc, Mutex},
};

use blake3::Hash;
use shiro_application::library::{
    book::{
        commands::RestoreBookIdCommand,
        errors::BookApplicationError,
        file::{errors::BookFileApplicationError, service::BookFileService},
        queries::GetBooksQuery,
        service::BookService,
    },
    errors::LibraryApplicationError,
    ports::{
        BookFileStorageError, BookFileStore, BookRepository, BookRepositoryError, BookTransaction,
        CoverByteStream, TagRepository, TagRepositoryError, TagTransaction,
    },
    tag::service::TagService,
};
use shiro_domain::library::{
    book::{
        entity::{Book, BookChange},
        file::entity::BookFileType,
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
    io::{AsyncRead, AsyncReadExt},
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
    save_error: Mutex<Option<ErrorKind>>,
    open_error: Mutex<Option<ErrorKind>>,
    gate: Mutex<Option<Arc<SaveGate>>>,
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

    fn set_gate(&self, gate: Arc<SaveGate>) {
        *self.gate.lock().unwrap() = Some(gate);
    }
}

impl BookFileStore for MockBookFileStore {
    fn contains(&self, key: &Hash) -> bool {
        self.existing.lock().unwrap().contains(key)
    }

    fn get_type(&self, key: &Hash) -> Option<BookFileType> {
        self.types.lock().unwrap().get(key).copied()
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
        reader.read_to_end(&mut buf).await?;
        self.existing.lock().unwrap().insert(*hash);
        self.types.lock().unwrap().insert(*hash, BookFileType::EPUB);
        Ok(())
    }

    async fn open(&self, _hash: &Hash) -> Result<File, BookFileStorageError> {
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

    async fn remove(&self, hash: &Hash) -> Result<(), BookFileStorageError> {
        self.existing.lock().unwrap().remove(hash);
        self.types.lock().unwrap().remove(hash);
        Ok(())
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
        })
    }
}

struct MockBookTransaction {
    books: Arc<Mutex<Vec<Book>>>,
    occupied_titles: Arc<Mutex<HashSet<String>>>,
}

impl BookTransaction for MockBookTransaction {
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
async fn book_file_service_upload_guard_blocks_concurrent_upload_and_delete_until_release() {
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
        svc.upload_book_file(upload_hash, Cursor::new(upload_data))
            .await
    });

    entered_rx.await.unwrap();

    let dup = service
        .upload_book_file(hash.clone(), Cursor::new(payload))
        .await;
    assert!(matches!(
        dup,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    let del = service.delete_book_file(&hash).await;
    assert!(matches!(
        del,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::UploadConflict
        ))
    ));

    release_tx.send(()).unwrap();
    task.await.unwrap().expect("first upload completes");

    // Once UploadGuard drops, subsequent operations are no longer blocked by UploadConflict.
    service
        .delete_book_file(&hash)
        .await
        .expect("delete succeeds after upload completes");
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
            .upload_book_file(hash.clone(), Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::AlreadyExists
        ))
    ));

    let mismatch = MockBookFileStore::default();
    mismatch.set_save_error(ErrorKind::InvalidData);
    assert!(matches!(
        BookFileService::new(mismatch)
            .upload_book_file(hash.clone(), Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::HashMismatch
        ))
    ));

    let unsupported = MockBookFileStore::default();
    unsupported.set_save_error(ErrorKind::Unsupported);
    assert!(matches!(
        BookFileService::new(unsupported)
            .upload_book_file(hash.clone(), Cursor::new(b"x"))
            .await,
        Err(LibraryApplicationError::BookFile(
            BookFileApplicationError::Unsupported
        ))
    ));

    let io_err = MockBookFileStore::default();
    io_err.set_save_error(ErrorKind::Other);
    assert!(matches!(
        BookFileService::new(io_err)
            .upload_book_file(hash.clone(), Cursor::new(b"x"))
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
