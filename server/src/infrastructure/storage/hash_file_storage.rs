use blake3::Hash;
use dashmap::DashSet;
use std::{io::Write, path::PathBuf, sync::Arc};

use tokio::{
    fs::File,
    io::{self, AsyncRead, AsyncReadExt},
};

use rayon::prelude::*;

pub struct HashFileStorage {
    dir: PathBuf,
    ext: String,
    files: DashSet<blake3::Hash>,
}

impl HashFileStorage {
    pub fn new(dir: PathBuf, ext: String) -> Self {
        Self {
            dir: dir,
            ext: ext,
            files: DashSet::new(),
        }
    }
    pub fn scan(&mut self) -> std::io::Result<()> {
        let files = Arc::new(DashSet::new());

        std::fs::read_dir(&self.dir)?
            .into_iter()
            .filter(|res| {
                // 过滤出符合扩展名的文件
                res.as_ref().is_ok_and(|entry| {
                    entry.path().is_file()
                        && !entry.path().file_stem().is_some()
                        && entry.path().extension().and_then(|s| s.to_str())
                            == Some(self.ext.as_str())
                })
            })
            .map(|res| res.unwrap().path())
            .par_bridge()
            .for_each(|path| {
                // 并发计算每个文件名是否与计算出的hash匹配
                let filename_hash =
                    match blake3::Hash::from_hex(path.file_stem().unwrap().to_str().unwrap()) {
                        Ok(h) => h,
                        Err(e) => {
                            tracing::warn!(
                                "Skipped: Invalid blake3 hex in filename {:?}: {}",
                                path,
                                e
                            );
                            return;
                        }
                    };

                let mut hasher = blake3::Hasher::new();
                let mut file = std::fs::File::open(&path).unwrap();
                let count = std::io::copy(&mut file, &mut hasher);
                if count.is_err() {
                    tracing::warn!( // 我也不知道为什么会不可用，所以暂时只能写的这么模糊
                        "Skipped: Invalid file {:?}: {}",
                        path,
                        count.err().unwrap()
                    );
                    return;
                }

                let calculated_hash = hasher.finalize();

                if filename_hash == calculated_hash {
                    files.insert(calculated_hash);
                } else {
                    tracing::warn!(
                        "Skipped: Hash mismatch for file: {:?}. Expected: {}, Calculated: {}",
                        path,
                        filename_hash.to_hex(),
                        calculated_hash.to_hex()
                    );
                }
            });
        self.files = Arc::into_inner(files).unwrap();
        Ok(())
    }

    pub fn contains(&self, key: &Hash) -> bool {
        self.files.contains(&key)
    }

    pub async fn save<R>(&self, mut reader: R) -> io::Result<()>
    where
        R: AsyncRead + Unpin,
    {
        // 必须保证临时目录和目标目录在同一文件系统中，否则无法保证原子性。
        let mut temp_file = tempfile::NamedTempFile::new_in(&self.dir.join(".temp"))?;
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
        let final_path = self
            .dir
            .join(file_hash.to_hex().to_string())
            .with_extension(&self.ext);

        match temp_file.persist_noclobber(&final_path) {
            Ok(_) => {
                self.files.insert(file_hash);
                Ok(())
            }
            Err(e) => Err(e.error),
        }
    }

    pub async fn open(&self, hash: &Hash) -> io::Result<File> {
        if self.files.contains(hash) {
            File::open(
                self.dir
                    .join(hash.to_hex().to_string())
                    .with_extension(&self.ext),
            )
            .await
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
}
