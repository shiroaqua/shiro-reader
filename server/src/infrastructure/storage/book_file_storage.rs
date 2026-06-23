use blake3::Hash;
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::{Stream, StreamExt, stream};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use pdfium_render::prelude::{PdfPageRenderRotation, PdfRenderConfig, Pdfium};
use std::{
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio_util::io::ReaderStream;

use tokio::{
    fs::File,
    io::{self, AsyncRead, AsyncReadExt, AsyncSeekExt},
};

use rayon::prelude::*;

use crate::domain::library::book::file::entity::BookFileType;

pub struct BookFileStorage {
    dir: PathBuf,
    ext: String,
    files: DashMap<blake3::Hash, BookFileType>,
    pdfium: Pdfium,
}

impl BookFileStorage {
    pub fn new(dir: PathBuf, ext: String, pdfium: Pdfium) -> Self {
        // 如果无法创建目录理应直接崩溃
        std::fs::create_dir_all(dir.join("covers")).unwrap();
        std::fs::create_dir_all(dir.join(".temp")).unwrap(); // 必须保证临时目录和目标目录在同一文件系统中，否则无法保证原子性。

        Self {
            dir: dir,
            ext: ext,
            files: DashMap::new(),
            pdfium: pdfium,
        }
    }
    pub fn scan(&mut self) -> std::io::Result<()> {
        let files = Arc::new(DashMap::new());

        std::fs::read_dir(&self.dir)?
            .into_iter()
            .filter(|res| {
                // 过滤出符合扩展名的文件
                res.as_ref().is_ok_and(|entry| {
                    entry.path().is_file()
                        && entry.path().file_stem().is_some()
                        && entry.path().extension().and_then(|s| s.to_str())
                            == Some(self.ext.as_str())
                })
            })
            .map(|res| res.unwrap().path())
            .par_bridge()
            .for_each(|path| {
                // 并发计算每个文件名是否与计算出的hash匹配
                let file_name = path.file_name().unwrap().display();
                let mut hasher = blake3::Hasher::new();
                let mut file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!("Skipped: Cannot open file {}: {}", file_name, e);
                        return;
                    }
                };
                let file_type = match read_file_type_from_book_file(&mut file, 1) {
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

                let count = std::io::copy(&mut file, &mut hasher);
                if let Err(e) = count {
                    tracing::warn!("Skipped: Invalid file {}: {}", file_name, e);
                    return;
                }

                let calculated_hash = hasher.finalize();

                if file_hash == calculated_hash {
                    files.insert(calculated_hash, file_type);
                    tracing::debug!("loaded: {}", file_name);
                } else {
                    tracing::warn!(
                        "Skipped: Hash mismatch for file: {}. Expected: {}, Calculated: {}",
                        file_name,
                        file_hash.to_hex(),
                        calculated_hash.to_hex()
                    );
                }
            });
        tracing::info!("Loaded {} books", files.len());
        self.files = Arc::into_inner(files).unwrap();
        Ok(())
    }

    pub fn contains(&self, key: &Hash) -> bool {
        self.files.contains_key(&key)
    }

    pub fn get_type(&self, key: &Hash) -> Option<BookFileType> {
        if self.contains(key) {
            // 书籍永不删除，因此这边的线程安全问题无需在意.
            Some((*self.files.get(key).unwrap()).clone())
        } else {
            None
        }
    }

    pub async fn save<R>(&self, hash: &Hash, mut reader: R) -> io::Result<()>
    where
        R: AsyncRead + Unpin,
    {
        let path = self.dir.join(".temp");
        let mut temp_file = tempfile::NamedTempFile::new_in(&path)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let bytes_read = reader.read(&mut buf).await?;
            if bytes_read == 0 {
                break;
            }
            let buf = &buf[..bytes_read];
            hasher.update(&buf);
            temp_file.write_all(&buf)?;
        }
        temp_file.as_file_mut().flush()?;

        let file_hash = hasher.finalize();
        if !hash.eq(&file_hash) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        let file_type = get_file_type(temp_file.as_file_mut())?;
        let mut final_temp_file = tempfile::NamedTempFile::new_in(&path)?;
        final_temp_file.write_all(&vec![0])?; // version
        final_temp_file.write_all(&vec![match file_type {
            BookFileType::PDF => 0,
            BookFileType::EPUB => 1,
        }])?;
        final_temp_file.write_all(file_hash.as_bytes())?;

        let cover = match file_type {
            BookFileType::PDF => extract_pdf_cover(temp_file.as_file_mut(), &self.pdfium),
            BookFileType::EPUB => extract_epub_cover(temp_file.as_file_mut()),
        };

        temp_file.seek(std::io::SeekFrom::Start(0))?;
        std::io::copy(&mut temp_file, &mut final_temp_file)?;
        let _ = temp_file.close(); // 提早清除文件（报错无所谓）

        let hash = file_hash.to_hex().to_string();
        if let Ok(cover) = cover {
            let mut temp_cover_file = tempfile::NamedTempFile::new_in(&path)?;
            temp_cover_file.write_all(&cover)?;
            drop(cover);

            temp_cover_file
                .persist_noclobber(&self.dir.join("covers").join(&hash).with_extension("png"))?;
        }

        match final_temp_file.persist_noclobber(&self.dir.join(&hash).with_extension(&self.ext)) {
            Ok(_) => {
                self.files.insert(file_hash, file_type);
                Ok(())
            }
            Err(e) => Err(e.error),
        }
    }

    pub async fn open_cover(
        &self,
        key: Hash,
    ) -> io::Result<impl Stream<Item = io::Result<Bytes>> + 'static> {
        let path = self
            .dir
            .join("covers")
            .join(key.to_hex().to_string())
            .with_extension("png");

        if path.exists() {
            let file = File::open(path).await?;
            let stream = ReaderStream::new(file);
            Ok(stream.boxed())
        } else {
            let data = blank_cover();
            let stream = stream::once(async move { Ok(Bytes::from(data)) });
            Ok(stream.boxed())
        }
    }

    pub async fn open(&self, hash: &Hash) -> io::Result<File> {
        if self.files.contains_key(hash) {
            let mut file = File::open(
                self.dir
                    .join(hash.to_hex().to_string())
                    .with_extension(&self.ext),
            )
            .await?;
            file.seek(std::io::SeekFrom::Start(2 + 32)).await?;
            Ok(file)
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
}

fn read_file_hash_from_book_file(file: &mut std::fs::File) -> io::Result<Hash> {
    let mut buf = [0u8; 32];
    file.read_exact(&mut buf)?;
    Ok(Hash::from_bytes(buf))
}

fn read_file_type_from_book_file(
    file: &mut std::fs::File,
    offset: u64,
) -> io::Result<BookFileType> {
    let mut buf = [0u8; 1];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    match buf[0] {
        0 => Ok(BookFileType::PDF),
        1 => Ok(BookFileType::EPUB),
        _ => Err(io::Error::from(io::ErrorKind::Unsupported)),
    }
}

fn extract_pdf_cover(file: &mut std::fs::File, pdfium: &Pdfium) -> anyhow::Result<Vec<u8>> {
    let doc = pdfium.load_pdf_from_reader(file, None)?;
    let page = doc.pages().first()?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2000)
        .set_maximum_height(2000)
        .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

    let bitmap = page.render_with_config(&render_config)?;
    let image = bitmap.as_image()?;

    let mut png_bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;

    Ok(png_bytes)
}

