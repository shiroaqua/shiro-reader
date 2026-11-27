use std::{
    fmt,
    future::Future,
    io::{self, ErrorKind},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use blake3::Hash;
use bytes::Bytes;
use dashmap::{DashMap, mapref::entry::Entry};
use shiro_domain::library::book::{
    navigation::BookNavigation, value_objects::BookFileType,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    time::{Instant, Sleep},
};

use crate::library::{
    book::file::errors::BookFileApplicationError,
    errors::LibraryApplicationError,
    ports::{
        BookFileMetadata, BookFileStorageError, BookFileStore, CoverByteStream,
        ExtractedBookMetadata, OpenedBookFile,
    },
};

const DEFAULT_BASE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MIN_BYTES_PER_SEC: u64 = 64 * 1024;
const DEFAULT_MIN_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_TIMEOUT_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadTimeoutPolicy {
    pub base_timeout: Duration,
    pub min_bytes_per_sec: u64,
    pub min_timeout: Duration,
    pub max_timeout: Duration,
}

impl Default for UploadTimeoutPolicy {
    fn default() -> Self {
        Self {
            base_timeout: Duration::from_secs(DEFAULT_BASE_TIMEOUT_SECS),
            min_bytes_per_sec: DEFAULT_MIN_BYTES_PER_SEC,
            min_timeout: Duration::from_secs(DEFAULT_MIN_TIMEOUT_SECS),
            max_timeout: Duration::from_secs(DEFAULT_MAX_TIMEOUT_SECS),
        }
    }
}

impl UploadTimeoutPolicy {
    /// Computes the stream receive timeout budget for a declared net byte size.
    ///
    /// Uses `u128` ceiling division and saturating arithmetic so arbitrary
    /// `declared_size` values (including `u64::MAX`) never overflow or panic.
    pub fn timeout_for_size(&self, declared_size: u64) -> Duration {
        let rate = u128::from(self.min_bytes_per_sec.max(1));
        let transfer_ms = u128::from(declared_size)
            .saturating_mul(1_000)
            .div_ceil(rate);
        let transfer = Duration::from_millis(u64::try_from(transfer_ms).unwrap_or(u64::MAX));
        let computed = self.base_timeout.saturating_add(transfer);
        computed.max(self.min_timeout).min(self.max_timeout)
    }
}

#[derive(Debug, Clone, Copy)]
enum UploadLeasePhase {
    Streaming { deadline: Instant },
    Finalizing,
}

#[derive(Debug, Clone, Copy)]
struct UploadLease {
    lease_id: u64,
    phase: UploadLeasePhase,
}

impl UploadLease {
    fn is_active(self, now: Instant) -> bool {
        match self.phase {
            UploadLeasePhase::Streaming { deadline } => now < deadline,
            UploadLeasePhase::Finalizing => true,
        }
    }
}

#[derive(Debug)]
struct UploadStreamTimeoutError;

impl fmt::Display for UploadStreamTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("book file upload stream timed out")
    }
}

impl std::error::Error for UploadStreamTimeoutError {}

#[derive(Debug)]
struct SizeMismatchError;

impl fmt::Display for SizeMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("book file stream size does not match declared size")
    }
}

impl std::error::Error for SizeMismatchError {}

pub struct BookFileService<S> {
    storage: S,
    uploading: Arc<DashMap<Hash, UploadLease>>,
    next_lease_id: AtomicU64,
    timeout_policy: UploadTimeoutPolicy,
}

impl<S: BookFileStore> BookFileService<S> {
    pub fn new(storage: S) -> Self {
        Self::with_timeout_policy(storage, UploadTimeoutPolicy::default())
    }

    pub fn with_timeout_policy(storage: S, timeout_policy: UploadTimeoutPolicy) -> Self {
        Self {
            storage,
            uploading: Arc::new(DashMap::new()),
            next_lease_id: AtomicU64::new(1),
            timeout_policy,
        }
    }

