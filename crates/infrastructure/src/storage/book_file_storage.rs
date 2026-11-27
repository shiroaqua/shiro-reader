use std::{
    collections::HashSet,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Context;
use blake3::Hash;
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::{StreamExt, stream};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use pdfium_render::prelude::{
    PdfDocumentMetadataTagType, PdfPageRenderRotation, PdfRenderConfig, Pdfium,
};
use rayon::prelude::*;
use rbook::{Epub, ebook::metadata::TitleKind};
use shiro_application::library::ports::{
    BookFileMetadata, BookFileStorageError, BookFileStore, CoverByteStream,
    ExtractedBookMetadata, OpenedBookFile,
};
use shiro_domain::library::book::{
    navigation::BookNavigation,
    value_objects::{
        BookAuthor, BookDescription, BookFileType, BookIdentifier, BookLanguage, BookPublishedDate,
        BookPublisher, BookSubtitle, MAX_BOOK_AUTHORS,
    },
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, Take},
};
use tokio_util::io::ReaderStream;

use super::book_navigation;

const BOOK_FILE_VERSION: u8 = 1;
const BOOK_FILE_HEADER_SIZE: u64 = 1 + 1 + 32 + 8 + 4;

pub type StorageResult<T> = Result<T, BookFileStorageError>;

pub struct LocalBookFileStorage {
    dir: PathBuf,
    ext: String,
    files: Arc<DashMap<Hash, BookFileMetadata>>,
    covers: Arc<DashMap<Hash, (PathBuf, ImageFormat)>>,
    pdfium: Option<Arc<Mutex<Pdfium>>>,
}

impl LocalBookFileStorage {
    pub fn new(
        dir: PathBuf,
        ext: impl Into<String>,
        pdfium: Option<Arc<Mutex<Pdfium>>>,
    ) -> StorageResult<Self> {
        if dir.exists() && !dir.is_dir() {
            return Err(BookFileStorageError::Io(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("book storage path is not a directory: {}", dir.display()),
            )));
        }
        std::fs::create_dir_all(dir.join("covers"))?;
        // 必须保证临时目录和目标目录在同一文件系统中，否则无法保证原子替换。
        std::fs::create_dir_all(dir.join(".temp"))?;

        Ok(Self {
            dir,
            ext: ext.into(),
            files: Arc::new(DashMap::new()),
            covers: Arc::new(DashMap::new()),
            pdfium,
        })
    }

    fn book_file_path(&self, hash: &Hash) -> PathBuf {
        self.dir
            .join(hash.to_hex().as_str())
            .with_extension(&self.ext)
    }

    pub fn scan(&mut self) -> io::Result<()> {
        let (files, covers) = scan_book_storage(&self.dir, &self.ext)?;
        self.files = Arc::new(files);
        self.covers = Arc::new(covers);
        Ok(())
    }

    pub async fn scan_async(mut self) -> io::Result<Self> {
        let dir = self.dir.clone();
        let ext = self.ext.clone();
        let (files, covers) = tokio::task::spawn_blocking(move || scan_book_storage(&dir, &ext))
            .await
            .map_err(|err| io::Error::other(format!("book storage scan panicked: {err}")))??;
        self.files = Arc::new(files);
        self.covers = Arc::new(covers);
        Ok(self)
    }

    /// Extracts the unified navigation index (TOC and linear units) for a stored book file.
    ///
    /// Returns an empty [`BookNavigation`] if the file is missing or parsing fails,
    /// logging a warning with the failure reason.
    pub async fn extract_navigation(&self, hash: &Hash) -> BookNavigation {
        let Some(meta) = self.get_meta(hash) else {
            tracing::warn!(
                hash = %hash.to_hex(),
                reason = "book file metadata not found in storage",
                "failed to extract book navigation, returning empty navigation"
            );
            return BookNavigation::empty();
        };

        let path = self.book_file_path(hash);
        let pdfium = self.pdfium.clone();
        let hash_hex = hash.to_hex().to_string();
        let log_hash = hash_hex.clone();
        let log_path = path.display().to_string();

        match tokio::task::spawn_blocking(move || {
            extract_book_navigation_from_file(&path, meta, pdfium.as_deref(), &hash_hex)
        })
        .await
        {
            Ok(Ok(navigation)) => navigation,
            Ok(Err(err)) => {
                tracing::warn!(
                    hash = %log_hash,
                    file_type = %meta.file_type,
                    path = %log_path,
                    reason = %format!("{err:#}"),
                    "failed to extract book navigation, returning empty navigation"
                );
                BookNavigation::empty()
            }
            Err(err) => {
                tracing::warn!(
                    hash = %log_hash,
                    file_type = %meta.file_type,
                    path = %log_path,
                    reason = %err,
                    "book navigation extraction task failed, returning empty navigation"
                );
                BookNavigation::empty()
            }
        }
    }

    /// Best-effort extracts embedded descriptive metadata from a stored book file.
    ///
    /// Returns an empty [`ExtractedBookMetadata`] if the file is missing or parsing fails.
    pub async fn extract_metadata(&self, hash: &Hash) -> ExtractedBookMetadata {
        let Some(meta) = self.get_meta(hash) else {
            tracing::warn!(
                hash = %hash.to_hex(),
                reason = "book file metadata not found in storage",
                "failed to extract book metadata, returning empty metadata"
            );
            return ExtractedBookMetadata::default();
        };

        let path = self.book_file_path(hash);
        let pdfium = self.pdfium.clone();
        let log_hash = hash.to_hex().to_string();
        let log_path = path.display().to_string();

        match tokio::task::spawn_blocking(move || {
            extract_book_metadata_from_file(&path, meta, pdfium.as_deref())
        })
        .await
        {
            Ok(Ok(extracted)) => extracted,
            Ok(Err(err)) => {
                tracing::warn!(
                    hash = %log_hash,
                    file_type = %meta.file_type,
                    path = %log_path,
                    reason = %format!("{err:#}"),
                    "failed to extract book metadata, returning empty metadata"
                );
                ExtractedBookMetadata::default()
            }
            Err(err) => {
                tracing::warn!(
                    hash = %log_hash,
                    file_type = %meta.file_type,
                    path = %log_path,
                    reason = %err,
                    "book metadata extraction task failed, returning empty metadata"
                );
                ExtractedBookMetadata::default()
            }
        }
    }
}

impl BookFileStore for LocalBookFileStorage {
    type Reader = Take<File>;

    fn get_meta(&self, key: &Hash) -> Option<BookFileMetadata> {
        // 书籍永不删除，因此这边的线程安全问题无需在意。
        self.files.get(key).map(|entry| *entry.value())
    }

    async fn import<R>(&self, hash: &Hash, mut reader: R) -> Result<(), BookFileStorageError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let temp_dir = self.dir.join(".temp");
        let mut temp_file = tempfile::NamedTempFile::new_in(&temp_dir)?;
        let std_writer = temp_file.as_file().try_clone()?;
        let mut async_writer = File::from_std(std_writer);

        // 预留文件头空间，finalize 阶段原地回填，避免整本文件二次磁盘拷贝。
        async_writer
            .write_all(&[0u8; BOOK_FILE_HEADER_SIZE as usize])
            .await?;