fn extract_epub_cover(file: &mut std::fs::File) -> anyhow::Result<Vec<u8>> {
    let mut epub = epub::doc::EpubDoc::from_reader(std::io::BufReader::new(file))?;
    if let Some(colver) = epub.get_cover() {
        Ok(colver.0)
    } else {
        Err(anyhow::anyhow!("can't read epub cover"))
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

fn get_file_type(file: &mut std::fs::File) -> io::Result<BookFileType> {
    if compare_bytes_from_file(file, b"PK\x03\x04" /* ZIP 签名 */, 0)? {
        // 1. 检查压缩方法是否为 0（不压缩，EPUB 对 mimetype 的要求）
        file.seek(SeekFrom::Start(8))?;
        let mut compression = [0u8; 2];
        file.read_exact(&mut compression)?;
        let method = u16::from_le_bytes(compression);

        if method == 0 {
            // 2. 读取文件名长度（偏移 26）
            file.seek(SeekFrom::Start(26))?;
            let mut name_len_buf = [0u8; 2];
            file.read_exact(&mut name_len_buf)?;
            let name_len = u16::from_le_bytes(name_len_buf);

            // 3. 读取额外字段长度（偏移 28）
            let mut extra_len_buf = [0u8; 2];
            file.read_exact(&mut extra_len_buf)?;
            let extra_len = u16::from_le_bytes(extra_len_buf);

            // 4. 验证文件名是否为 "mimetype"（偏移 30，固定 8 字节）
            if name_len == 8 && compare_bytes_from_file(file, b"mimetype", 30)? {
                if extra_len == 0 {
                    // 5. 读取未压缩大小（偏移 18）
                    file.seek(SeekFrom::Start(18))?;
                    let mut size_buf = [0u8; 4];
                    file.read_exact(&mut size_buf)?;
                    let uncompressed_size = u32::from_le_bytes(size_buf);

                    // 6. 读取数据区内容
                    let data_offset = 30 + name_len as u64 + extra_len as u64;
                    file.seek(SeekFrom::Start(data_offset))?;
                    let mut content = vec![0u8; uncompressed_size as usize];
                    file.read_exact(&mut content)?;

                    // 7. 比对 mimetype 内容
                    if content == b"application/epub+zip" {
                        return Ok(BookFileType::EPUB);
                    }
                }
            }
        }
    } else if compare_bytes_from_file(file, b"%PDF-" /* PDF魔法头 */, 0)? {
        return Ok(BookFileType::PDF);
    }
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

fn compare_bytes_from_file(
    file: &mut std::fs::File,
    expected_head: &[u8],
    offset: u64,
) -> io::Result<bool> {
    let mut buf = vec![0u8; expected_head.len()];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    Ok(buf == expected_head)
}
