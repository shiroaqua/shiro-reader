use std::{io, io::ErrorKind, sync::Arc};

use crate::{
    application::library::{book::file::errors::BookFileApplicationError, errors::LibraryApplicationError},
    infrastructure::storage::hash_file_storage::HashFileStorage,
};
use blake3::Hash;
use dashmap::DashSet;
use tokio::{fs::File, io::AsyncRead};

pub struct BookFileService {
    storage: HashFileStorage,
    uploading: Arc<DashSet<Hash>>,
}

impl BookFileService {
    pub fn new(storage: HashFileStorage) -> Self {
        let uploading: Arc<DashSet<Hash>> = Arc::new(DashSet::new());
        Self {
            storage: storage,
            uploading: uploading,
        }
    }

    pub async fn upload_book_file<R>(&self, hash: String, reader: R) -> Result<(), LibraryApplicationError>
    where
        R: AsyncRead + Unpin,
    {
        let guard = UploadGuard::try_new(Self::to_hex(&hash)?, self.uploading.clone())?;

        if self.storage.contains(&guard.hash) {
            return Err(BookFileApplicationError::AlreadyExists.into());
        }

        match self.storage.save(&guard.hash, reader).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(BookFileApplicationError::AlreadyExists.into()),
            Err(e) if e.kind() == ErrorKind::InvalidData => Err(BookFileApplicationError::HashMismatch.into()),
            Err(e) => Err(e.into()),
        }
    }
    pub async fn download_book_file(&self, hash: &str) -> Result<File, LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        if self.storage.contains(&hash) {
            self.storage.open(&hash).await.map_err(LibraryApplicationError::from)
        } else {
            Err(BookFileApplicationError::NotFound.into())
        }
    }
    pub fn parse_existing_hash(&self, hash: &str) -> Result<Hash, LibraryApplicationError> {
         let hash = Self::to_hex(hash)?;
         if self.storage.contains(&hash) {
            Ok(hash)
         }
         else {
             Err(BookFileApplicationError::NotFound.into())
         }
    }

    #[inline]
    fn to_hex(hash: &str) -> Result<Hash, BookFileApplicationError> {
        Hash::from_hex(hash).map_err(|_| BookFileApplicationError::InvalidHashFormat)
    }
}

struct UploadGuard {
    pub hash: Hash,
    set: Arc<DashSet<Hash>>,
}

impl UploadGuard {
    fn try_new(hash: Hash, set: Arc<DashSet<Hash>>) -> Result<Self, BookFileApplicationError> {
        if set.insert(hash.clone()) {
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


impl From<io::Error> for LibraryApplicationError {
    fn from(value: io::Error) -> Self {
        match value.kind() {
            ErrorKind::NotFound => BookFileApplicationError::NotFound,
            _ => BookFileApplicationError::Storage(value.into()),
        }.into()
    }
}