        let mut hasher = blake3::Hasher::new();
        let mut file_size = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let bytes_read = reader.read(&mut buf).await?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buf[..bytes_read];
            hasher.update(chunk);
            async_writer.write_all(chunk).await?;
            file_size += bytes_read as u64;
        }
        async_writer.flush().await?;
        drop(async_writer);

        let file_hash = hasher.finalize();
        if file_hash != *hash {
            return Err(BookFileStorageError::HashMismatch);
        }

        let dir = self.dir.clone();
        let ext = self.ext.clone();
        let pdfium = self.pdfium.clone();
        let files = Arc::clone(&self.files);
        let covers = Arc::clone(&self.covers);
        tokio::task::spawn_blocking(move || {
            let temp_dir = dir.join(".temp");

            // 独立作用域确保克隆句柄在回填 Header 和 persist_noclobber 前关闭（兼容 Windows）。
            let (file_type, page_count, extracted_cover) = {
                let mut content = BookFileContentReader::new(
                    temp_file.as_file().try_clone()?,
                    BOOK_FILE_HEADER_SIZE,
                    file_size,
                )?;

                let file_type = match detect_book_file_type(&mut content) {
                    Ok(file_type) => file_type,
                    Err(err) if err.kind() == io::ErrorKind::Unsupported => {
                        return Err(BookFileStorageError::Unsupported);
                    }
                    Err(err) => return Err(BookFileStorageError::Io(err)),
                };

                content.seek(SeekFrom::Start(0))?;
                let (page_count, cover) = match file_type {
                    BookFileType::PDF => match pdfium.as_deref() {
                        Some(pdfium) => inspect_pdf_on_import(content, pdfium),
                        None => (None, Err(anyhow::anyhow!("Pdfium library is not loaded"))),
                    },
                    BookFileType::EPUB => (None, extract_epub_cover(content)),
                };
                let cover =
                    cover.and_then(|bytes| Ok((image::guess_format(&bytes)?, Bytes::from(bytes))));

                (file_type, page_count, cover)
            };

            let (cover_format, cover) = match extracted_cover {
                Ok(pair) => pair,
                Err(err) => {
                    tracing::warn!(
                        hash = %file_hash.to_hex(),
                        file_type = %file_type,
                        error = %err,
                        "failed to extract cover image, falling back to blank PNG cover"
                    );
                    (ImageFormat::Png, blank_cover())
                }
            };

            let mut header = [0u8; BOOK_FILE_HEADER_SIZE as usize];
            header[0] = BOOK_FILE_VERSION;
            header[1] = match file_type {
                BookFileType::PDF => 0,
                BookFileType::EPUB => 1,
            };
            header[2..34].copy_from_slice(file_hash.as_bytes());
            header[34..42].copy_from_slice(&file_size.to_be_bytes());
            header[42..46].copy_from_slice(&page_count.unwrap_or(0).to_be_bytes());

            temp_file.seek(SeekFrom::Start(0))?;
            temp_file.write_all(&header)?;
            temp_file.as_file_mut().flush()?;

            let hash_hex = file_hash.to_hex().to_string();
            let cover_path = dir
                .join("covers")
                .join(&hash_hex)
                .with_extension(cover_format.extensions_str()[0]);
            let mut temp_cover_file = tempfile::NamedTempFile::new_in(&temp_dir)?;
            temp_cover_file.write_all(&cover)?;
            temp_cover_file.as_file_mut().flush()?;
            drop(cover);

            match temp_cover_file.persist_noclobber(&cover_path) {
                Ok(_) => {}
                Err(err) if err.error.kind() == io::ErrorKind::AlreadyExists => {
                    tracing::debug!(path = %cover_path.display(), "reusing existing cover file");
                }
                Err(err) => return Err(BookFileStorageError::Io(err.error)),
            }

            let final_path = dir.join(&hash_hex).with_extension(ext);
            match temp_file.persist_noclobber(&final_path) {
                Ok(_) => {
                    covers.insert(file_hash, (cover_path, cover_format));
                    files.insert(
                        file_hash,
                        BookFileMetadata {
                            file_type,
                            size: file_size,
                            page_count,
                        },
                    );
                    Ok(())
                }
                Err(err) if err.error.kind() == io::ErrorKind::AlreadyExists => {
                    Err(BookFileStorageError::AlreadyExists)
                }
                Err(err) => Err(BookFileStorageError::Io(err.error)),
            }
        })
        .await
        .map_err(|err| {
            BookFileStorageError::Io(io::Error::other(format!(
                "book file finalization task failed: {err}"
            )))
        })?
    }

    async fn open_range(
        &self,
        hash: &Hash,
        offset: u64,
        length: Option<u64>,
    ) -> Result<OpenedBookFile<Self::Reader>, BookFileStorageError> {
        let meta = self.get_meta(hash).ok_or(BookFileStorageError::NotFound)?;
        let is_empty_req = length == Some(0) || (meta.size == 0 && offset == 0 && length.is_none());
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
        let physical_offset = BOOK_FILE_HEADER_SIZE.checked_add(offset).ok_or_else(|| {
            BookFileStorageError::Io(io::Error::from(io::ErrorKind::InvalidInput))
        })?;

        let path = self.book_file_path(hash);
        let mut file = match File::open(&path).await {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(BookFileStorageError::NotFound);
            }
            Err(err) => return Err(BookFileStorageError::Io(err)),
        };

        let mut version = [0u8; 1];
        file.read_exact(&mut version).await?;
        if version[0] != BOOK_FILE_VERSION {
            return Err(BookFileStorageError::Unsupported);
        }

        file.seek(SeekFrom::Start(physical_offset)).await?;
        Ok(OpenedBookFile {
            reader: file.take(actual_len),
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
        let meta = self.get_meta(hash).ok_or(BookFileStorageError::NotFound)?;
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
        if offset
            .checked_add(len_u64)
            .is_none_or(|end| end > meta.size)
        {
            return Err(BookFileStorageError::RangeOutOfBounds {
                offset,
                len: Some(len_u64),
                size: meta.size,
            });
        }

        let physical_offset = BOOK_FILE_HEADER_SIZE.checked_add(offset).ok_or_else(|| {
            BookFileStorageError::Io(io::Error::from(io::ErrorKind::InvalidInput))
        })?;
        let path = self.book_file_path(hash);

        tokio::task::spawn_blocking(move || read_book_file_exact_at(&path, physical_offset, len))
            .await
            .map_err(|err| {
                BookFileStorageError::Io(io::Error::other(format!(
                    "book file positional read task failed: {err}"
                )))
            })?
    }

    async fn open_cover(
        &self,
        key: Hash,
    ) -> Result<(CoverByteStream, String), BookFileStorageError> {
        let cover = self.covers.get(&key).map(|cover| cover.value().clone());
        if let Some((path, format)) = cover {
            match File::open(path).await {
                Ok(file) => {
                    return Ok((
                        ReaderStream::new(file).boxed(),
                        format.to_mime_type().to_owned(),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        let data = blank_cover();
        let stream = stream::once(async move { Ok(data) });
        Ok((stream.boxed(), ImageFormat::Png.to_mime_type().to_owned()))
    }

    async fn extract_navigation(&self, hash: &Hash) -> BookNavigation {
        LocalBookFileStorage::extract_navigation(self, hash).await
    }

    async fn extract_metadata(&self, hash: &Hash) -> ExtractedBookMetadata {
        LocalBookFileStorage::extract_metadata(self, hash).await
    }
}

fn extract_book_navigation_from_file(
    path: &Path,
    meta: BookFileMetadata,
    pdfium: Option<&Mutex<Pdfium>>,
    hash_hex: &str,
) -> anyhow::Result<BookNavigation> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to open book file at {}", path.display()))?;
    let mut version = [0u8; 1];
    file.read_exact(&mut version)
        .with_context(|| format!("failed to read book file header version at {}", path.display()))?;
    if version[0] != BOOK_FILE_VERSION {
        anyhow::bail!(
            "unsupported book file container version {} (expected {}) at {}",
            version[0],
            BOOK_FILE_VERSION,
            path.display()
        );
    }

    let content = BookFileContentReader::new(file, BOOK_FILE_HEADER_SIZE, meta.size)
        .with_context(|| format!("failed to seek book payload at {}", path.display()))?;
    match meta.file_type {
        BookFileType::EPUB => book_navigation::extract_epub_navigation(content, hash_hex),
        BookFileType::PDF => match pdfium {
            Some(pdfium) => book_navigation::extract_pdf_navigation(content, pdfium, hash_hex),
            None => Err(anyhow::anyhow!("Pdfium library is not loaded")),
        },
    }
}

type ScannedBookStorage = (
    DashMap<Hash, BookFileMetadata>,
    DashMap<Hash, (PathBuf, ImageFormat)>,
);

fn scan_book_storage(dir: &Path, ext: &str) -> io::Result<ScannedBookStorage> {
    let files = Arc::new(DashMap::new());
    std::fs::read_dir(dir)?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.is_file()
                && path.file_stem().is_some()
                && path.extension().and_then(|s| s.to_str()) == Some(ext))
            .then_some(path)
        })
        .par_bridge()
        .for_each(|path| {
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut hasher = blake3::Hasher::new();
            let mut file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!("Skipped: Cannot open file {}: {}", file_name, e);
                    return;
                }
            };
            let mut version = [0u8; 1];
            if file.read_exact(&mut version).is_err() || version[0] != BOOK_FILE_VERSION {
                tracing::warn!("Skipped: Unsupported book file version: {}", file_name);
                return;
            }
            let file_type = match read_file_type_from_book_file(&mut file) {
                Ok(ft) => ft,
                Err(e) => {
                    tracing::warn!("Skipped: Cannot read file type from {}: {}", file_name, e);
                    return;
                }
            };
            let file_hash = match read_file_hash_from_book_file(&mut file) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("Skipped: Cannot read file hash from {}: {}", file_name, e);
                    return;
                }
            };
            let mut size_bytes = [0u8; 8];
            if let Err(e) = file.read_exact(&mut size_bytes) {
                tracing::warn!("Skipped: Cannot read file size from {}: {}", file_name, e);
                return;
            }
            let file_size = u64::from_be_bytes(size_bytes);

            let mut page_count_bytes = [0u8; 4];
            if let Err(e) = file.read_exact(&mut page_count_bytes) {
                tracing::warn!("Skipped: Cannot read page count from {}: {}", file_name, e);
                return;
            }
            let raw_page_count = u32::from_be_bytes(page_count_bytes);
            let page_count = match file_type {
                BookFileType::PDF => (raw_page_count > 0).then_some(raw_page_count),
                BookFileType::EPUB => None,
            };

            let actual_size = match std::io::copy(
                &mut Read::by_ref(&mut file).take(file_size),
                &mut hasher,
            ) {
                Ok(size) => size,
                Err(e) => {
                    tracing::warn!("Skipped: Invalid file {}: {}", file_name, e);
                    return;
                }
            };
            let has_trailing_data = match file.read(&mut [0u8; 1]) {
                Ok(size) => size != 0,
                Err(e) => {
                    tracing::warn!("Skipped: Cannot validate file length for {}: {}", file_name, e);
                    return;
                }
            };

            let calculated_hash = hasher.finalize();
            if !has_trailing_data
                && file_size == actual_size
                && file_hash == calculated_hash
                && path.file_stem().and_then(|stem| stem.to_str())
                    == Some(file_hash.to_hex().as_str())
            {
                files.insert(
                    calculated_hash,
                    BookFileMetadata {
                        file_type,
                        size: file_size,
                        page_count,
                    },
                );
                tracing::debug!("loaded: {}", file_name);
            } else {
                tracing::warn!(
                    "Skipped: Book file integrity mismatch: {}. Expected: {} ({} bytes), Calculated: {} ({} bytes)",
                    file_name,
                    file_hash.to_hex(),
                    file_size,
                    calculated_hash.to_hex(),
                    actual_size
                );
            }
        });

    let files = Arc::try_unwrap(files).expect("scan owns all handles to files map");
    tracing::info!("Loaded {} books", files.len());

    let covers = DashMap::new();
    for entry in std::fs::read_dir(dir.join("covers"))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(hash) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| Hash::from_hex(stem).ok())
        else {
            continue;
        };
        let format = std::fs::File::open(&path)
            .and_then(|file| {
                image::ImageReader::new(std::io::BufReader::new(file)).with_guessed_format()
            })
            .ok()
            .and_then(|reader| reader.format());
        if let Some(format) = format {
            covers.insert(hash, (path, format));
        }
    }

    Ok((files, covers))
}

