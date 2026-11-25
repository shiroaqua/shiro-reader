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

pub(crate) struct EpubSession {
    pub(crate) session_id: EpubSessionId,
    pub(crate) book_id: Arc<str>,
    pub(crate) source: BookByteSource,
    pub(crate) index: Arc<ZipIndex>,
    last_accessed: AtomicU64,
}

pub(crate) struct EpubSessionManager {
    sessions: RwLock<HashMap<EpubSessionId, Arc<EpubSession>>>,
    access_clock: AtomicU64,
    chunk_cache: Arc<LruChunkCache>,
}

impl EpubSessionManager {
    pub fn new(cache_max_bytes: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            access_clock: AtomicU64::new(1),
            chunk_cache: Arc::new(LruChunkCache::new(cache_max_bytes)),
        }
    }

    pub fn chunk_cache(&self) -> &Arc<LruChunkCache> {
        &self.chunk_cache
    }

    fn next_tick(&self) -> u64 {
        self.access_clock.fetch_add(1, Ordering::Relaxed)
    }

    pub fn insert(
        &self,
        book_id: String,
        source: BookByteSource,
        index: ZipIndex,
    ) -> Arc<EpubSession> {
        let session_id = EpubSessionId::new();
        let session = Arc::new(EpubSession {
            session_id,
            book_id: Arc::from(book_id),
            source,
            index: Arc::new(index),
            last_accessed: AtomicU64::new(self.next_tick()),
        });

        let mut guard = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        while guard.len() >= MAX_OPEN_SESSIONS {
            let Some(oldest_id) = guard
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed.load(Ordering::Relaxed))
                .map(|(id, _)| *id)
            else {
                break;
            };
            guard.remove(&oldest_id);
        }
        guard.insert(session_id, Arc::clone(&session));
        session
    }

    pub fn get(&self, session_id: EpubSessionId) -> Option<Arc<EpubSession>> {
        let session = {
            let guard = self.sessions.read().unwrap_or_else(|e| e.into_inner());
            guard.get(&session_id).cloned()
        }?;
        session
            .last_accessed
            .store(self.next_tick(), Ordering::Relaxed);
        Some(session)
    }

    pub fn remove(&self, session_id: EpubSessionId) {
        self.sessions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&session_id);
    }

    pub fn evict_books(&self, book_ids: &[String]) {
        if book_ids.is_empty() {
            return;
        }
        let mut guard = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, session| {
            !book_ids
                .iter()
                .any(|id| id.as_str() == session.book_id.as_ref())
        });
    }

    pub fn clear_all(&self) {
        self.sessions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.chunk_cache.clear_all();
    }
}
