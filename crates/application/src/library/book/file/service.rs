use std::{io::ErrorKind, sync::Arc};

use blake3::Hash;
use dashmap::DashSet;
use shiro_domain::library::book::file::entity::BookFileType;
use tokio::{fs::File, io::AsyncRead};

use crate::library::{
    book::file::errors::BookFileApplicationError,
    errors::LibraryApplicationError,
    ports::{BookFileStore, CoverByteStream},
};

pub struct BookFileService<S> {
    storage: S,
    uploading: Arc<DashSet<Hash>>,
}

impl<S: BookFileStore> BookFileService<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            uploading: Arc::new(DashSet::new()),
        }
    }

    pub async fn upload_book_file<R>(
        &self,
        hash: String,
        reader: R,
    ) -> Result<(), LibraryApplicationError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let guard = UploadGuard::try_new(Self::to_hex(&hash)?, Arc::clone(&self.uploading))?;

        if self.storage.contains(&guard.hash) {
            return Err(BookFileApplicationError::AlreadyExists.into());
        }

        match self.storage.save(&guard.hash, reader).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                Err(BookFileApplicationError::AlreadyExists.into())
            }
            Err(e) if e.kind() == ErrorKind::InvalidData => {
                Err(BookFileApplicationError::HashMismatch.into())
            }
            Err(e) if e.kind() == ErrorKind::Unsupported => {
                Err(BookFileApplicationError::Unsupported.into())
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn download_book_file(&self, hash: &str) -> Result<File, LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        if self.storage.contains(&hash) {
            self.storage
                .open(&hash)
                .await
                .map_err(LibraryApplicationError::from)
        } else {
            Err(BookFileApplicationError::NotFound.into())
        }
    }

    pub async fn download_book_cover_file(
        &self,
        hash: Hash,
    ) -> Result<(CoverByteStream, String), LibraryApplicationError> {
        self.storage.open_cover(hash).await.map_err(|e| {
            LibraryApplicationError::BookFile(BookFileApplicationError::Storage(
                anyhow::Error::new(e),
            ))
        })
    }

    pub fn parse_existing_hash(&self, hash: &str) -> Result<Hash, LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        if self.storage.contains(&hash) {
            Ok(hash)
        } else {
            Err(BookFileApplicationError::NotFound.into())
        }
    }

    pub fn get_book_type(&self, hash: &Hash) -> Option<BookFileType> {
        self.storage.get_type(hash)
    }

    pub async fn delete_book_file(&self, hash: &str) -> Result<(), LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        let _guard = UploadGuard::try_new(hash, Arc::clone(&self.uploading))?;
        self.storage
            .remove(&hash)
            .await
            .map_err(LibraryApplicationError::from)
    }

    #[inline]
    fn to_hex(hash: &str) -> Result<Hash, BookFileApplicationError> {
        Hash::from_hex(hash).map_err(|_| BookFileApplicationError::InvalidHashFormat)
    }
}

struct UploadGuard {
    hash: Hash,
    set: Arc<DashSet<Hash>>,
}

impl UploadGuard {
    fn try_new(hash: Hash, set: Arc<DashSet<Hash>>) -> Result<Self, BookFileApplicationError> {
        if set.insert(hash) {
            Ok(Self { hash, set })
        } else {
            Err(BookFileApplicationError::UploadConflict)
        }
    }
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        self.set.remove(&self.hash);
    }
}