fn read_book_file_exact_at(
    path: &Path,
    physical_offset: u64,
    len: usize,
) -> Result<Bytes, BookFileStorageError> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(BookFileStorageError::NotFound);
        }
        Err(err) => return Err(BookFileStorageError::Io(err)),
    };

    let mut version = [0u8; 1];
    file.read_exact(&mut version)?;
    if version[0] != BOOK_FILE_VERSION {
        return Err(BookFileStorageError::Unsupported);
    }

    let mut buf = vec![0u8; len];
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(&mut buf, physical_offset)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut read = 0usize;
        while read < len {
            let n = file.seek_read(&mut buf[read..], physical_offset + read as u64)?;
            if n == 0 {
                return Err(BookFileStorageError::Io(io::Error::from(
                    io::ErrorKind::UnexpectedEof,
                )));
            }
            read += n;
        }
    }
    Ok(Bytes::from(buf))
}

fn read_file_hash_from_book_file(file: &mut std::fs::File) -> io::Result<Hash> {
    let mut buf = [0u8; 32];
    file.read_exact(&mut buf)?;
    Ok(Hash::from_bytes(buf))
}

fn read_file_type_from_book_file(file: &mut std::fs::File) -> io::Result<BookFileType> {
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf)?;
    match buf[0] {
        0 => Ok(BookFileType::PDF),
        1 => Ok(BookFileType::EPUB),
        _ => Err(io::Error::from(io::ErrorKind::Unsupported)),
    }
}

fn inspect_pdf_on_import(
    file: BookFileContentReader,
    pdfium: &Mutex<Pdfium>,
) -> (Option<u32>, anyhow::Result<Vec<u8>>) {
    let Ok(pdfium) = pdfium.lock() else {
        return (None, Err(anyhow::anyhow!("Pdfium lock poisoned")));
    };
    let doc = match pdfium.load_pdf_from_reader(file, None) {
        Ok(doc) => doc,
        Err(err) => return (None, Err(err.into())),
    };
    let page_count = u32::try_from(doc.pages().len()).ok().filter(|&n| n > 0);
    let cover_res = (|| -> anyhow::Result<Vec<u8>> {
        let page = doc.pages().first()?;
        let render_config = PdfRenderConfig::new()
            .set_target_width(2000)
            .set_maximum_height(2000)
            .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);
        let bitmap = page.render_with_config(&render_config)?;
        let image = bitmap.as_image()?;

        let mut png_bytes = Vec::new();
        image.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)?;
        Ok(png_bytes)
    })();
    (page_count, cover_res)
}

fn extract_book_metadata_from_file(
    path: &Path,
    meta: BookFileMetadata,
    pdfium: Option<&Mutex<Pdfium>>,
) -> anyhow::Result<ExtractedBookMetadata> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to open book file at {}", path.display()))?;
    let mut version = [0u8; 1];
    file.read_exact(&mut version)
        .with_context(|| format!("failed to read book file header version at {}", path.display()))?;
    if version[0] != BOOK_FILE_VERSION {
        anyhow::bail!(
            "unsupported book file container version {} (expected {}) at {}",
            version[0],
            BOOK_FILE_VERSION,
            path.display()
        );
    }

    let content = BookFileContentReader::new(file, BOOK_FILE_HEADER_SIZE, meta.size)
        .with_context(|| format!("failed to seek book payload at {}", path.display()))?;
    match meta.file_type {
        BookFileType::EPUB => extract_epub_metadata(content),
        BookFileType::PDF => match pdfium {
            Some(pdfium) => extract_pdf_metadata(content, pdfium),
            None => Ok(ExtractedBookMetadata::default()),
        },
    }
}

fn extract_epub_metadata(content: BookFileContentReader) -> anyhow::Result<ExtractedBookMetadata> {
    let epub = Epub::read(content).context("failed to parse EPUB archive")?;
    let meta = epub.metadata();

    let subtitle = meta
        .titles()
        .filter(|entry| entry.kind() == TitleKind::Subtitle)
        .find_map(|entry| {
            let cleaned = sanitize_singleline_text(entry.value());
            BookSubtitle::parse(cleaned).ok()
        });

    let authors = extract_epub_authors(&meta);

    let publisher = meta
        .publishers()
        .find_map(|entry| {
            let cleaned = sanitize_singleline_text(entry.value());
            BookPublisher::parse(cleaned).ok()
        });

    let published_date = extract_epub_published_date(&meta);

    let language = meta.languages().find_map(|entry| {
        let normalized = entry.value().trim().replace('_', "-");
        BookLanguage::parse(normalized).ok()
    });

    let description = meta.descriptions().find_map(|entry| {
        let cleaned = sanitize_multiline_text(entry.value());
        BookDescription::parse(cleaned).ok()
    });

    let identifier = meta
        .identifier()
        .into_iter()
        .chain(meta.identifiers())
        .find_map(|entry| {
            let raw = entry.value().trim();
            if raw.is_empty() {
                return None;
            }
            if let Ok(parsed) = BookIdentifier::parse(raw) {
                return Some(parsed);
            }
            if let Some(scheme) = entry.scheme() {
                let code = scheme.code().trim();
                if !code.is_empty()
                    && let Ok(parsed) = BookIdentifier::parse(format!("{code}:{raw}"))
                {
                    return Some(parsed);
                }
            }
            None
        });

    Ok(ExtractedBookMetadata {
        subtitle,
        authors,
        publisher,
        published_date,
        language,
        description,
        identifier,
    })
}

fn extract_epub_authors(metadata: &rbook::epub::metadata::EpubMetadata<'_>) -> Vec<BookAuthor> {
    fn collect_authors<'a>(
        entries: impl Iterator<Item = rbook::epub::metadata::EpubContributor<'a>>,
    ) -> Vec<BookAuthor> {
        let mut seen = HashSet::new();
        let mut authors = Vec::new();
        for entry in entries {
            let cleaned = sanitize_singleline_text(entry.value());
            if let Ok(author) = BookAuthor::parse(cleaned)
                && seen.insert(author.clone())
            {
                authors.push(author);
                if authors.len() >= MAX_BOOK_AUTHORS {
                    break;
                }
            }
        }
        authors
    }

    let primary = collect_authors(metadata.creators().filter(|entry| {
        let mut roles = entry.roles().peekable();
        roles.peek().is_none() || roles.any(|role| role.code().trim().eq_ignore_ascii_case("aut"))
    }));
    if !primary.is_empty() {
        return primary;
    }
    collect_authors(metadata.creators())
}

fn extract_epub_published_date(
    metadata: &rbook::epub::metadata::EpubMetadata<'_>,
) -> Option<BookPublishedDate> {
    if let Some(entry) = metadata.published_entry() {
        let raw = entry.value().trim();
        if let Ok(parsed) = BookPublishedDate::parse(raw) {
            return Some(parsed);
        }
        if raw.len() > 10
            && matches!(raw.as_bytes()[10], b'T' | b't' | b' ')
            && let Ok(parsed) = BookPublishedDate::parse(&raw[..10])
        {
            return Some(parsed);
        }
    }
    let dt = metadata.published()?;
    let d = dt.date();
    BookPublishedDate::parse(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())).ok()
}

fn extract_pdf_metadata(
    content: BookFileContentReader,
    pdfium: &Mutex<Pdfium>,
) -> anyhow::Result<ExtractedBookMetadata> {
    let pdfium = pdfium
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium lock poisoned"))?;
    let doc = pdfium
        .load_pdf_from_reader(content, None)
        .context("failed to load PDF document with Pdfium")?;
    let meta = doc.metadata();

    let authors = meta
        .get(PdfDocumentMetadataTagType::Author)
        .map(|tag| extract_pdf_authors(tag.value()))
        .unwrap_or_default();

    let description = meta
        .get(PdfDocumentMetadataTagType::Subject)
        .map(|tag| sanitize_multiline_text(tag.value()))
        .and_then(|cleaned| BookDescription::parse(cleaned).ok());

    let published_date = meta
        .get(PdfDocumentMetadataTagType::CreationDate)
        .and_then(|tag| parse_pdf_creation_date(tag.value()));

    Ok(ExtractedBookMetadata {
        subtitle: None,
        authors,
        publisher: None,
        published_date,
        language: None,
        description,
        identifier: None,
    })
}

fn extract_pdf_authors(raw: &str) -> Vec<BookAuthor> {
    let mut seen = HashSet::new();
    let mut authors = Vec::new();
    for part in raw.split([';', '；']) {
        let cleaned = sanitize_singleline_text(part);
        if let Ok(author) = BookAuthor::parse(cleaned)
            && seen.insert(author.clone())
        {
            authors.push(author);
            if authors.len() >= MAX_BOOK_AUTHORS {
                break;
            }
        }
    }
    authors
}

