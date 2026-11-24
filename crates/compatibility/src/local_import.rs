use std::{
    io::{Cursor, SeekFrom},
    path::Path,
};

use shiro_application::library::{
    book::{commands::CreateBookCommand, file::errors::BookFileApplicationError},
    errors::LibraryApplicationError,
};
use shiro_error_catalog::ErrorCode;
use shiro_infrastructure::{
    library_runtime::LocalLibrary,
    storage::book_file_storage::{detect_book_file_type, extract_book_metadata_title},
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};
use tokio_util::sync::CancellationToken;

pub(crate) const STREAM_CHUNK_BYTES: usize = 64 * 1024;

pub type ImportProgressCallback<'a> = &'a mut (dyn FnMut(u64) + Send);

/// Derives a clean book title from a file name (or display name), stripping `.pdf`/`.epub`.
pub fn derive_book_title(file_name: &str) -> Result<String, ErrorCode> {
    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim();

    let stripped = strip_known_extension(base).trim();
    if stripped.is_empty() {
        return Err(ErrorCode::BookMissingTitle);
    }
    Ok(stripped.to_owned())
}

fn strip_known_extension(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        &name[..name.len() - 4]
    } else if lower.ends_with(".epub") {
        &name[..name.len() - 5]
    } else {
        name
    }
}

/// Returns `true` when `file_name` looks like an opaque Android SAF document identifier
/// (e.g. `"100001"` from `msf:100001`, `"document"`, or `"book"`) without a `.pdf`/`.epub` extension.
pub fn is_opaque_book_file_name(file_name: &str) -> bool {
    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim();
    if base.is_empty() {
        return true;
    }
    let lower = base.to_ascii_lowercase();
    if lower.ends_with(".pdf") || lower.ends_with(".epub") {
        return false;
    }
    if matches!(lower.as_str(), "book" | "document" | "file" | "download") {
        return true;
    }
    let tail = base.rsplit(':').next().unwrap_or(base).trim();
    !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
}

/// Resolves the final book title for a staged import:
/// - Normal file names keep their filename-derived title.
/// - Opaque Android SAF identifiers (such as `"100001"`) attempt to read embedded book metadata
///   (e.g. EPUB `<dc:title>`) and otherwise fall back to `book-<short_hash>`.
pub(crate) async fn resolve_staged_book_title(
    file_name: &str,
    staged: &mut StagedBook,
) -> Result<String, ErrorCode> {
    let initial = derive_book_title(file_name)?;
    if !is_opaque_book_file_name(file_name) {
        return Ok(initial);
    }

    let std_clone = staged
        .file
        .try_clone()
        .await
        .map_err(|_| ErrorCode::CommonInternalServerError)?
        .into_std()
        .await;
    let embedded = tokio::task::spawn_blocking(move || {
        let mut std_file = std_clone;
        extract_book_metadata_title(&mut std_file)
    })
    .await
    .ok()
    .flatten();

    staged
        .file
        .seek(SeekFrom::Start(0))
        .await
        .map_err(|_| ErrorCode::CommonInternalServerError)?;

    if let Some(meta_title) = embedded
        && let Ok(valid) = derive_book_title(&meta_title)
    {
        return Ok(valid);
    }

    let short_hash = &staged.hash[..staged.hash.len().min(8)];
    Ok(format!("book-{short_hash}"))
}

pub(crate) struct StagedBook {
    pub file: File,
    pub hash: String,
    pub size: u64,
}

