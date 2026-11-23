use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, bail};
use pdfium_render::prelude::Pdfium;
use shiro_application::library::{
    book::{file::service::BookFileService, service::BookService},
    context::LibraryContext,
    tag::service::TagService,
};

use crate::{
    db::pool::connect_library,
    repositories::{
        book_repository::SqliteBookRepository,
        tag_repository::SqliteTagRepository,
    },
    storage::book_file_storage::LocalBookFileStorage,
};

pub type SqliteBookService = BookService<SqliteBookRepository, LocalBookFileStorage>;
pub type SqliteTagService = TagService<SqliteTagRepository>;
pub type SqliteBookFileService = BookFileService<LocalBookFileStorage>;
pub type SqliteLibraryContext =
    LibraryContext<SqliteBookRepository, SqliteTagRepository, LocalBookFileStorage>;

/// Standalone local-mode library runtime.
///
/// Opens a single SQLite library database (`<root>/library.db`) and book file storage
/// (`<root>/books/`) without creating any identity database, user tables, sessions,
/// or HTTP server.
pub struct LocalLibrary {
    root_dir: PathBuf,
    db_path: PathBuf,
    context: SqliteLibraryContext,
}

impl LocalLibrary {
    /// Opens or initializes a local library rooted at `root_dir`.
    ///
    /// Fails explicitly if `root_dir` is a regular file, is read-only/unwritable,
    /// or contains a corrupted `library.db` so callers never silently mask a broken library.
    pub async fn open(
        root_dir: impl Into<PathBuf>,
        pdfium: Option<Arc<Mutex<Pdfium>>>,
    ) -> anyhow::Result<Self> {
        let root_dir = root_dir.into();
        validate_and_prepare_root(&root_dir)?;

        let db_path = root_dir.join("library.db");
        let db = connect_library(&db_path).await?;

        let storage = LocalBookFileStorage::new(root_dir.join("books"), "book", pdfium)
            .with_context(|| {
                format!(
                    "failed to initialize book file storage under {}",
                    root_dir.display()
                )
            })?
            .scan_async()
            .await
            .with_context(|| format!("failed to scan book files under {}", root_dir.display()))?;

        let book_files = Arc::new(BookFileService::new(storage));
        let context = LibraryContext::new(
            SqliteBookRepository::new(db.clone()),
            SqliteTagRepository::new(db.clone()),
            book_files,
        );

        Ok(Self {
            root_dir,
            db_path,
            context,
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn context(&self) -> &SqliteLibraryContext {
        &self.context
    }

    pub fn book_service(&self) -> SqliteBookService {
        self.context.book_service()
    }

    pub fn tag_service(&self) -> SqliteTagService {
        self.context.tag_service()
    }

    pub fn book_file_service(&self) -> &Arc<SqliteBookFileService> {
        self.context.book_file_service()
    }
}

fn validate_and_prepare_root(root_dir: &Path) -> anyhow::Result<()> {
    if root_dir.as_os_str().is_empty() {
        bail!("library root directory path cannot be empty");
    }

    if root_dir.exists() {
        if !root_dir.is_dir() {
            bail!(
                "library root path exists but is not a directory: {}",
                root_dir.display()
            );
        }
    } else {
        fs::create_dir_all(root_dir).with_context(|| {
            format!(
                "failed to create library root directory {}",
                root_dir.display()
            )
        })?;
    }

    // Verify write permission up front before opening SQLite or mutating directory contents.
    let _write_probe = tempfile::NamedTempFile::new_in(root_dir).with_context(|| {
        format!(
            "library root directory is not writable: {}",
            root_dir.display()
        )
    })?;

    Ok(())
}