fn parse_pdf_creation_date(raw: &str) -> Option<BookPublishedDate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed
        .strip_prefix("D:")
        .or_else(|| trimmed.strip_prefix("d:"))
        .unwrap_or(trimmed);

    if let Ok(parsed) = BookPublishedDate::parse(stripped) {
        return Some(parsed);
    }
    if stripped.len() > 10
        && matches!(stripped.as_bytes()[10], b'T' | b't' | b' ')
        && let Ok(parsed) = BookPublishedDate::parse(&stripped[..10])
    {
        return Some(parsed);
    }

    let digits: String = stripped
        .bytes()
        .take_while(u8::is_ascii_digit)
        .take(8)
        .map(char::from)
        .collect();
    if digits.len() >= 8 {
        BookPublishedDate::parse(format!(
            "{}-{}-{}",
            &digits[0..4],
            &digits[4..6],
            &digits[6..8]
        ))
        .ok()
    } else if digits.len() >= 6 {
        BookPublishedDate::parse(format!("{}-{}", &digits[0..4], &digits[4..6])).ok()
    } else if digits.len() >= 4 {
        BookPublishedDate::parse(&digits[0..4]).ok()
    } else {
        None
    }
}

fn sanitize_singleline_text(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut prev_space = true;
    for ch in raw.chars() {
        if ch.is_control() || ch.is_whitespace() {
            if !prev_space {
                normalized.push(' ');
                prev_space = true;
            }
        } else {
            normalized.push(ch);
            prev_space = false;
        }
    }
    if normalized.ends_with(' ') {
        normalized.pop();
    }
    normalized
}

fn sanitize_multiline_text(raw: &str) -> String {
    let normalized_newlines = raw.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned: String = normalized_newlines
        .chars()
        .filter(|&c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    cleaned.trim().to_owned()
}

fn extract_epub_cover(file: BookFileContentReader) -> anyhow::Result<Vec<u8>> {
    let epub = Epub::read(file)?;
    if let Some(cover) = epub.manifest().cover_image()
        && let Ok(bytes) = cover.read_bytes()
        && image::guess_format(&bytes).is_ok()
    {
        return Ok(bytes);
    }

    // 兜底策略 1：扫描封面 Spine 章节（或前 3 个 Spine 章节）中的首张 <img> / <image>
    let mut candidate_indices: Vec<usize> = Vec::new();
    let spine_entries: Vec<_> = epub.spine().iter().collect();
    for (idx, entry) in spine_entries.iter().enumerate() {
        if candidate_indices.len() >= 3 {
            break;
        }
        let is_cover_like = entry.idref().to_ascii_lowercase().contains("cover")
            || entry.manifest_entry().is_some_and(|m| {
                let path_lower = m.href().path().decode().to_ascii_lowercase();
                let id_lower = m.id().to_ascii_lowercase();
                id_lower.contains("cover")
                    || path_lower.contains("cover")
                    || path_lower.contains("titlepage")
            });
        if is_cover_like {
            candidate_indices.push(idx);
        }
    }
    if !spine_entries.is_empty() && !candidate_indices.contains(&0) && candidate_indices.len() < 3 {
        candidate_indices.push(0);
    }
    for idx in 0..spine_entries.len().min(3) {
        if candidate_indices.len() >= 3 {
            break;
        }
        if !candidate_indices.contains(&idx) {
            candidate_indices.push(idx);
        }
    }

    for idx in candidate_indices {
        let Some(manifest_entry) = spine_entries[idx].manifest_entry() else {
            continue;
        };
        let markup = match manifest_entry.read_str() {
            Ok(text) => text,
            Err(_) => match manifest_entry.read_bytes() {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => continue,
            },
        };
        let base_href = manifest_entry.href().path().as_str();
        for img_ref in extract_markup_image_refs(&markup) {
            if let Some(resolved) = resolve_epub_relative_path(base_href, &img_ref)
                && let Some(bytes) = find_manifest_image_bytes(&epub, &resolved)
            {
                return Ok(bytes);
            }
        }
    }

    // 兜底策略 2：在 manifest 图片项中按文件名/ID 匹配 cover
    let mut partial_matches = Vec::new();
    for image_entry in epub.manifest().images() {
        let decoded_path = image_entry.href().path().decode();
        let file_name = decoded_path.rsplit('/').next().unwrap_or(decoded_path.as_ref());
        let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
        if stem.eq_ignore_ascii_case("cover") {
            if let Ok(bytes) = image_entry.read_bytes()
                && image::guess_format(&bytes).is_ok()
            {
                return Ok(bytes);
            }
        } else if file_name.to_ascii_lowercase().contains("cover")
            || image_entry.id().to_ascii_lowercase().contains("cover")
        {
            partial_matches.push(image_entry);
        }
    }
    for image_entry in partial_matches {
        if let Ok(bytes) = image_entry.read_bytes()
            && image::guess_format(&bytes).is_ok()
        {
            return Ok(bytes);
        }
    }

    anyhow::bail!("epub does not declare a cover image")
}

fn find_manifest_image_bytes(epub: &Epub, resolved_path: &str) -> Option<Vec<u8>> {
    let target_norm = normalize_epub_archive_path(resolved_path)?;
    let target_decoded = percent_decode_utf8_lossy(&target_norm);

    if let Some(entry) = epub.manifest().by_href(&target_norm)
        && entry.media_type().starts_with("image/")
        && let Ok(bytes) = entry.read_bytes()
        && image::guess_format(&bytes).is_ok()
    {
        return Some(bytes);
    }

    for entry in epub.manifest().images() {
        let raw_path = entry.href().path().as_str();
        let decoded_path = entry.href().path().decode();
        let raw_norm = normalize_epub_archive_path(raw_path);
        let decoded_norm = normalize_epub_archive_path(decoded_path.as_ref());

        let matches = raw_norm
            .as_deref()
            .is_some_and(|p| p == target_norm || p.eq_ignore_ascii_case(&target_norm))
            || decoded_norm
                .as_deref()
                .is_some_and(|p| p == target_decoded || p.eq_ignore_ascii_case(&target_decoded));

        if matches
            && let Ok(bytes) = entry.read_bytes()
            && image::guess_format(&bytes).is_ok()
        {
            return Some(bytes);
        }
    }

    None
}

fn extract_markup_image_refs(markup: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let lower = markup.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut cursor = 0usize;

    while let Some(rel_lt) = lower[cursor..].find('<') {
        let tag_start = cursor + rel_lt;
        let rest_bytes = &bytes[tag_start + 1..];
        let is_img = rest_bytes.starts_with(b"img")
            && rest_bytes.get(3).is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>');
        let is_svg_image = (rest_bytes.starts_with(b"image")
            && rest_bytes
                .get(5)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>'))
            || (rest_bytes.starts_with(b"svg:image")
                && rest_bytes
                    .get(9)
                    .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>'));

        if !is_img && !is_svg_image {
            cursor = tag_start + 1;
            continue;
        }

        let Some(tag_end) = find_tag_end(markup, tag_start + 1) else {
            break;
        };
        let tag_slice = &markup[tag_start + 1..tag_end];
        let attr_candidates: &[&str] = if is_img {
            &["src"]
        } else {
            &["href", "xlink:href"]
        };
        for attr_name in attr_candidates {
            if let Some(val) = extract_tag_attr_value(tag_slice, attr_name) {
                let decoded = val.replace("&amp;", "&");
                let trimmed = decoded.trim();
                if !trimmed.is_empty() {
                    refs.push(trimmed.to_owned());
                    break;
                }
            }
        }
        cursor = tag_end + 1;
    }

    refs
}

fn find_tag_end(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

fn extract_tag_attr_value(tag_slice: &str, target_attr: &str) -> Option<String> {
    let bytes = tag_slice.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' && bytes[i] != b'>' {
        i += 1;
    }

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'/' {
            i += 1;
        }
        let attr_name = &tag_slice[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let value = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let val = &tag_slice[val_start..i];
            if i < bytes.len() {
                i += 1;
            }
            val
        } else {
            let val_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                i += 1;
            }
            &tag_slice[val_start..i]
        };

        if attr_name.eq_ignore_ascii_case(target_attr) {
            return Some(value.to_owned());
        }
    }

    None
}

fn resolve_epub_relative_path(base_href: &str, relative: &str) -> Option<String> {
    let clean_rel = relative.split(['?', '#']).next().unwrap_or(relative).trim();
    if clean_rel.is_empty() {
        return None;
    }
    let lower = clean_rel.to_ascii_lowercase();
    if lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
    {
        return None;
    }

    let combined = if clean_rel.starts_with('/') {
        clean_rel.to_owned()
    } else {
        let base_clean = base_href.split(['?', '#']).next().unwrap_or(base_href);
        let base_norm = base_clean.trim_start_matches('/');
        match base_norm.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{clean_rel}"),
            None => clean_rel.to_owned(),
        }
    };

    normalize_epub_archive_path(&combined)
}

fn normalize_epub_archive_path(raw: &str) -> Option<String> {
    let clean = raw.split(['?', '#']).next().unwrap_or(raw).replace('\\', "/");
    let mut stack: Vec<&str> = Vec::new();
    for seg in clean.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            stack.pop()?;
            continue;
        }
        stack.push(seg);
    }
    if stack.is_empty() {
        None
    } else {
        Some(stack.join("/"))
    }
}

