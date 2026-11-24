use std::{
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use blake3::Hash;
use bytes::{Buf, BufMut, Bytes, TryGetError};
use dashmap::DashMap;
use futures_util::{StreamExt, stream};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use pdfium_render::prelude::{PdfPageRenderRotation, PdfRenderConfig, Pdfium};
use rayon::prelude::*;
use rbook::Epub;
use shiro_application::library::ports::{BookFileStorageError, BookFileStore, CoverByteStream};
use shiro_domain::library::book::chapter::ChapterLocation;
use shiro_domain::library::book::file::entity::{BookFileType, BookFileChapter};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, Take},
};
use tokio_util::io::ReaderStream;

const BOOK_FILE_VERSION: u8 = 1;
const BOOK_FILE_HEADER_SIZE: u64 = 1 + 1 + 32 + 8;


pub type StorageResult<T> = Result<T, BookFileStorageError>;

pub struct LocalBookFileStorage {
    dir: PathBuf,
    ext: String,
    files: Arc<DashMap<Hash, BookFileType>>,
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
            return Err(BookFileStorageError(io::Error::new(
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

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn scan(&mut self) -> io::Result<()> {
        let (files, covers) = scan_directory(&self.dir, &self.ext)?;
        self.files = Arc::new(files);
        self.covers = Arc::new(covers);
        Ok(())
    }

    pub async fn scan_async(mut self) -> io::Result<Self> {
        let dir = self.dir.clone();
        let ext = self.ext.clone();
        let (files, covers) = tokio::task::spawn_blocking(move || scan_directory(&dir, &ext))
            .await
            .map_err(|err| io::Error::other(format!("book storage scan panicked: {err}")))??;
        self.files = Arc::new(files);
        self.covers = Arc::new(covers);
        Ok(self)
    }
}

impl BookFileStore for LocalBookFileStorage {
    fn contains(&self, key: &Hash) -> bool {
        self.files.contains_key(key)
    }

    fn get_type(&self, key: &Hash) -> Option<BookFileType> {
        // 书籍永不删除，因此这边的线程安全问题无需在意。
        self.files.get(key).map(|entry| *entry.value())
    }

    async fn save<R>(&self, hash: &Hash, mut reader: R) -> Result<(), BookFileStorageError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let temp_dir = self.dir.join(".temp");
        let temp_file = tempfile::NamedTempFile::new_in(&temp_dir)?;
        let std_writer = temp_file.as_file().try_clone()?;
        let mut async_writer = File::from_std(std_writer);

        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let bytes_read = reader.read(&mut buf).await?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buf[..bytes_read];
            hasher.update(chunk);
            async_writer.write_all(chunk).await?;
        }
        async_writer.flush().await?;
        drop(async_writer);

        let file_hash = hasher.finalize();
        if file_hash != *hash {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let dir = self.dir.clone();
        let ext = self.ext.clone();
        let pdfium = self.pdfium.clone();
        let files = Arc::clone(&self.files);
        let covers = Arc::clone(&self.covers);

        tokio::task::spawn_blocking(move || {
            finalize_book_file(temp_file, &dir, &ext, file_hash, pdfium, &files, &covers)
        })
        .await
        .map_err(|err| {
            BookFileStorageError(io::Error::other(format!(
                "book file finalization task failed: {err}"
            )))
        })?
    }

    async fn open(&self, hash: &Hash) -> Result<Take<File>, BookFileStorageError> {
        if self.files.contains_key(hash) {
            let mut file = File::open(
                self.dir
                    .join(hash.to_hex().to_string())
                    .with_extension(&self.ext),
            )
            .await?;
            let mut version = [0u8; 1];
            file.read_exact(&mut version).await?;
            if version[0] != BOOK_FILE_VERSION {
                return Err(io::ErrorKind::Unsupported.into());
            }
            file.seek(SeekFrom::Start(BOOK_FILE_HEADER_SIZE - 8))
                .await?;
            let mut size_bytes = [0u8; 8];
            file.read_exact(&mut size_bytes).await?;
            file.seek(SeekFrom::Start(BOOK_FILE_HEADER_SIZE)).await?;
            Ok(file.take(u64::from_be_bytes(size_bytes)))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }



    fn get_chapters(&self, hash: &Hash) -> Vec<BookFileChapter> {
        (|| -> io::Result<Vec<BookFileChapter>> {
            let file_type = self
                .get_type(hash)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            let mut file = std::fs::File::open(
                self.dir
                    .join(hash.to_hex().to_string())
                    .with_extension(&self.ext),
            )?;
            file.seek(SeekFrom::Start(BOOK_FILE_HEADER_SIZE - 8))?;

            let mut size_bytes = [0u8; 8];
            file.read_exact(&mut size_bytes)?;
            file.seek(SeekFrom::Start(
                BOOK_FILE_HEADER_SIZE
                    .checked_add(u64::from_be_bytes(size_bytes))
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?,
            ))?;

            let mut chapter_bytes = Vec::new();
            file.read_to_end(&mut chapter_bytes)?;
            read_book_chapters(file_type, &chapter_bytes)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
        })()
        .unwrap_or_default()
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
        let stream = stream::once(async move { Ok(Bytes::from(data)) });
        Ok((stream.boxed(), ImageFormat::Png.to_mime_type().to_owned()))
    }
}

fn scan_directory(
    dir: &Path,
    ext: &str,
) -> io::Result<(
    DashMap<Hash, BookFileType>,
    DashMap<Hash, (PathBuf, ImageFormat)>,
)> {
    let files = Arc::new(DashMap::new());

    std::fs::read_dir(dir)?
        .filter_map(|res| {
            let path = res.ok()?.path();
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

            let mut chapter_bytes = Vec::new();
            if let Err(e) = file.read_to_end(&mut chapter_bytes) {
                tracing::warn!(
                    "Skipped: Cannot read chapter data from {}: {}",
                    file_name, e
                );
                return;
            }
            if let Err(e) = read_book_chapters(file_type, &chapter_bytes) {
                tracing::warn!("Skipped: Invalid chapter data in {}: {}", file_name, e);
                return;
            }

            let calculated_hash = hasher.finalize();
            if file_size == actual_size
                && file_hash == calculated_hash
                && path.file_stem().and_then(|stem| stem.to_str())
                    == Some(file_hash.to_hex().as_str())
            {
                files.insert(calculated_hash, file_type);
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

fn finalize_book_file(
    mut temp_file: tempfile::NamedTempFile,
    dir: &Path,
    ext: &str,
    file_hash: Hash,
    pdfium: Option<Arc<Mutex<Pdfium>>>,
    files: &DashMap<Hash, BookFileType>,
    covers: &DashMap<Hash, (PathBuf, ImageFormat)>,
) -> StorageResult<()> {
    let temp_dir = dir.join(".temp");
    let file_size = temp_file.as_file().metadata()?.len();

    let file_type = detect_book_file_type(temp_file.as_file_mut())?;
    let chapters =
        match extract_book_chapters(temp_file.as_file_mut(), file_type, pdfium.as_deref()) {
            Ok(chapters) => chapters,
            Err(err) => {
                tracing::warn!(
                    hash = %file_hash.to_hex(),
                    file_type = %file_type,
                    error = %err,
                    "failed to extract book chapters, storing an empty chapter outline"
                );
                Vec::new()
            }
        };
    let mut final_temp_file = tempfile::NamedTempFile::new_in(&temp_dir)?;
    final_temp_file.write_all(&[BOOK_FILE_VERSION])?;
    final_temp_file.write_all(&[match file_type {
        BookFileType::PDF => 0,
        BookFileType::EPUB => 1,
    }])?;
    final_temp_file.write_all(file_hash.as_bytes())?;
    final_temp_file.write_all(&file_size.to_be_bytes())?;

    let extracted = match file_type {
        BookFileType::PDF => match pdfium.as_deref() {
            Some(pdfium) => extract_pdf_cover(temp_file.as_file_mut(), pdfium),
            None => Err(anyhow::anyhow!("Pdfium library is not loaded")),
        },
        BookFileType::EPUB => extract_epub_cover(temp_file.as_file_mut()),
    }
    .and_then(|cover| Ok((image::guess_format(&cover)?, cover)));

    let (cover_format, cover) = match extracted {
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

    temp_file.seek(SeekFrom::Start(0))?;
    std::io::copy(&mut temp_file, &mut final_temp_file)?;
    let mut chapter_bytes = Vec::new();
    write_book_chapters(chapters, &mut chapter_bytes)?;
    final_temp_file.write_all(&chapter_bytes)?;
    final_temp_file.as_file_mut().flush()?;
    let _ = temp_file.close();

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
        Err(err) => return Err(BookFileStorageError(err.error)),
    }

    let final_path = dir.join(&hash_hex).with_extension(ext);
    match final_temp_file.persist_noclobber(&final_path) {
        Ok(_) => {
            covers.insert(file_hash, (cover_path, cover_format));
            files.insert(file_hash, file_type);
            Ok(())
        }
        Err(err) => Err(BookFileStorageError(err.error)),
    }
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

fn extract_pdf_cover(file: &mut std::fs::File, pdfium: &Mutex<Pdfium>) -> anyhow::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
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

fn extract_epub_cover(file: &mut std::fs::File) -> anyhow::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let epub = Epub::read(file.try_clone()?)?;
    let cover = epub
        .manifest()
        .cover_image()
        .ok_or_else(|| anyhow::anyhow!("epub does not declare a cover image"))?;
    Ok(cover.read_bytes()?)
}

fn write_book_chapters(chapters: Vec<BookFileChapter>, bytes: &mut Vec<u8>) -> io::Result<()> {
    bytes.put_u32(chapters.len() as u32);
    for chapter in chapters {
        bytes.put_u32(chapter.position);
        write_string(&chapter.title, bytes);
        match chapter.location {
            ChapterLocation::Pdf {
                start_page,
                end_page,
            } => {
                bytes.put_u32(start_page);
                bytes.put_u32(end_page);
            }
            ChapterLocation::Epub { href, cfi } => {
                write_string(&href, bytes);
                match cfi {
                    Some(cfi) => {
                        bytes.put_u8(1);
                        write_string(&cfi, bytes);
                    }
                    None => bytes.put_u8(0),
                }
            }
        }
    }

    Ok(())
}
fn read_book_chapters(
    file_type: BookFileType,
    mut bytes: &[u8],
) -> Result<Vec<BookFileChapter>, TryGetError> {
    let count = bytes.try_get_u32()? as usize;
    let mut chapters = Vec::with_capacity(count);

    for _ in 0..count {
        let position = bytes.try_get_u32()?;
        let title = read_string(&mut bytes)?;

        let location = match file_type {
            BookFileType::PDF => {
                let start_page = bytes.try_get_u32()?;
                let end_page = bytes.try_get_u32()?;
                ChapterLocation::Pdf {
                    start_page,
                    end_page,
                }
            }
            BookFileType::EPUB => {
                let href = read_string(&mut bytes)?;
                let cfi = match bytes.try_get_u8()? {
                    1 => Some(read_string(&mut bytes)?),
                    0 => None,
                    _ => return Err(TryGetError {requested: 0, available: 0})
                };
                ChapterLocation::Epub { href, cfi }
            }
        };

        chapters.push(BookFileChapter {
            position,
            title,
            location,
        });
    }

    if !bytes.is_empty() {
        return Err(TryGetError {requested: 0, available: 0});
    }
    Ok(chapters)
}

fn write_string(value: &str, bytes: &mut Vec<u8>) {
    bytes.put_u16(value.len() as u16);
    bytes.put_slice(value.as_bytes());
}

fn read_string(bytes: &mut &[u8]) -> Result<String, TryGetError> {
    let len = bytes.try_get_u16()? as usize;
    let (str, remaining) = bytes.split_at(len);
    let str = std::str::from_utf8(str).map_err(|_| TryGetError {requested: 0, available: 0})?.to_owned();
    *bytes = remaining;
    Ok(str)
}

fn extract_book_chapters(
    file: &mut std::fs::File,
    file_type: BookFileType,
    pdfium: Option<&Mutex<Pdfium>>,
) -> anyhow::Result<Vec<BookFileChapter>> {
    match file_type {
        BookFileType::EPUB => extract_epub_chapters(file),
        BookFileType::PDF => match pdfium {
            Some(pdfium) => extract_pdf_chapters(file, pdfium),
            None => Err(anyhow::anyhow!("Pdfium library is not loaded")),
        },
    }
}

fn extract_epub_chapters(file: &mut std::fs::File) -> anyhow::Result<Vec<BookFileChapter>> {
    file.seek(SeekFrom::Start(0))?;
    let epub = Epub::read(file.try_clone()?)?;
    let Some(toc) = epub.toc().contents() else {
        return Ok(Vec::new());
    };

    Ok(toc
        .flatten()
        .filter_map(|entry| {
            let title = entry.label().trim();
            let href = entry.href()?.as_str();
            (!title.is_empty()).then(|| (title.to_owned(), href.to_owned()))
        })
        .enumerate()
        .map(|(position, (title, href))| BookFileChapter {
            position: position as u32,
            title,
            location: ChapterLocation::Epub { href, cfi: None },
        })
        .collect())
}

fn extract_pdf_chapters(
    file: &mut std::fs::File,
    pdfium: &Mutex<Pdfium>,
) -> anyhow::Result<Vec<BookFileChapter>> {
    file.seek(SeekFrom::Start(0))?;
    let pdfium = pdfium
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium lock poisoned"))?;
    let document = pdfium.load_pdf_from_reader(file, None)?;
    let page_count = u32::try_from(document.pages().len())?;
    let bookmarks = document
        .bookmarks()
        .iter()
        .filter_map(|bookmark| {
            let title = bookmark.title()?;
            let title = title.trim();
            let page_index = bookmark.destination()?.page_index().ok()?;
            let start_page = u32::try_from(page_index).ok()?.checked_add(1)?;
            (!title.is_empty() && start_page <= page_count).then(|| (title.to_owned(), start_page))
        })
        .collect::<Vec<_>>();

    Ok(bookmarks
        .iter()
        .enumerate()
        .map(|(position, (title, start_page))| {
            let end_page = bookmarks
                .iter()
                .skip(position + 1)
                .map(|(_, page)| *page)
                .find(|page| page > start_page)
                .map(|page| page - 1)
                .unwrap_or(page_count)
                .max(*start_page);
            BookFileChapter {
                position: position as u32,
                title: title.clone(),
                location: ChapterLocation::Pdf {
                    start_page: *start_page,
                    end_page,
                },
            }
        })
        .collect())
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

fn blank_cover() -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(1000, 1600, Rgba([255, 255, 255, 255]));

    let mut buf = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .expect("encode blank cover");
    buf
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
