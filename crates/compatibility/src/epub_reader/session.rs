use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{
    EpubSessionId, MAX_OPEN_SESSIONS, byte_source::BookByteSource, chunk_cache::LruChunkCache,
    zip_index::ZipIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionGuardToken {
    lifecycle_epoch: u64,
    eviction_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InsertSessionError {
    LifecycleChanged,
    EvictionOccurred,
}

struct SessionPoolState {
    sessions: HashMap<EpubSessionId, Arc<EpubSession>>,
    lifecycle_epoch: u64,
    eviction_epoch: u64,
}

pub(crate) struct EpubSession {
    pub(crate) session_id: EpubSessionId,
    pub(crate) book_id: Arc<str>,
    pub(crate) source: BookByteSource,
    pub(crate) index: Arc<ZipIndex>,
    last_accessed: AtomicU64,
}

pub(crate) struct EpubSessionManager {
    state: RwLock<SessionPoolState>,
    access_clock: AtomicU64,
    chunk_cache: Arc<LruChunkCache>,
}

impl EpubSessionManager {
    pub fn new(cache_max_bytes: usize) -> Self {
        Self {
            state: RwLock::new(SessionPoolState {
                sessions: HashMap::new(),
                lifecycle_epoch: 0,
                eviction_epoch: 0,
            }),
            access_clock: AtomicU64::new(1),
            chunk_cache: Arc::new(LruChunkCache::new(cache_max_bytes)),
        }
    }

    pub fn chunk_cache(&self) -> &Arc<LruChunkCache> {
        &self.chunk_cache
    }

    pub fn snapshot_token(&self) -> SessionGuardToken {
        let guard = self.state.read().unwrap_or_else(|e| e.into_inner());
        SessionGuardToken {
            lifecycle_epoch: guard.lifecycle_epoch,
            eviction_epoch: guard.eviction_epoch,
        }
    }

    fn next_tick(&self) -> u64 {
        self.access_clock.fetch_add(1, Ordering::Relaxed)
    }

    pub fn try_insert(
        &self,
        expected: SessionGuardToken,
        book_id: String,
        source: BookByteSource,
        index: Arc<ZipIndex>,
    ) -> Result<Arc<EpubSession>, InsertSessionError> {
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        if guard.lifecycle_epoch != expected.lifecycle_epoch {
            return Err(InsertSessionError::LifecycleChanged);
        }
        if guard.eviction_epoch != expected.eviction_epoch {
            return Err(InsertSessionError::EvictionOccurred);
        }

        let session_id = EpubSessionId::new();
        let session = Arc::new(EpubSession {
            session_id,
            book_id: Arc::from(book_id),
            source,
            index,
            last_accessed: AtomicU64::new(self.next_tick()),
        });

        while guard.sessions.len() >= MAX_OPEN_SESSIONS {
            let Some(oldest_id) = guard
                .sessions
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed.load(Ordering::Relaxed))
                .map(|(id, _)| *id)
            else {
                break;
            };
            guard.sessions.remove(&oldest_id);
        }
        guard.sessions.insert(session_id, Arc::clone(&session));
        Ok(session)
    }

    pub fn get(&self, session_id: EpubSessionId) -> Option<Arc<EpubSession>> {
        let session = {
            let guard = self.state.read().unwrap_or_else(|e| e.into_inner());
            guard.sessions.get(&session_id).cloned()
        }?;
        session
            .last_accessed
            .store(self.next_tick(), Ordering::Relaxed);
        Some(session)
    }

    pub fn remove(&self, session_id: EpubSessionId) {
        self.state
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .remove(&session_id);
    }

    pub fn evict_books(&self, book_ids: &[String]) {
        if book_ids.is_empty() {
            return;
        }
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        guard.eviction_epoch = guard.eviction_epoch.wrapping_add(1);
        guard.sessions.retain(|_, session| {
            !book_ids
                .iter()
                .any(|id| id.as_str() == session.book_id.as_ref())
        });
    }

    pub fn clear_all(&self) {
        {
            let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
            guard.lifecycle_epoch = guard.lifecycle_epoch.wrapping_add(1);
            guard.sessions.clear();
        }
        self.chunk_cache.clear_all();
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    const EMPTY_ZIP_EOCD: [u8; 22] = [
        0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    async fn empty_zip_index() -> (BookByteSource, Arc<ZipIndex>) {
        let source = BookByteSource::Memory(Bytes::from_static(&EMPTY_ZIP_EOCD));
        let index = ZipIndex::parse(&source).await.expect("parse empty zip");
        (source, Arc::new(index))
    }

    #[tokio::test]
    async fn try_insert_validates_lifecycle_and_eviction_epochs() {
        let manager = EpubSessionManager::new(1024 * 1024);
        let (source, index) = empty_zip_index().await;

        let fresh = manager.snapshot_token();
        let inserted = manager
            .try_insert(fresh, "b1".to_owned(), source.clone(), Arc::clone(&index))
            .expect("fresh token inserts");
        assert!(manager.get(inserted.session_id).is_some());

        let before_clear = manager.snapshot_token();
        manager.clear_all();
        assert!(manager.get(inserted.session_id).is_none());
        assert_eq!(
            manager
                .try_insert(
                    before_clear,
                    "b1".to_owned(),
                    source.clone(),
                    Arc::clone(&index),
                )
                .err(),
            Some(InsertSessionError::LifecycleChanged)
        );
        assert_eq!(
            manager
                .state
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .sessions
                .len(),
            0
        );

        let before_evict = manager.snapshot_token();
        manager.evict_books(&["b1".to_owned()]);
        assert_eq!(
            manager
                .try_insert(before_evict, "b1".to_owned(), source, index)
                .err(),
            Some(InsertSessionError::EvictionOccurred)
        );
        assert_eq!(
            manager
                .state
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .sessions
                .len(),
            0
        );
    }
}