pub(crate) async fn stage_book<R>(
    temp_dir: &Path,
    mut reader: R,
    size_limit: Option<(u64, ErrorCode)>,
    cancel: &CancellationToken,
    mut on_progress: Option<ImportProgressCallback<'_>>,
) -> Result<StagedBook, ErrorCode>
where
    R: AsyncRead + Unpin + Send,
{
    std::fs::create_dir_all(temp_dir).map_err(|_| ErrorCode::CommonInternalServerError)?;
    let mut file = File::from_std(
        tempfile::tempfile_in(temp_dir).map_err(|_| ErrorCode::CommonInternalServerError)?,
    );

    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; STREAM_CHUNK_BYTES];
    let mut size: u64 = 0;
    // The EPUB magic header spans 58 bytes. Retain up to 64 for both formats.
    let mut header = Vec::with_capacity(64);

    loop {
        let n = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
            result = reader.read(&mut buf) => result.map_err(|_| ErrorCode::ImportReadFailed)?,
        };
        if n == 0 {
            break;
        }

        size = size.saturating_add(n as u64);
        if let Some((max_bytes, limit_error)) = size_limit
            && size > max_bytes
        {
            return Err(limit_error);
        }

        let chunk = &buf[..n];
        hasher.update(chunk);
        header.extend_from_slice(&chunk[..chunk.len().min(64 - header.len())]);
        file.write_all(chunk)
            .await
            .map_err(|_| ErrorCode::CommonInternalServerError)?;

        if let Some(cb) = on_progress.as_deref_mut() {
            cb(size);
        }
    }

    detect_book_file_type(&mut Cursor::new(header)).map_err(|_| ErrorCode::BookFileUnsupported)?;
    file.flush()
        .await
        .map_err(|_| ErrorCode::CommonInternalServerError)?;
    file.seek(SeekFrom::Start(0))
        .await
        .map_err(|_| ErrorCode::CommonInternalServerError)?;

    Ok(StagedBook {
        file,
        hash: hasher.finalize().to_hex().to_string(),
        size,
    })
}

pub async fn import_reader_into_local<R>(
    library: &LocalLibrary,
    file_name: &str,
    reader: R,
    cancel: Option<&CancellationToken>,
    on_progress: Option<ImportProgressCallback<'_>>,
) -> Result<shiro_application::library::book::dto::GetBookOutput, ErrorCode>
where
    R: AsyncRead + Unpin + Send,
{
    let uncancelled = CancellationToken::new();
    let cancel = cancel.unwrap_or(&uncancelled);
    let _ = derive_book_title(file_name)?;
    let temp_dir = library.root_dir().join("books").join(".temp");
    let mut staged = stage_book(&temp_dir, reader, None, cancel, on_progress).await?;
    let title = resolve_staged_book_title(file_name, &mut staged).await?;
    let StagedBook { file, hash, .. } = staged;
    if cancel.is_cancelled() {
        return Err(ErrorCode::ImportCancelled);
    }
    if library
        .book_file_service()
        .parse_existing_hash(&hash)
        .is_err()
    {
        match library
            .book_file_service()
            .upload_book_file(hash.clone(), file)
            .await
        {
            Ok(())
            // Expected dedup race: another concurrent upload already stored the exact hash.
            | Err(LibraryApplicationError::BookFile(BookFileApplicationError::AlreadyExists)) => {}
            Err(err) => return Err(ErrorCode::from(&err)),
        }
    }

    if cancel.is_cancelled() {
        return Err(ErrorCode::ImportCancelled);
    }

    let created = library
        .book_service()
        .create_book(CreateBookCommand { title, hash })
        .await
        .map_err(|err| ErrorCode::from(&err))?;

    Ok(created.into())
}

#[cfg(test)]
mod tests {
    use shiro_http_contract::DEFAULT_MAX_UPLOAD_BYTES;

    use super::*;

