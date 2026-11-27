use std::{
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
use pdfium_render::prelude::{PdfPageRenderRotation, PdfRenderConfig, Pdfium};
use rayon::prelude::*;
use rbook::Epub;
use shiro_application::library::ports::{
    BookFileMetadata, BookFileStorageError, BookFileStore, CoverByteStream, OpenedBookFile,
};
use shiro_domain::library::book::{
    file::entity::BookFileType,
    navigation::BookNavigation,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, Take},
};
use tokio_util::io::ReaderStream;

use super::book_navigation;

const BOOK_FILE_VERSION: u8 = 1;
const BOOK_FILE_HEADER_SIZE: u64 = 1 + 1 + 32 + 8;

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
            let (file_type, extracted_cover) = {
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
                let cover = match file_type {
                    BookFileType::PDF => match pdfium.as_deref() {
                        Some(pdfium) => extract_pdf_cover(content, pdfium),
                        None => Err(anyhow::anyhow!("Pdfium library is not loaded")),
                    },
                    BookFileType::EPUB => extract_epub_cover(content),
                };
                let cover =
                    cover.and_then(|bytes| Ok((image::guess_format(&bytes)?, Bytes::from(bytes))));

                (file_type, cover)
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

fn extract_pdf_cover(
    file: BookFileContentReader,
    pdfium: &Mutex<Pdfium>,
) -> anyhow::Result<Vec<u8>> {
    let pdfium = pdfium
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium lock poisoned"))?;
    let doc = pdfium.load_pdf_from_reader(file, None)?;
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
}

fn extract_epub_cover(file: BookFileContentReader) -> anyhow::Result<Vec<u8>> {
    let epub = Epub::read(file)?;
    let cover = epub
        .manifest()
        .cover_image()
        .ok_or_else(|| anyhow::anyhow!("epub does not declare a cover image"))?;
    Ok(cover.read_bytes()?)
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
        // 1. 检查压缩方法是否为 0（不压缩，EPUB 对 mimetype 的要求）
        if let Some(method) = read_u16_le_at(file, 8)?
            && method == 0
            && let Some(name_len) = read_u16_le_at(file, 26)?
            && let Some(extra_len) = read_u16_le_at(file, 28)?
            && name_len == 8
            && extra_len == 0
            && compare_bytes_from_file(file, b"mimetype", 30)?
            && let Some(uncompressed_size) = read_u32_le_at(file, 18)?
            && uncompressed_size == b"application/epub+zip".len() as u32
            && compare_bytes_from_file(file, b"application/epub+zip", 38)?
        {
            return Ok(BookFileType::EPUB);
        }
    } else if compare_bytes_from_file(file, b"%PDF-", 0)? {
        return Ok(BookFileType::PDF);
    }
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

fn read_u16_le_at(file: &mut (impl Read + Seek), offset: u64) -> io::Result<Option<u16>> {
    let mut buf = [0u8; 2];
    file.seek(SeekFrom::Start(offset))?;
    match file.read_exact(&mut buf) {
        Ok(()) => Ok(Some(u16::from_le_bytes(buf))),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(err) => Err(err),
    }
}

fn read_u32_le_at(file: &mut (impl Read + Seek), offset: u64) -> io::Result<Option<u32>> {
    let mut buf = [0u8; 4];
    file.seek(SeekFrom::Start(offset))?;
    match file.read_exact(&mut buf) {
        Ok(()) => Ok(Some(u32::from_le_bytes(buf))),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(err) => Err(err),
    }
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
        assert!(matches!(
            nav.toc()[0].locator(),
            Some(shiro_domain::library::book::value_objects::BookLocator::Epub {
                spine_index: Some(0),
                path,
                fragment: Some(frag),
                cfi: None,
            }) if path.as_str() == "EPUB/C%23_Ch1.xhtml" && frag == "sec-1"
        ));
        assert!(matches!(
            nav.toc()[1].locator(),
            Some(shiro_domain::library::book::value_objects::BookLocator::Epub {
                spine_index: Some(1),
                path,
                fragment: None,
                cfi: None,
            }) if path.as_str() == "EPUB/What%3F.xhtml"
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
        use shiro_domain::library::book::value_objects::{BookLocator, TocEntryFlags};

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
            Some(BookLocator::Epub {
                spine_index: Some(0),
                path,
                fragment: None,
                cfi: None,
            }) if path.as_str() == "Ch01:Intro.xhtml"
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
}