fn percent_decode_utf8_lossy(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Exposes only the book payload while keeping reads and seeks on the original file.
pub(super) struct BookFileContentReader {
    file: std::fs::File,
    start: u64,
    len: u64,
    position: u64,
}

impl BookFileContentReader {
    pub(super) fn new(mut file: std::fs::File, start: u64, len: u64) -> io::Result<Self> {
        file.seek(SeekFrom::Start(start))?;
        Ok(Self {
            file,
            start,
            len,
            position: 0,
        })
    }
}

impl Read for BookFileContentReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.len.saturating_sub(self.position);
        if remaining == 0 {
            return Ok(0);
        }

        let count = (&mut self.file).take(remaining).read(buf)?;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for BookFileContentReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => position,
            SeekFrom::End(offset) => seek_offset(self.len, offset)?,
            SeekFrom::Current(offset) => seek_offset(self.position, offset)?,
        };
        let physical_position = self
            .start
            .checked_add(position)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        self.file.seek(SeekFrom::Start(physical_position))?;
        self.position = position;
        Ok(position)
    }
}

fn seek_offset(base: u64, offset: i64) -> io::Result<u64> {
    if offset >= 0 {
        base.checked_add(offset as u64)
    } else {
        base.checked_sub(offset.unsigned_abs())
    }
    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
}

/// Attempts to extract an embedded document title from a staged book file (e.g. EPUB `<dc:title>`)
/// when the source URI (such as an Android SAF `msf:<id>` pipe) does not carry a real file name.
pub fn extract_book_metadata_title(file: &mut std::fs::File) -> Option<String> {
    let file_type = detect_book_file_type(file).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    match file_type {
        BookFileType::EPUB => {
            let epub = Epub::read(file.try_clone().ok()?).ok()?;
            epub.metadata()
                .title()
                .map(|title| title.value().trim().to_owned())
                .filter(|t| !t.is_empty())
        }
        BookFileType::PDF => None,
    }
}

fn blank_cover() -> Bytes {
    static BLANK_COVER: OnceLock<Bytes> = OnceLock::new();
    BLANK_COVER
        .get_or_init(|| {
            let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_pixel(1000, 1600, Rgba([255, 255, 255, 255]));

            let mut buf = Vec::new();
            DynamicImage::ImageRgba8(img)
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .expect("encode blank cover");
            Bytes::from(buf)
        })
        .clone()
}

pub fn detect_book_file_type(file: &mut (impl Read + Seek)) -> io::Result<BookFileType> {
    if compare_bytes_from_file(file, b"PK\x03\x04", 0)? {
        // 1. 快速路径：偏移 0 为未压缩 `mimetype`（允许非零 extra_len 与 Data Descriptor）。
        let mut lfh = [0u8; 30];
        file.seek(SeekFrom::Start(0))?;
        if file.read_exact(&mut lfh).is_ok() {
            let flags = u16::from_le_bytes([lfh[6], lfh[7]]);
            let method = u16::from_le_bytes([lfh[8], lfh[9]]);
            let uncompressed_size = u32::from_le_bytes([lfh[22], lfh[23], lfh[24], lfh[25]]);
            let name_len = u16::from_le_bytes([lfh[26], lfh[27]]);
            let extra_len = u16::from_le_bytes([lfh[28], lfh[29]]);

            if method == 0 && name_len == 8 && compare_bytes_from_file(file, b"mimetype", 30)? {
                let payload_offset = 30u64 + 8u64 + u64::from(extra_len);
                if (flags & 0x0008) == 0 && (20..=32).contains(&uncompressed_size) {
                    let mut payload = vec![0u8; uncompressed_size as usize];
                    file.seek(SeekFrom::Start(payload_offset))?;
                    if file.read_exact(&mut payload).is_ok()
                        && payload.trim_ascii() == b"application/epub+zip"
                    {
                        let _ = file.seek(SeekFrom::Start(0));
                        return Ok(BookFileType::EPUB);
                    }
                } else if (flags & 0x0008) != 0
                    && compare_bytes_from_file(file, b"application/epub+zip", payload_offset)?
                {
                    let _ = file.seek(SeekFrom::Start(0));
                    return Ok(BookFileType::EPUB);
                }
            }
        }

        // 2. 回退路径：扫描 ZIP / ZIP64 Central Directory，兼容重排条目顺序或流式打包的 EPUB。
        if has_epub_central_directory_markers(file)? {
            let _ = file.seek(SeekFrom::Start(0));
            return Ok(BookFileType::EPUB);
        }
    } else if compare_bytes_from_file(file, b"%PDF-", 0)? {
        let _ = file.seek(SeekFrom::Start(0));
        return Ok(BookFileType::PDF);
    }
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

fn has_epub_central_directory_markers(file: &mut (impl Read + Seek)) -> io::Result<bool> {
    const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
    const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
    const CD_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    const MAX_TAIL_WINDOW: u64 = 65_536 + 22;
    const MAX_CD_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_ENTRIES: u64 = 50_000;

    let total_size = file.seek(SeekFrom::End(0))?;
    if total_size < 22 {
        return Ok(false);
    }

    let tail_len = total_size.min(MAX_TAIL_WINDOW) as usize;
    let tail_start = total_size - tail_len as u64;
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::Start(tail_start))?;
    file.read_exact(&mut tail)?;

    let eocd_rel = find_eocd_offset(&tail, tail_start);
    let Some(eocd_rel) = eocd_rel else {
        return Ok(false);
    };
    let eocd = &tail[eocd_rel..];
    let eocd_offset = tail_start + eocd_rel as u64;

    let mut disk_number = u64::from(u16::from_le_bytes([eocd[4], eocd[5]]));
    let mut cd_disk = u64::from(u16::from_le_bytes([eocd[6], eocd[7]]));
    let mut disk_entries = u64::from(u16::from_le_bytes([eocd[8], eocd[9]]));
    let mut total_entries = u64::from(u16::from_le_bytes([eocd[10], eocd[11]]));
    let mut cd_size = u64::from(u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]));
    let mut cd_offset = u64::from(u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]));
    let mut cd_upper_bound = eocd_offset;

    let needs_zip64 = disk_number == 0xFFFF
        || cd_disk == 0xFFFF
        || disk_entries == 0xFFFF
        || total_entries == 0xFFFF
        || cd_size == 0xFFFF_FFFF
        || cd_offset == 0xFFFF_FFFF;

    if needs_zip64 {
        if eocd_offset < 20 {
            return Ok(false);
        }
        let locator_offset = eocd_offset - 20;
        let mut locator = [0u8; 20];
        file.seek(SeekFrom::Start(locator_offset))?;
        if file.read_exact(&mut locator).is_err() || locator[0..4] != ZIP64_LOCATOR_SIG {
            return Ok(false);
        }
        let zip64_eocd_disk = u32::from_le_bytes([locator[4], locator[5], locator[6], locator[7]]);
        let zip64_eocd_offset = u64::from_le_bytes([
            locator[8], locator[9], locator[10], locator[11], locator[12], locator[13],
            locator[14], locator[15],
        ]);
        let total_disks = u32::from_le_bytes([locator[16], locator[17], locator[18], locator[19]]);
        if zip64_eocd_disk != 0
            || total_disks > 1
            || zip64_eocd_offset
                .checked_add(56)
                .is_none_or(|end| end > locator_offset)
        {
            return Ok(false);
        }

        let mut z64 = [0u8; 56];
        file.seek(SeekFrom::Start(zip64_eocd_offset))?;
        if file.read_exact(&mut z64).is_err() || z64[0..4] != ZIP64_EOCD_SIG {
            return Ok(false);
        }
        let record_size = u64::from_le_bytes([
            z64[4], z64[5], z64[6], z64[7], z64[8], z64[9], z64[10], z64[11],
        ]);
        if record_size < 44
            || zip64_eocd_offset
                .checked_add(12)
                .and_then(|o| o.checked_add(record_size))
                .is_none_or(|end| end > locator_offset)
        {
            return Ok(false);
        }

        disk_number = u64::from(u32::from_le_bytes([z64[16], z64[17], z64[18], z64[19]]));
        cd_disk = u64::from(u32::from_le_bytes([z64[20], z64[21], z64[22], z64[23]]));
        disk_entries = u64::from_le_bytes([
            z64[24], z64[25], z64[26], z64[27], z64[28], z64[29], z64[30], z64[31],
        ]);
        total_entries = u64::from_le_bytes([
            z64[32], z64[33], z64[34], z64[35], z64[36], z64[37], z64[38], z64[39],
        ]);
        cd_size = u64::from_le_bytes([
            z64[40], z64[41], z64[42], z64[43], z64[44], z64[45], z64[46], z64[47],
        ]);
        cd_offset = u64::from_le_bytes([
            z64[48], z64[49], z64[50], z64[51], z64[52], z64[53], z64[54], z64[55],
        ]);
        cd_upper_bound = zip64_eocd_offset;
    }

    if disk_number != 0
        || cd_disk != 0
        || disk_entries != total_entries
        || cd_size > MAX_CD_BYTES
        || total_entries > MAX_ENTRIES
        || cd_offset
            .checked_add(cd_size)
            .is_none_or(|end| end > cd_upper_bound)
    {
        return Ok(false);
    }

    let mut cd = vec![0u8; cd_size as usize];
    file.seek(SeekFrom::Start(cd_offset))?;
    file.read_exact(&mut cd)?;

    let mut cursor = 0usize;
    for _ in 0..total_entries {
        let Some(header_end) = cursor.checked_add(46).filter(|&end| end <= cd.len()) else {
            return Ok(false);
        };
        let hdr = &cd[cursor..header_end];
        if hdr[0..4] != CD_SIG {
            return Ok(false);
        }

        let method = u16::from_le_bytes([hdr[10], hdr[11]]);
        let raw_comp_size = u32::from_le_bytes([hdr[20], hdr[21], hdr[22], hdr[23]]);
        let raw_uncomp_size = u32::from_le_bytes([hdr[24], hdr[25], hdr[26], hdr[27]]);
        let name_len = usize::from(u16::from_le_bytes([hdr[28], hdr[29]]));
        let extra_len = usize::from(u16::from_le_bytes([hdr[30], hdr[31]]));
        let comment_len = usize::from(u16::from_le_bytes([hdr[32], hdr[33]]));
        let raw_lh_offset = u32::from_le_bytes([hdr[42], hdr[43], hdr[44], hdr[45]]);

        let name_start = header_end;
        let Some(extra_start) = name_start.checked_add(name_len).filter(|&end| end <= cd.len())
        else {
            return Ok(false);
        };
        let Some(comment_start) = extra_start.checked_add(extra_len).filter(|&end| end <= cd.len())
        else {
            return Ok(false);
        };
        let Some(next_cursor) = comment_start
            .checked_add(comment_len)
            .filter(|&end| end <= cd.len())
        else {
            return Ok(false);
        };

        let name = String::from_utf8_lossy(&cd[name_start..extra_start]);
        let norm_cow = name.replace('\\', "/");
        let mut trimmed = norm_cow.as_str();
        while let Some(rest) = trimmed.strip_prefix('/').or_else(|| trimmed.strip_prefix("./")) {
            trimmed = rest;
        }

        if trimmed.eq_ignore_ascii_case("meta-inf/container.xml") {
            return Ok(true);
        }

        if trimmed == "mimetype" && method == 0 {
            let local_header_offset = if raw_lh_offset == 0xFFFF_FFFF {
                resolve_zip64_lh_offset(
                    &cd[extra_start..comment_start],
                    raw_uncomp_size == 0xFFFF_FFFF,
                    raw_comp_size == 0xFFFF_FFFF,
                )
            } else {
                Some(u64::from(raw_lh_offset))
            };
            if let Some(lh_offset) = local_header_offset
                && lh_offset.checked_add(30).is_some_and(|end| end <= cd_offset)
            {
                let mut lfh = [0u8; 30];
                file.seek(SeekFrom::Start(lh_offset))?;
                if file.read_exact(&mut lfh).is_ok() && lfh[0..4] == [0x50, 0x4b, 0x03, 0x04] {
                    let lfh_name_len = u64::from(u16::from_le_bytes([lfh[26], lfh[27]]));
                    let lfh_extra_len = u64::from(u16::from_le_bytes([lfh[28], lfh[29]]));
                    if let Some(payload_offset) = lh_offset
                        .checked_add(30)
                        .and_then(|o| o.checked_add(lfh_name_len))
                        .and_then(|o| o.checked_add(lfh_extra_len))
                        && compare_bytes_from_file(file, b"application/epub+zip", payload_offset)?
                    {
                        return Ok(true);
                    }
                }
            }
        }

        cursor = next_cursor;
    }

    Ok(false)
}

