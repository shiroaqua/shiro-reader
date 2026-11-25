use std::sync::Arc;

use blake3::Hash;
use bytes::Bytes;
use shiro_error_catalog::ErrorCode;
use shiro_infrastructure::library_runtime::SqliteBookFileService;

use crate::cloud_client::CloudClient;

use super::chunk_cache::LruChunkCache;

/// Unified immutable random-access byte source hiding Local vs Cloud storage differences.
#[derive(Clone)]
pub(crate) enum BookByteSource {
    Local {
        book_files: Arc<SqliteBookFileService>,
        hash: Hash,
        total_size: u64,
    },
    Cloud {
        client: Arc<CloudClient>,
        cache: Arc<LruChunkCache>,
        book_id: Arc<str>,
        hash: Hash,
        total_size: u64,
    },
    #[cfg(test)]
    Memory(Bytes),
}

impl BookByteSource {
    pub fn total_size(&self) -> u64 {
        match self {
            Self::Local { total_size, .. } | Self::Cloud { total_size, .. } => *total_size,
            #[cfg(test)]
            Self::Memory(bytes) => bytes.len() as u64,
        }
    }

    pub async fn read_exact_at(&self, offset: u64, len: usize) -> Result<Bytes, ErrorCode> {
        let total = self.total_size();
        if len == 0 {
            return if offset <= total {
                Ok(Bytes::new())
            } else {
                Err(ErrorCode::ReaderCorruptedEpub)
            };
        }
        let len_u64 = u64::try_from(len).map_err(|_| ErrorCode::ReaderCorruptedEpub)?;
        let end = offset
            .checked_add(len_u64)
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;
        if end > total {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        match self {
            Self::Local {
                book_files, hash, ..
            } => book_files
                .read_book_file_exact_at(hash, offset, len)
                .await
                .map_err(|err| ErrorCode::from(&err)),
            Self::Cloud {
                client,
                cache,
                book_id,
                hash,
                total_size,
            } => {
                cache
                    .read_range(client, book_id, *hash, *total_size, offset, len)
                    .await
            }
            #[cfg(test)]
            Self::Memory(bytes) => Ok(bytes.slice(offset as usize..end as usize)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_byte_source_checks_bounds_and_slices_accurately() {
        let source = BookByteSource::Memory(Bytes::from_static(b"0123456789"));
        assert_eq!(source.total_size(), 10);
        assert_eq!(source.read_exact_at(2, 4).await.unwrap().as_ref(), b"2345");
        assert!(source.read_exact_at(10, 0).await.unwrap().is_empty());
        assert_eq!(
            source.read_exact_at(10, 1).await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );
        assert_eq!(
            source.read_exact_at(u64::MAX - 2, 10).await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );
    }
}