    pub async fn upload_book_file<R>(
        &self,
        hash: String,
        declared_size: u64,
        reader: R,
    ) -> Result<(), LibraryApplicationError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let parsed_hash = Self::to_hex(&hash)?;
        if self.storage.contains(&parsed_hash) {
            return Err(BookFileApplicationError::AlreadyExists.into());
        }

        let now = Instant::now();
        let timeout = self.timeout_policy.timeout_for_size(declared_size);
        let deadline = compute_deadline(now, timeout, self.timeout_policy.max_timeout);
        let lease_id = self.next_lease_id.fetch_add(1, Ordering::Relaxed);
        let guard = UploadGuard::try_acquire(
            parsed_hash,
            lease_id,
            deadline,
            now,
            Arc::clone(&self.uploading),
        )?;

        if self.storage.contains(&guard.hash) {
            return Err(BookFileApplicationError::AlreadyExists.into());
        }

        let exact_reader = ExactSizeReader::new(
            reader,
            declared_size,
            deadline,
            guard.lease_handle(),
        );

        match self.storage.import(&guard.hash, exact_reader).await {
            Ok(()) => Ok(()),
            Err(BookFileStorageError::Io(err)) => {
                if err
                    .get_ref()
                    .is_some_and(|inner| inner.is::<UploadStreamTimeoutError>())
                {
                    return Err(BookFileApplicationError::UploadTimeout.into());
                }
                if err
                    .get_ref()
                    .is_some_and(|inner| inner.is::<SizeMismatchError>())
                {
                    return Err(BookFileApplicationError::SizeMismatch.into());
                }
                Err(BookFileStorageError::Io(err).into())
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn get_book_meta(&self, hash: &Hash) -> Option<BookFileMetadata> {
        self.storage.get_meta(hash)
    }

    pub async fn open_book_file_range(
        &self,
        hash: &Hash,
        offset: u64,
        length: Option<u64>,
    ) -> Result<OpenedBookFile<S::Reader>, LibraryApplicationError> {
        self.storage
            .open_range(hash, offset, length)
            .await
            .map_err(LibraryApplicationError::from)
    }

    pub async fn read_book_file_exact_at(
        &self,
        hash: &Hash,
        offset: u64,
        len: usize,
    ) -> Result<Bytes, LibraryApplicationError> {
        self.storage
            .read_exact_at(hash, offset, len)
            .await
            .map_err(LibraryApplicationError::from)
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

    pub async fn extract_book_navigation(&self, hash: &Hash) -> BookNavigation {
        self.storage.extract_navigation(hash).await
    }

    pub async fn extract_book_metadata(&self, hash: &Hash) -> ExtractedBookMetadata {
        self.storage.extract_metadata(hash).await
    }

    pub fn parse_existing_hash(&self, hash: &str) -> Result<Hash, LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        if self.storage.contains(&hash) {
            Ok(hash)
        } else {
            Err(BookFileApplicationError::NotFound.into())
        }
    }

    pub fn probe_book_file(&self, hash: &str) -> Result<Hash, LibraryApplicationError> {
        let hash = Self::to_hex(hash)?;
        if self.storage.contains(&hash) {
            return Ok(hash);
        }
        let now = Instant::now();
        if self
            .uploading
            .get(&hash)
            .is_some_and(|lease| lease.is_active(now))
        {
            return Err(BookFileApplicationError::UploadConflict.into());
        }
        Err(BookFileApplicationError::NotFound.into())
    }

    pub fn get_book_type(&self, hash: &Hash) -> Option<BookFileType> {
        self.storage.get_type(hash)
    }

    #[inline]
    fn to_hex(hash: &str) -> Result<Hash, BookFileApplicationError> {
        Hash::from_hex(hash).map_err(|_| BookFileApplicationError::InvalidHashFormat)
    }
}

fn compute_deadline(now: Instant, timeout: Duration, max_timeout: Duration) -> Instant {
    now.checked_add(timeout)
        .or_else(|| now.checked_add(max_timeout))
        .or_else(|| now.checked_add(Duration::from_secs(DEFAULT_MAX_TIMEOUT_SECS)))
        .unwrap_or(now)
}

struct UploadLeaseHandle {
    hash: Hash,
    lease_id: u64,
    uploading: Arc<DashMap<Hash, UploadLease>>,
}

impl UploadLeaseHandle {
    fn mark_finalizing(&self, now: Instant) -> bool {
        if let Some(mut entry) = self.uploading.get_mut(&self.hash)
            && entry.lease_id == self.lease_id
            && entry.is_active(now)
        {
            entry.phase = UploadLeasePhase::Finalizing;
            return true;
        }
        false
    }
}

struct UploadGuard {
    hash: Hash,
    lease_id: u64,
    uploading: Arc<DashMap<Hash, UploadLease>>,
}

impl UploadGuard {
    fn try_acquire(
        hash: Hash,
        lease_id: u64,
        deadline: Instant,
        now: Instant,
        uploading: Arc<DashMap<Hash, UploadLease>>,
    ) -> Result<Self, BookFileApplicationError> {
        let new_lease = UploadLease {
            lease_id,
            phase: UploadLeasePhase::Streaming { deadline },
        };
        let acquired = match uploading.entry(hash) {
            Entry::Vacant(slot) => {
                slot.insert(new_lease);
                true
            }
            Entry::Occupied(mut slot) => {
                if slot.get().is_active(now) {
                    false
                } else {
                    *slot.get_mut() = new_lease;
                    true
                }
            }
        };

        if acquired {
            Ok(Self {
                hash,
                lease_id,
                uploading,
            })
        } else {
            Err(BookFileApplicationError::UploadConflict)
        }
    }

    fn lease_handle(&self) -> UploadLeaseHandle {
        UploadLeaseHandle {
            hash: self.hash,
            lease_id: self.lease_id,
            uploading: Arc::clone(&self.uploading),
        }
    }
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        self.uploading
            .remove_if(&self.hash, |_, lease| lease.lease_id == self.lease_id);
    }
}

struct ExactSizeReader<R> {
    reader: R,
    declared_size: u64,
    read_bytes: u64,
    deadline: Instant,
    sleep: Pin<Box<Sleep>>,
    lease: UploadLeaseHandle,
    eof_reached: bool,
}

impl<R> ExactSizeReader<R> {
    fn new(reader: R, declared_size: u64, deadline: Instant, lease: UploadLeaseHandle) -> Self {
        Self {
            reader,
            declared_size,
            read_bytes: 0,
            deadline,
            sleep: Box::pin(tokio::time::sleep_until(deadline)),
            lease,
            eof_reached: false,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ExactSizeReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.eof_reached || buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let before = buf.filled().len();
        match Pin::new(&mut this.reader).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let bytes_read = buf.filled().len() - before;
                if bytes_read == 0 {
                    let now = Instant::now();
                    if now >= this.deadline {
                        return Poll::Ready(Err(io::Error::new(
                            ErrorKind::TimedOut,
                            UploadStreamTimeoutError,
                        )));
                    }
                    if this.read_bytes != this.declared_size {
                        return Poll::Ready(Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            SizeMismatchError,
                        )));
                    }
                    if !this.lease.mark_finalizing(now) {
                        return Poll::Ready(Err(io::Error::new(
                            ErrorKind::TimedOut,
                            UploadStreamTimeoutError,
                        )));
                    }
                    this.eof_reached = true;
                    return Poll::Ready(Ok(()));
                }

                let Some(next_total) = this.read_bytes.checked_add(bytes_read as u64) else {
                    buf.set_filled(before);
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        SizeMismatchError,
                    )));
                };
                this.read_bytes = next_total;

                if this.read_bytes > this.declared_size {
                    buf.set_filled(before);
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        SizeMismatchError,
                    )));
                }

                if Instant::now() >= this.deadline {
                    buf.set_filled(before);
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::TimedOut,
                        UploadStreamTimeoutError,
                    )));
                }

                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => {
                if this.sleep.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::TimedOut,
                        UploadStreamTimeoutError,
                    )));
                }
                Poll::Pending
            }
        }
    }
}