fn find_eocd_offset(tail: &[u8], tail_start: u64) -> Option<usize> {
    const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];

    if tail.len() < 22 {
        return None;
    }
    for i in (0..=tail.len() - 22).rev() {
        if tail[i..i + 4] == EOCD_SIG {
            let comment_len = usize::from(u16::from_le_bytes([tail[i + 20], tail[i + 21]]));
            if i + 22 + comment_len == tail.len() {
                return Some(i);
            }
        }
    }
    // 容错匹配：允许 EOCD 之后附带少量尾随字节
    for i in (0..=tail.len() - 22).rev() {
        if tail[i..i + 4] == EOCD_SIG {
            let comment_len = usize::from(u16::from_le_bytes([tail[i + 20], tail[i + 21]]));
            if i + 22 + comment_len <= tail.len() {
                let disk_num = u16::from_le_bytes([tail[i + 4], tail[i + 5]]);
                let cd_disk = u16::from_le_bytes([tail[i + 6], tail[i + 7]]);
                let cd_size = u32::from_le_bytes([tail[i + 12], tail[i + 13], tail[i + 14], tail[i + 15]]);
                let cd_offset = u32::from_le_bytes([tail[i + 16], tail[i + 17], tail[i + 18], tail[i + 19]]);
                let is_z64_sentinel = disk_num == 0xFFFF
                    || cd_disk == 0xFFFF
                    || cd_size == 0xFFFF_FFFF
                    || cd_offset == 0xFFFF_FFFF;
                if is_z64_sentinel {
                    if i >= 20 && tail[i - 20..i - 16] == ZIP64_LOCATOR_SIG {
                        return Some(i);
                    }
                } else if disk_num == 0
                    && cd_disk == 0
                    && u64::from(cd_offset)
                        .checked_add(u64::from(cd_size))
                        .is_some_and(|end| end <= tail_start + i as u64)
                {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn resolve_zip64_lh_offset(
    extra: &[u8],
    skip_uncomp: bool,
    skip_comp: bool,
) -> Option<u64> {
    let mut cursor = 0usize;
    while cursor + 4 <= extra.len() {
        let header_id = u16::from_le_bytes([extra[cursor], extra[cursor + 1]]);
        let field_len = usize::from(u16::from_le_bytes([extra[cursor + 2], extra[cursor + 3]]));
        let data_start = cursor + 4;
        let data_end = data_start.checked_add(field_len)?;
        if data_end > extra.len() {
            return None;
        }
        if header_id == 0x0001 {
            let z64 = &extra[data_start..data_end];
            let offset_pos = (usize::from(skip_uncomp) + usize::from(skip_comp)) * 8;
            let end_pos = offset_pos.checked_add(8)?;
            if end_pos <= z64.len() {
                return Some(u64::from_le_bytes([
                    z64[offset_pos],
                    z64[offset_pos + 1],
                    z64[offset_pos + 2],
                    z64[offset_pos + 3],
                    z64[offset_pos + 4],
                    z64[offset_pos + 5],
                    z64[offset_pos + 6],
                    z64[offset_pos + 7],
                ]));
            }
            return None;
        }
        cursor = data_end;
    }
    None
}

fn compare_bytes_from_file(
    file: &mut (impl Read + Seek),
    expected_head: &[u8],
    offset: u64,
) -> io::Result<bool> {
    let mut buf = vec![0u8; expected_head.len()];
    file.seek(SeekFrom::Start(offset))?;
    match file.read_exact(&mut buf) {
        Ok(()) => Ok(buf == expected_head),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use tracing::{
        Event, Level, Metadata, Subscriber,
        field::{Field, Visit},
        span::{Attributes, Id, Record},
    };

    use super::*;

    #[derive(Clone, Default)]
    struct LogCapture {
        entries: Arc<Mutex<Vec<(Level, String)>>>,
    }

    impl LogCapture {
        fn take_entries(&self) -> Vec<(Level, String)> {
            std::mem::take(&mut self.entries.lock().unwrap())
        }
    }

    struct FieldFormatter(String);

    impl Visit for FieldFormatter {
        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0
                .push_str(&format!("{}={:?}", field.name(), value));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(&format!("{}={}", field.name(), value));
        }
    }

    impl Subscriber for LogCapture {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut visitor = FieldFormatter(String::new());
            event.record(&mut visitor);
            self.entries
                .lock()
                .unwrap()
                .push((*event.metadata().level(), visitor.0));
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for &(name, data) in entries {
            let offset = out.len() as u32;
            let crc = crc32(data);
            let size = data.len() as u32;
            let name_bytes = name.as_bytes();
            let name_len = name_bytes.len() as u16;

            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&name_len.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        let count = entries.len() as u16;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn build_valid_header_broken_epub() -> Vec<u8> {
        let mimetype = b"application/epub+zip";
        let size = mimetype.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc32(mimetype).to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"mimetype");
        out.extend_from_slice(mimetype);
        out
    }

    #[test]
    fn extract_navigation_logs_warning_reasons_on_failures_and_stays_silent_for_empty_toc() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalBookFileStorage::new(dir.path().join("books"), "book", None).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let capture = LogCapture::default();

        tracing::subscriber::with_default(capture.clone(), || {
            // 1. Missing book hash emits WARN with explicit reason.
            let missing_hash = blake3::hash(b"missing-book");
            let missing_nav = rt.block_on(storage.extract_navigation(&missing_hash));
            assert!(missing_nav.is_empty());
            let logs = capture.take_entries();
            assert!(
                logs.iter().any(|(level, msg)| {
                    *level == Level::WARN && msg.contains("book file metadata not found in storage")
                }),
                "expected WARN log for missing hash, got {logs:?}"
            );

            // 2. Stored PDF with pdfium: None emits WARN explaining Pdfium is not loaded.
            let pdf_bytes = b"%PDF-1.4\nbroken";
            let pdf_hash = blake3::hash(pdf_bytes);
            rt.block_on(storage.import(&pdf_hash, Cursor::new(pdf_bytes)))
                .unwrap();
            let _ = capture.take_entries();
            let pdf_nav = rt.block_on(storage.extract_navigation(&pdf_hash));
            assert!(pdf_nav.is_empty());
            let logs = capture.take_entries();
            assert!(
                logs.iter().any(|(level, msg)| {
                    *level == Level::WARN && msg.contains("Pdfium library is not loaded")
                }),
                "expected WARN log for missing Pdfium, got {logs:?}"
            );

            // 3. Stored malformed EPUB emits WARN explaining parse failure.
            let broken_epub = build_valid_header_broken_epub();
            let broken_epub_hash = blake3::hash(&broken_epub);
            rt.block_on(storage.import(&broken_epub_hash, Cursor::new(broken_epub)))
                .unwrap();
            let _ = capture.take_entries();
            let broken_nav = rt.block_on(storage.extract_navigation(&broken_epub_hash));
            assert!(broken_nav.is_empty());
            let logs = capture.take_entries();
            assert!(
                logs.iter().any(|(level, msg)| {
                    *level == Level::WARN && msg.contains("failed to parse EPUB archive")
                }),
                "expected WARN log for malformed EPUB, got {logs:?}"
            );

            // 4. Valid EPUB with genuinely empty TOC produces 0 WARN/ERROR logs.
            let empty_toc_epub = build_stored_zip(&[
                ("mimetype", b"application/epub+zip"),
                (
                    "META-INF/container.xml",
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
                ),
                (
                    "EPUB/package.opf",
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:22222222-2222-2222-2222-222222222222</dc:identifier>
    <dc:title>Empty TOC</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1" linear="yes"/>
  </spine>
</package>"#,
                ),
                (
                    "EPUB/nav.xhtml",
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body><nav epub:type="toc" id="toc"><ol></ol></nav></body>
</html>"#,
                ),
                (
                    "EPUB/chapter1.xhtml",
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Body text</p></body></html>"#,
                ),
            ]);
            let mut tmp = tempfile::tempfile().unwrap();
            tmp.write_all(&empty_toc_epub).unwrap();
            let reader =
                BookFileContentReader::new(tmp, 0, empty_toc_epub.len() as u64).unwrap();
            let _ = capture.take_entries();
            let nav = book_navigation::extract_epub_navigation(reader, "empty-toc-hash").unwrap();
            assert!(nav.toc().is_empty());
            assert_eq!(nav.reading_units().len(), 1);
            assert!(nav.total_weight() > 0);
            let logs = capture.take_entries();
            let warn_or_error: Vec<_> = logs
                .into_iter()
                .filter(|(level, _)| *level <= Level::WARN)
                .collect();
            assert!(
                warn_or_error.is_empty(),
                "genuinely empty TOC should not emit WARN/ERROR logs, got {warn_or_error:?}"
            );
        });
    }

    #[test]
    fn extract_epub_navigation_preserves_percent_encoded_hash_and_question_mark_filenames() {
        let capture = LogCapture::default();
        let epub = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
            ),
            (
                "EPUB/package.opf",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:33333333-3333-3333-3333-333333333333</dc:identifier>
    <dc:title>Encoded Href</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="C%23_Ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="What%3F.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1" linear="yes"/>
    <itemref idref="ch2" linear="yes"/>
  </spine>
</package>"#,
            ),
            (
                "EPUB/nav.xhtml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body><nav epub:type="toc" id="toc"><ol>
  <li><a href="C%23_Ch1.xhtml#sec-1">C# Intro</a></li>
  <li><a href="What%3F.xhtml">FAQ</a></li>
</ol></nav></body>
</html>"#,
            ),
            (
                "EPUB/C#_Ch1.xhtml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Lead text before section.</p><h2 id="sec-1">Section One</h2></body></html>"#,
            ),
            (
                "EPUB/What?.xhtml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Frequently asked questions.</p></body></html>"#,
            ),
        ]);

        let mut tmp = tempfile::tempfile().unwrap();
        tmp.write_all(&epub).unwrap();
        let reader = BookFileContentReader::new(tmp, 0, epub.len() as u64).unwrap();

        let nav = tracing::subscriber::with_default(capture.clone(), || {
            book_navigation::extract_epub_navigation(reader, "encoded-hash").unwrap()
        });

        assert_eq!(nav.toc().len(), 2);
        assert_eq!(nav.toc()[0].unit_index(), Some(0));
        assert!(nav.toc()[0].unit_offset().value() > 0.0);
        use shiro_domain::library::book::locator::{
            BookAnchor, EpubAnchor, EpubNavTarget,
        };
        assert!(matches!(
            nav.toc()[0].locator(),
            Some(BookAnchor::Epub(EpubAnchor {
                spine_index: Some(0),
                href,
                target: EpubNavTarget::Fragment(frag),
            })) if href.as_str() == "EPUB/C%23_Ch1.xhtml" && frag.as_str() == "sec-1"
        ));
        assert!(matches!(
            nav.toc()[1].locator(),
            Some(BookAnchor::Epub(EpubAnchor {
                spine_index: Some(1),
                href,
                target: EpubNavTarget::Start,
            })) if href.as_str() == "EPUB/What%3F.xhtml"
        ));

        let warn_or_error: Vec<_> = capture
            .take_entries()
            .into_iter()
            .filter(|(level, _)| *level <= Level::WARN)
            .collect();
        assert!(
            warn_or_error.is_empty(),
            "expected 0 warnings, got {warn_or_error:?}"
        );
    }

    #[test]
    fn extract_epub_navigation_handles_colon_filenames_as_internal_and_uris_as_external() {
        use shiro_domain::library::book::{
            locator::{EpubAnchor, EpubNavTarget, NavigationLocator},
            value_objects::TocEntryFlags,
        };

        let capture = LogCapture::default();
        let epub = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
            ),
            (
                "package.opf",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:44444444-4444-4444-4444-444444444444</dc:identifier>
    <dc:title>Colon Href</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="Ch01:Intro.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1" linear="yes"/>
  </spine>
</package>"#,
            ),
            (
                "nav.xhtml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body><nav epub:type="toc" id="toc"><ol>
  <li><a href="Ch01:Intro.xhtml">Chapter One</a></li>
  <li><a href="https://example.com/docs">External Docs</a></li>
</ol></nav></body>
</html>"#,
            ),
            (
                "Ch01:Intro.xhtml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Colon filename chapter text.</p></body></html>"#,
            ),
        ]);

        let mut tmp = tempfile::tempfile().unwrap();
        tmp.write_all(&epub).unwrap();
        let reader = BookFileContentReader::new(tmp, 0, epub.len() as u64).unwrap();

        let nav = tracing::subscriber::with_default(capture.clone(), || {
            book_navigation::extract_epub_navigation(reader, "colon-hash").unwrap()
        });

        assert_eq!(nav.toc().len(), 2);
        assert_eq!(nav.toc()[0].flags(), TocEntryFlags::empty());
        assert!(matches!(
            nav.toc()[0].locator(),
            Some(NavigationLocator::Epub(EpubAnchor {
                spine_index: Some(0),
                href,
                target: EpubNavTarget::Start,
            })) if href.as_str() == "Ch01:Intro.xhtml"
        ));
        assert_eq!(nav.toc()[1].flags(), TocEntryFlags::EXTERNAL_LINK);
        assert_eq!(nav.toc()[1].locator(), None);

        let warn_or_error: Vec<_> = capture
            .take_entries()
            .into_iter()
            .filter(|(level, _)| *level <= Level::WARN)
            .collect();
        assert!(
            warn_or_error.is_empty(),
            "expected 0 warnings, got {warn_or_error:?}"
        );
    }

    fn build_custom_stored_zip(entries: &[(&str, &[u8], u16, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for &(name, data, flags, extra, zero_lfh_sizes) in entries {
            let offset = out.len() as u32;
            let crc = crc32(data);
            let size = data.len() as u32;
            let lfh_crc = if zero_lfh_sizes { 0 } else { crc };
            let lfh_size = if zero_lfh_sizes { 0 } else { size };
            let name_bytes = name.as_bytes();
            let name_len = name_bytes.len() as u16;
            let extra_len = extra.len() as u16;

            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&lfh_crc.to_le_bytes());
            out.extend_from_slice(&lfh_size.to_le_bytes());
            out.extend_from_slice(&lfh_size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&extra_len.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(extra);
            out.extend_from_slice(data);
            if (flags & 0x0008) != 0 {
                out.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
                out.extend_from_slice(&crc.to_le_bytes());
                out.extend_from_slice(&size.to_le_bytes());
                out.extend_from_slice(&size.to_le_bytes());
            }

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&flags.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&name_len.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        let count = entries.len() as u16;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn detect_book_file_type_accepts_reordered_extra_and_data_descriptor_epubs_and_rejects_plain_zip() {
        let dd_epub = build_custom_stored_zip(&[(
            "mimetype",
            b"application/epub+zip",
            0x0008,
            &[],
            true,
        )]);
        assert_eq!(
            detect_book_file_type(&mut Cursor::new(&dd_epub)).unwrap(),
            BookFileType::EPUB
        );

        let extra_epub = build_custom_stored_zip(&[(
            "mimetype",
            b"application/epub+zip",
            0,
            &[0x55; 36],
            false,
        )]);
        assert_eq!(
            detect_book_file_type(&mut Cursor::new(&extra_epub)).unwrap(),
            BookFileType::EPUB
        );

        let reordered_epub = build_custom_stored_zip(&[
            ("OEBPS/padding.xhtml", &[b'x'; 256], 0, &[], false),
            ("META-INF/container.xml", b"<container/>", 0, &[], false),
            ("mimetype", b"application/epub+zip", 0, &[0x11; 16], false),
        ]);
        assert_eq!(
            detect_book_file_type(&mut Cursor::new(&reordered_epub)).unwrap(),
            BookFileType::EPUB
        );

        let container_only_epub = build_custom_stored_zip(&[
            ("OEBPS/ch1.xhtml", b"<html></html>", 0, &[], false),
            ("META-INF/container.xml", b"<container/>", 0, &[], false),
        ]);
        assert_eq!(
            detect_book_file_type(&mut Cursor::new(&container_only_epub)).unwrap(),
            BookFileType::EPUB
        );

        let plain_zip = build_stored_zip(&[("readme.txt", b"not an epub")]);
        assert_eq!(
            detect_book_file_type(&mut Cursor::new(&plain_zip))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn extract_epub_cover_falls_back_to_cover_xhtml_and_manifest_cover_image() {
        let mut png_bytes = Vec::new();
        let img = ImageBuffer::from_pixel(2, 2, Rgba([12, 34, 56, 255]));
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .unwrap();

        let container = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:55555555-5555-5555-5555-555555555555</dc:identifier>
    <dc:title>Fallback Cover</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="covpage" href="Text/cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="img1" href="Images/HeroArt.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="covpage" linear="yes"/></spine>
</package>"#;

        let cover_xhtml = br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><section><img src="../Images/HeroArt.png" alt="cover"/></section></body></html>"#;

        let epub_via_xhtml = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("OEBPS/package.opf", opf),
            ("OEBPS/Text/cover.xhtml", cover_xhtml),
            ("OEBPS/Images/HeroArt.png", &png_bytes),
        ]);
        let mut tmp1 = tempfile::tempfile().unwrap();
        tmp1.write_all(&epub_via_xhtml).unwrap();
        let reader1 = BookFileContentReader::new(tmp1, 0, epub_via_xhtml.len() as u64).unwrap();
        assert_eq!(extract_epub_cover(reader1).unwrap(), png_bytes);

        let opf_manifest_only = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:66666666-6666-6666-6666-666666666666</dc:identifier>
    <dc:title>Manifest Cover Name</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="Text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="img-cover" href="Images/Cover.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="ch1" linear="yes"/></spine>
</package>"#;
        let epub_via_manifest = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("OEBPS/package.opf", opf_manifest_only),
            ("OEBPS/Text/ch1.xhtml", b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Text</p></body></html>"),
            ("OEBPS/Images/Cover.png", &png_bytes),
        ]);
        let mut tmp2 = tempfile::tempfile().unwrap();
        tmp2.write_all(&epub_via_manifest).unwrap();
        let reader2 = BookFileContentReader::new(tmp2, 0, epub_via_manifest.len() as u64).unwrap();
        assert_eq!(extract_epub_cover(reader2).unwrap(), png_bytes);
    }

    #[test]
    fn scan_book_storage_persists_pdf_page_count_and_keeps_epub_none() {
        let dir = tempfile::tempdir().unwrap();
        let books_dir = dir.path().join("books");
        let storage = LocalBookFileStorage::new(books_dir.clone(), "book", None).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let epub = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
            ),
            (
                "EPUB/package.opf",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">978-0-306-40615-7</dc:identifier>
    <dc:title>Header Scan Book</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1" linear="yes"/></spine>
</package>"#,
            ),
            ("EPUB/ch1.xhtml", b"<html><body><p>Hello</p></body></html>"),
        ]);
        let epub_hash = blake3::hash(&epub);
        rt.block_on(storage.import(&epub_hash, Cursor::new(epub.clone())))
            .unwrap();

        // Synthesize a stored PDF .book file with an explicit 42-page header field.
        let pdf_payload = b"%PDF-1.4\n%fake\n";
        let pdf_hash = blake3::hash(pdf_payload);
        let mut pdf_stored = Vec::with_capacity(BOOK_FILE_HEADER_SIZE as usize + pdf_payload.len());
        pdf_stored.push(BOOK_FILE_VERSION);
        pdf_stored.push(0);
        pdf_stored.extend_from_slice(pdf_hash.as_bytes());
        pdf_stored.extend_from_slice(&(pdf_payload.len() as u64).to_be_bytes());
        pdf_stored.extend_from_slice(&42u32.to_be_bytes());
        pdf_stored.extend_from_slice(pdf_payload);
        std::fs::write(
            books_dir.join(format!("{}.book", pdf_hash.to_hex())),
            pdf_stored,
        )
        .unwrap();

        let (scanned, _) = scan_book_storage(&books_dir, "book").unwrap();
        let epub_meta = *scanned.get(&epub_hash).unwrap();
        assert_eq!(epub_meta.file_type, BookFileType::EPUB);
        assert_eq!(epub_meta.size, epub.len() as u64);
        assert_eq!(epub_meta.page_count, None);

        let pdf_meta = *scanned.get(&pdf_hash).unwrap();
        assert_eq!(pdf_meta.file_type, BookFileType::PDF);
        assert_eq!(pdf_meta.size, pdf_payload.len() as u64);
        assert_eq!(pdf_meta.page_count, Some(42));
    }

    #[test]
    fn extract_epub_metadata_prefers_valid_fields_and_skips_invalid_primary_uuid() {
        let epub = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
            ),
            (
                "EPUB/package.opf",
                br##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="uid">urn:uuid:77777777-7777-7777-7777-777777777777</dc:identifier>
    <dc:identifier opf:scheme="ISBN">978-0-306-40615-7</dc:identifier>
    <dc:title id="t1">Metadata Extraction</dc:title>
    <meta refines="#t1" property="title-type">main</meta>
    <dc:title id="t2">  Deep   Dive  </dc:title>
    <meta refines="#t2" property="title-type">subtitle</meta>
    <dc:creator id="c1">  Alice   Smith  </dc:creator>
    <meta refines="#c1" property="role" scheme="marc:relators">aut</meta>
    <dc:creator id="c2">Bob Editor</dc:creator>
    <meta refines="#c2" property="role" scheme="marc:relators">edt</meta>
    <dc:creator>Alice Smith</dc:creator>
    <dc:creator>Carol Jones</dc:creator>
    <dc:publisher>  Shiro   Press  </dc:publisher>
    <dc:date opf:event="publication">2024-05</dc:date>
    <dc:language>zh_cn</dc:language>
    <dc:description>  Line one
Line two  </dc:description>
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1" linear="yes"/></spine>
</package>"##,
            ),
            ("EPUB/ch1.xhtml", b"<html><body><p>Body</p></body></html>"),
        ]);

        let mut tmp = tempfile::tempfile().unwrap();
        tmp.write_all(&epub).unwrap();
        let reader = BookFileContentReader::new(tmp, 0, epub.len() as u64).unwrap();
        let extracted = extract_epub_metadata(reader).unwrap();

        assert_eq!(
            extracted.subtitle.as_ref().map(BookSubtitle::as_str),
            Some("Deep Dive")
        );
        assert_eq!(
            extracted
                .authors
                .iter()
                .map(BookAuthor::as_str)
                .collect::<Vec<_>>(),
            ["Alice Smith", "Carol Jones"]
        );
        assert_eq!(
            extracted.publisher.as_ref().map(BookPublisher::as_str),
            Some("Shiro Press")
        );
        assert_eq!(
            extracted.published_date.map(|d| d.to_string()).as_deref(),
            Some("2024-05")
        );
        assert_eq!(
            extracted.language.as_ref().map(BookLanguage::as_str),
            Some("zh-CN")
        );
        assert_eq!(
            extracted.description.as_ref().map(BookDescription::as_str),
            Some("Line one Line two")
        );
        assert_eq!(
            sanitize_multiline_text("  First\r\nSecond\x00  "),
            "First\nSecond"
        );
        assert_eq!(
            extracted.identifier.as_ref().map(BookIdentifier::as_str),
            Some("9780306406157")
        );
    }

    #[test]
    fn extract_epub_authors_falls_back_to_all_creators_and_pdf_splits_on_semicolons() {
        let epub = build_stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
            ),
            (
                "EPUB/package.opf",
                br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="uid">urn:uuid:88888888-8888-8888-8888-888888888888</dc:identifier>
    <dc:title>Fallback Authors</dc:title>
    <dc:creator opf:role="edt">Only Editor</dc:creator>
    <dc:creator opf:role="edt">Only Editor</dc:creator>
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1" linear="yes"/></spine>
</package>"#,
            ),
            ("EPUB/ch1.xhtml", b"<html><body><p>Body</p></body></html>"),
        ]);

        let mut tmp = tempfile::tempfile().unwrap();
        tmp.write_all(&epub).unwrap();
        let reader = BookFileContentReader::new(tmp, 0, epub.len() as u64).unwrap();
        let extracted = extract_epub_metadata(reader).unwrap();
        assert_eq!(extracted.subtitle, None);
        assert_eq!(
            extracted
                .authors
                .iter()
                .map(BookAuthor::as_str)
                .collect::<Vec<_>>(),
            ["Only Editor"]
        );

        let pdf_authors = extract_pdf_authors("Doe, Jane ; 张三；Doe, Jane ;  ");
        assert_eq!(
            pdf_authors
                .iter()
                .map(BookAuthor::as_str)
                .collect::<Vec<_>>(),
            ["Doe, Jane", "张三"]
        );
    }

    #[test]
    fn parse_pdf_creation_date_handles_standard_and_partial_formats() {
        assert_eq!(
            parse_pdf_creation_date("D:20240512103000+08'00'")
                .map(|d| d.to_string())
                .as_deref(),
            Some("2024-05-12")
        );
        assert_eq!(
            parse_pdf_creation_date("D:202405")
                .map(|d| d.to_string())
                .as_deref(),
            Some("2024-05")
        );
        assert_eq!(
            parse_pdf_creation_date("D:2024")
                .map(|d| d.to_string())
                .as_deref(),
            Some("2024")
        );
        assert_eq!(
            parse_pdf_creation_date("2023-11-09T00:00:00Z")
                .map(|d| d.to_string())
                .as_deref(),
            Some("2023-11-09")
        );
        assert_eq!(parse_pdf_creation_date("D:20230229"), None);
        assert_eq!(parse_pdf_creation_date("not-a-date"), None);
    }
}