    #[tokio::test]
    async fn staging_enforces_optional_net_limit_and_allows_unbounded_reads() {
        assert_eq!(DEFAULT_MAX_UPLOAD_BYTES, 512 * 1024 * 1024);
        const TEST_LIMIT: u64 = 128 * 1024;
        assert!(TEST_LIMIT > STREAM_CHUNK_BYTES as u64);

        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let pdf = include_bytes!("../../../scripts/packaging/fixtures/smoke_pattern.pdf");
        let stream =
            |size| Cursor::new(pdf).chain(tokio::io::repeat(b' ').take(size - pdf.len() as u64));

        let at_limit = stage_book(
            dir.path(),
            stream(TEST_LIMIT),
            Some((TEST_LIMIT, ErrorCode::ImportFileTooLarge)),
            &cancel,
            None,
        )
        .await
        .unwrap();
        assert_eq!(at_limit.size, TEST_LIMIT);
        drop(at_limit);

        let over_limit = stage_book(
            dir.path(),
            stream(TEST_LIMIT + 1),
            Some((TEST_LIMIT, ErrorCode::ImportFileTooLarge)),
            &cancel,
            None,
        )
        .await
        .err();
        assert_eq!(over_limit, Some(ErrorCode::ImportFileTooLarge));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let unbounded = stage_book(
            dir.path(),
            stream(TEST_LIMIT + 1),
            None,
            &cancel,
            None,
        )
        .await
        .unwrap();
        assert_eq!(unbounded.size, TEST_LIMIT + 1);
        drop(unbounded);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let lib_dir = tempfile::tempdir().unwrap();
        let library = LocalLibrary::open(lib_dir.path(), None).await.unwrap();
        let created = import_reader_into_local(
            &library,
            "MultiChunkLocalBook.pdf",
            stream(192 * 1024),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(created.title, "MultiChunkLocalBook");
        assert_eq!(created.file_type, "PDF");
    }

    #[tokio::test]
    async fn resolves_embedded_epub_title_or_hash_fallback_for_opaque_android_saf_names() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();

        let epub = include_bytes!("../../../scripts/packaging/fixtures/smoke_cover.epub");
        let mut staged_epub = stage_book(
            dir.path(),
            Cursor::new(epub),
            None,
            &cancel,
            None,
        )
        .await
        .unwrap();
        let epub_title = resolve_staged_book_title("100001", &mut staged_epub)
            .await
            .unwrap();
        assert_ne!(epub_title, "100001");
        assert!(!epub_title.is_empty());

        let pdf = include_bytes!("../../../scripts/packaging/fixtures/smoke_pattern.pdf");
        let mut staged_pdf = stage_book(
            dir.path(),
            Cursor::new(pdf),
            None,
            &cancel,
            None,
        )
        .await
        .unwrap();
        let pdf_title = resolve_staged_book_title("100001", &mut staged_pdf)
            .await
            .unwrap();
        assert_eq!(pdf_title, format!("book-{}", &staged_pdf.hash[..8]));

        // Explicit real file names keep their filename-derived title unchanged.
        let named_title = resolve_staged_book_title("100001.pdf", &mut staged_pdf)
            .await
            .unwrap();
        assert_eq!(named_title, "100001");
    }

    #[tokio::test]
    async fn local_import_preserves_stored_book_files_when_create_book_fails() {
        let dir = tempfile::tempdir().unwrap();
        let library = LocalLibrary::open(dir.path(), None).await.unwrap();

        let epub = include_bytes!("../../../scripts/packaging/fixtures/smoke_cover.epub");
        let pdf = include_bytes!("../../../scripts/packaging/fixtures/smoke_pattern.pdf");
        let epub_hash = blake3::hash(epub).to_hex().to_string();
        let pdf_hash = blake3::hash(pdf).to_hex().to_string();

        // First import succeeds and stores epub file + cover.
        let created = import_reader_into_local(
            &library,
            "SharedTitle.epub",
            Cursor::new(epub.as_slice()),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(created.title, "SharedTitle");
        assert!(library.book_file_service().parse_existing_hash(&epub_hash).is_ok());

        let books_dir = library.root_dir().join("books");
        let covers_dir = books_dir.join("covers");
        let epub_book_path = books_dir.join(format!("{epub_hash}.book"));
        assert!(epub_book_path.exists());
        let cover_files_with_hash = |hash: &str| -> usize {
            std::fs::read_dir(&covers_dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .path()
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        == Some(hash)
                })
                .count()
        };
        assert_eq!(cover_files_with_hash(&epub_hash), 1);

        // Once validated and persisted, the stored PDF file and cover are preserved even if create_book fails.
        let conflict_new = import_reader_into_local(
            &library,
            "SharedTitle.pdf",
            Cursor::new(pdf.as_slice()),
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(conflict_new, ErrorCode::BookTitleConflict);
        assert!(library.book_file_service().parse_existing_hash(&pdf_hash).is_ok());
        assert!(books_dir.join(format!("{pdf_hash}.book")).exists());
        assert_eq!(cover_files_with_hash(&pdf_hash), 1);

        // Importing an already-stored file with a conflicting title also preserves the existing file.
        let conflict_existing = import_reader_into_local(
            &library,
            "SharedTitle.epub",
            Cursor::new(epub.as_slice()),
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(conflict_existing, ErrorCode::BookTitleConflict);
        assert!(library.book_file_service().parse_existing_hash(&epub_hash).is_ok());
        assert!(epub_book_path.exists());
        assert_eq!(cover_files_with_hash(&epub_hash), 1);
    }
}
