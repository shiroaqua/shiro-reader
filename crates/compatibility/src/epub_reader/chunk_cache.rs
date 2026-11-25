use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    sync::Mutex,
};

use blake3::Hash;
use bytes::{Bytes, BytesMut};
use futures_util::stream::{self, StreamExt};
use shiro_error_catalog::ErrorCode;
use tokio::sync::watch;

use crate::cloud_client::CloudClient;

use super::{ByteRange, EPUB_CHUNK_SIZE, MAX_CHUNK_CONCURRENCY};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ChunkKey {
    hash: [u8; 32],
    chunk_index: u64,
}

impl ChunkKey {
    fn new(hash: Hash, chunk_index: u64) -> Self {
        Self {
            hash: *hash.as_bytes(),
            chunk_index,
        }
    }
}

#[derive(Clone)]
enum InFlightOutcome {
    Ready(Result<Bytes, ErrorCode>),
    LeaderCancelled,
}

struct CachedChunk {
    data: Bytes,
    last_used: u64,
}

struct InFlightEntry {
    generation: u64,
    rx: watch::Receiver<Option<InFlightOutcome>>,
}

struct CacheState {
    entries: HashMap<ChunkKey, CachedChunk>,
    lru_order: BTreeSet<(u64, ChunkKey)>,
    in_flight: HashMap<ChunkKey, InFlightEntry>,
    total_bytes: usize,
    clock: u64,
    generation: u64,
    max_bytes: usize,
}

impl CacheState {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru_order: BTreeSet::new(),
            in_flight: HashMap::new(),
            total_bytes: 0,
            clock: 0,
            generation: 0,
            max_bytes,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        self.clock
    }

    fn touch(&mut self, key: ChunkKey) -> Option<Bytes> {
        let tick = self.next_tick();
        let entry = self.entries.get_mut(&key)?;
        self.lru_order.remove(&(entry.last_used, key));
        entry.last_used = tick;
        self.lru_order.insert((tick, key));
        Some(entry.data.clone())
    }

    fn insert_chunk(&mut self, key: ChunkKey, data: Bytes) {
        let size = data.len();
        if size > self.max_bytes {
            return;
        }
        if let Some(existing) = self.entries.remove(&key) {
            self.lru_order.remove(&(existing.last_used, key));
            self.total_bytes = self.total_bytes.saturating_sub(existing.data.len());
        }
        while self.total_bytes.saturating_add(size) > self.max_bytes {
            let Some((_, oldest_key)) = self.lru_order.pop_first() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest_key) {
                self.total_bytes = self.total_bytes.saturating_sub(evicted.data.len());
            }
        }
        let tick = self.next_tick();
        self.total_bytes = self.total_bytes.saturating_add(size);
        self.lru_order.insert((tick, key));
        self.entries.insert(
            key,
            CachedChunk {
                data,
                last_used: tick,
            },
        );
    }
}

struct InFlightGuard<'a> {
    cache: &'a LruChunkCache,
    key: ChunkKey,
    leader_gen: u64,
    tx: Option<watch::Sender<Option<InFlightOutcome>>>,
}

impl<'a> InFlightGuard<'a> {
    fn complete(mut self, result: Result<Bytes, ErrorCode>) -> Result<Bytes, ErrorCode> {
        if let Some(tx) = self.tx.take() {
            {
                let mut state = self.cache.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.generation == self.leader_gen {
                    state.in_flight.remove(&self.key);
                    if let Ok(ref bytes) = result {
                        state.insert_chunk(self.key, bytes.clone());
                    }
                }
            }
            tx.send_replace(Some(InFlightOutcome::Ready(result.clone())));
        }
        result
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            {
                let mut state = self.cache.state.lock().unwrap_or_else(|e| e.into_inner());
                if state
                    .in_flight
                    .get(&self.key)
                    .is_some_and(|entry| entry.generation == self.leader_gen)
                {
                    state.in_flight.remove(&self.key);
                }
            }
            tx.send_replace(Some(InFlightOutcome::LeaderCancelled));
        }
    }
}

/// Process-local in-memory LRU chunk cache with single-flight coalescing and generation isolation.
pub struct LruChunkCache {
    state: Mutex<CacheState>,
}

impl LruChunkCache {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            state: Mutex::new(CacheState::new(max_bytes)),
        }
    }

    pub fn clear_all(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.entries.clear();
        state.lru_order.clear();
        state.in_flight.clear();
        state.total_bytes = 0;
    }

    #[cfg(test)]
    fn stats(&self) -> (usize, usize, u64) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (state.entries.len(), state.total_bytes, state.generation)
    }

    pub async fn read_range(
        &self,
        client: &CloudClient,
        book_id: &str,
        hash: Hash,
        total_size: u64,
        offset: u64,
        len: usize,
    ) -> Result<Bytes, ErrorCode> {
        self.read_range_with(hash, total_size, offset, len, |_, byte_range| {
            client.get_book_file_range(book_id, &hash, byte_range)
        })
        .await
    }

    pub async fn read_range_with<F, Fut>(
        &self,
        hash: Hash,
        total_size: u64,
        offset: u64,
        len: usize,
        fetcher: F,
    ) -> Result<Bytes, ErrorCode>
    where
        F: Fn(u64, ByteRange) -> Fut + Send + Sync,
        Fut: Future<Output = Result<Bytes, ErrorCode>> + Send,
    {
        if len == 0 {
            return if offset <= total_size {
                Ok(Bytes::new())
            } else {
                Err(ErrorCode::ReaderCorruptedEpub)
            };
        }

        let end_exclusive = offset
            .checked_add(len as u64)
            .filter(|&end| end <= total_size)
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;

        let chunk_size_u64 = EPUB_CHUNK_SIZE as u64;
        let first_chunk = offset / chunk_size_u64;
        let last_chunk = (end_exclusive - 1) / chunk_size_u64;

        if first_chunk == last_chunk {
            let range = chunk_byte_range(first_chunk, chunk_size_u64, total_size)?;
            let chunk = self
                .get_or_fetch_chunk(hash, first_chunk, range, &fetcher)
                .await?;
            let local_start = usize::try_from(offset - range.start())
                .map_err(|_| ErrorCode::ReaderCorruptedEpub)?;
            let local_end = local_start
                .checked_add(len)
                .filter(|&end| end <= chunk.len())
                .ok_or(ErrorCode::ReaderCorruptedEpub)?;
            return Ok(chunk.slice(local_start..local_end));
        }

        let fetcher_ref = &fetcher;
        let chunks: Vec<(ByteRange, Bytes)> = stream::iter(first_chunk..=last_chunk)
            .map(|chunk_idx| async move {
                let range = chunk_byte_range(chunk_idx, chunk_size_u64, total_size)?;
                let chunk = self
                    .get_or_fetch_chunk(hash, chunk_idx, range, fetcher_ref)
                    .await?;
                Ok::<(ByteRange, Bytes), ErrorCode>((range, chunk))
            })
            .buffered(MAX_CHUNK_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        let mut assembled = BytesMut::with_capacity(len);
        for (range, chunk) in chunks {
            let chunk_start = range.start();
            let chunk_end_exclusive = chunk_start + range.len() as u64;
            let slice_start = offset.max(chunk_start) - chunk_start;
            let slice_end = end_exclusive.min(chunk_end_exclusive) - chunk_start;
            let local_start =
                usize::try_from(slice_start).map_err(|_| ErrorCode::ReaderCorruptedEpub)?;
            let local_end =
                usize::try_from(slice_end).map_err(|_| ErrorCode::ReaderCorruptedEpub)?;
            if local_end > chunk.len() || local_start > local_end {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }
            assembled.extend_from_slice(&chunk[local_start..local_end]);
        }

        if assembled.len() != len {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }
        Ok(assembled.freeze())
    }

    async fn get_or_fetch_chunk<F, Fut>(
        &self,
        hash: Hash,
        chunk_index: u64,
        range: ByteRange,
        fetcher: &F,
    ) -> Result<Bytes, ErrorCode>
    where
        F: Fn(u64, ByteRange) -> Fut + Send + Sync,
        Fut: Future<Output = Result<Bytes, ErrorCode>> + Send,
    {
        let key = ChunkKey::new(hash, chunk_index);
        loop {
            enum Role<'a> {
                Hit(Bytes),
                Follower(watch::Receiver<Option<InFlightOutcome>>),
                Leader(InFlightGuard<'a>),
            }

            let role = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(hit) = state.touch(key) {
                    Role::Hit(hit)
                } else if let Some(existing) = state.in_flight.get(&key) {
                    Role::Follower(existing.rx.clone())
                } else {
                    let leader_gen = state.generation;
                    let (tx, rx) = watch::channel(None);
                    state.in_flight.insert(
                        key,
                        InFlightEntry {
                            generation: leader_gen,
                            rx,
                        },
                    );
                    Role::Leader(InFlightGuard {
                        cache: self,
                        key,
                        leader_gen,
                        tx: Some(tx),
                    })
                }
            };

            match role {
                Role::Hit(bytes) => return Ok(bytes),
                Role::Follower(mut rx) => match rx.wait_for(Option::is_some).await {
                    Ok(guard) => match guard.as_ref() {
                        Some(InFlightOutcome::Ready(result)) => return result.clone(),
                        Some(InFlightOutcome::LeaderCancelled) | None => continue,
                    },
                    Err(_) => continue,
                },
                Role::Leader(guard) => {
                    let fetched = fetcher(chunk_index, range).await.and_then(|bytes| {
                        if bytes.len() == range.len() {
                            Ok(bytes)
                        } else {
                            Err(ErrorCode::CloudInvalidResponse)
                        }
                    });
                    return guard.complete(fetched);
                }
            }
        }
    }
}

fn chunk_byte_range(
    chunk_index: u64,
    chunk_size_u64: u64,
    total_size: u64,
) -> Result<ByteRange, ErrorCode> {
    let chunk_start = chunk_index
        .checked_mul(chunk_size_u64)
        .ok_or(ErrorCode::ReaderCorruptedEpub)?;
    let chunk_end = chunk_start
        .saturating_add(chunk_size_u64 - 1)
        .min(total_size.saturating_sub(1));
    ByteRange::try_within(chunk_start, chunk_end, total_size).ok_or(ErrorCode::ReaderCorruptedEpub)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::{Notify, oneshot};

    use super::*;

    fn synthetic_bytes(len: usize) -> Bytes {
        let data: Vec<u8> = (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect();
        Bytes::from(data)
    }

    #[tokio::test]
    async fn lru_cache_evicts_least_recently_used_and_honors_zero_length_bounds() {
        let cache = LruChunkCache::new(2 * EPUB_CHUNK_SIZE);
        let data = synthetic_bytes(3 * EPUB_CHUNK_SIZE);
        let hash = blake3::hash(&data);
        let total = data.len() as u64;
        let fetches = Arc::new(AtomicUsize::new(0));

        let fetcher = |_, range: ByteRange| {
            let data = data.clone();
            let fetches = Arc::clone(&fetches);
            async move {
                fetches.fetch_add(1, Ordering::SeqCst);
                Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
            }
        };

        // Zero-length boundary checks: offset <= total succeeds, offset > total fails.
        assert!(cache.read_range_with(hash, total, total, 0, fetcher.clone()).await.unwrap().is_empty());
        assert_eq!(
            cache.read_range_with(hash, total, total + 1, 0, fetcher.clone()).await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );
        assert_eq!(fetches.load(Ordering::SeqCst), 0);

        // Fetch chunk 0 and chunk 1 (cache full: 2 chunks).
        let _ = cache.read_range_with(hash, total, 0, 16, fetcher.clone()).await.unwrap();
        let _ = cache
            .read_range_with(hash, total, EPUB_CHUNK_SIZE as u64, 16, fetcher.clone())
            .await
            .unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
        assert_eq!(cache.stats().1, 2 * EPUB_CHUNK_SIZE);

        // Touch chunk 0 so chunk 1 becomes least recently used.
        let _ = cache.read_range_with(hash, total, 32, 16, fetcher.clone()).await.unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 2);

        // Fetch chunk 2 -> must evict chunk 1, keeping chunk 0 and chunk 2.
        let _ = cache
            .read_range_with(hash, total, (2 * EPUB_CHUNK_SIZE) as u64, 16, fetcher.clone())
            .await
            .unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 3);
        assert_eq!(cache.stats().0, 2);

        // Chunk 0 is still cached; chunk 1 was evicted and requires re-fetch.
        let _ = cache.read_range_with(hash, total, 64, 16, fetcher.clone()).await.unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 3);
        let _ = cache
            .read_range_with(hash, total, EPUB_CHUNK_SIZE as u64, 16, fetcher)
            .await
            .unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn clamps_non_multiple_tail_chunk_and_reassembles_cross_chunk_reads() {
        let cache = LruChunkCache::new(4 * EPUB_CHUNK_SIZE);
        let tail_len = 13_579usize;
        let data = synthetic_bytes(2 * EPUB_CHUNK_SIZE + tail_len);
        let hash = blake3::hash(&data);
        let total = data.len() as u64;
        let observed_ranges = Arc::new(Mutex::new(Vec::new()));

        let fetcher = |_, range: ByteRange| {
            let data = data.clone();
            let observed = Arc::clone(&observed_ranges);
            async move {
                observed.lock().unwrap().push((range.start(), range.end_inclusive(), range.len()));
                Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
            }
        };

        // Read across chunk 0 -> chunk 1 -> chunk 2 (partial tail).
        let offset = (EPUB_CHUNK_SIZE - 100) as u64;
        let read_len = 100 + EPUB_CHUNK_SIZE + 500;
        let slice = cache
            .read_range_with(hash, total, offset, read_len, fetcher)
            .await
            .unwrap();
        assert_eq!(slice.as_ref(), &data[offset as usize..offset as usize + read_len]);

        let mut ranges = observed_ranges.lock().unwrap().clone();
        ranges.sort_by_key(|r| r.0);
        assert_eq!(
            ranges,
            vec![
                (0, (EPUB_CHUNK_SIZE - 1) as u64, EPUB_CHUNK_SIZE),
                (EPUB_CHUNK_SIZE as u64, (2 * EPUB_CHUNK_SIZE - 1) as u64, EPUB_CHUNK_SIZE),
                ((2 * EPUB_CHUNK_SIZE) as u64, total - 1, tail_len),
            ]
        );
    }

    #[tokio::test]
    async fn single_flight_coalesces_concurrent_misses_and_promotes_follower_when_leader_drops() {
        let cache = Arc::new(LruChunkCache::new(4 * EPUB_CHUNK_SIZE));
        let data = synthetic_bytes(EPUB_CHUNK_SIZE + 1024);
        let hash = blake3::hash(&data);
        let total = data.len() as u64;

        // 1. 10 concurrent misses on chunk 0 -> exactly 1 upstream fetch.
        let fetches = Arc::new(AtomicUsize::new(0));
        let leader_started = Arc::new(Notify::new());
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let release_rx = Arc::new(tokio::sync::Mutex::new(Some(release_rx)));

        let mut tasks = Vec::new();
        for _ in 0..10 {
            let cache = Arc::clone(&cache);
            let data = data.clone();
            let fetches = Arc::clone(&fetches);
            let leader_started = Arc::clone(&leader_started);
            let release_rx = Arc::clone(&release_rx);
            tasks.push(tokio::spawn(async move {
                cache
                    .read_range_with(hash, total, 10, 128, move |_, range| {
                        let data = data.clone();
                        let fetches = Arc::clone(&fetches);
                        let leader_started = Arc::clone(&leader_started);
                        let release_rx = Arc::clone(&release_rx);
                        async move {
                            fetches.fetch_add(1, Ordering::SeqCst);
                            leader_started.notify_one();
                            if let Some(rx) = release_rx.lock().await.take() {
                                let _ = rx.await;
                            }
                            Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
                        }
                    })
                    .await
            }));
        }
        leader_started.notified().await;
        release_tx.send(()).unwrap();
        for t in tasks {
            let got = t.await.unwrap().unwrap();
            assert_eq!(got.as_ref(), &data[10..138]);
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);

        // 2. Leader cancelled mid-flight -> surviving follower promotes and succeeds.
        let chunk1_offset = EPUB_CHUNK_SIZE as u64;
        let attempt = Arc::new(AtomicUsize::new(0));
        let first_entered = Arc::new(Notify::new());
        let follower_ready = Arc::new(Notify::new());

        let cache_leader = Arc::clone(&cache);
        let data_leader = data.clone();
        let attempt_leader = Arc::clone(&attempt);
        let first_entered_leader = Arc::clone(&first_entered);
        let leader_handle = tokio::spawn(async move {
            cache_leader
                .read_range_with(hash, total, chunk1_offset, 64, move |_, range| {
                    let data = data_leader.clone();
                    let attempt = Arc::clone(&attempt_leader);
                    let first_entered = Arc::clone(&first_entered_leader);
                    async move {
                        let n = attempt.fetch_add(1, Ordering::SeqCst);
                        if n == 0 {
                            first_entered.notify_one();
                            futures_util::future::pending::<()>().await;
                        }
                        Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
                    }
                })
                .await
        });

        first_entered.notified().await;

        let cache_follower = Arc::clone(&cache);
        let data_follower = data.clone();
        let attempt_follower = Arc::clone(&attempt);
        let follower_ready_clone = Arc::clone(&follower_ready);
        let follower_handle = tokio::spawn(async move {
            follower_ready_clone.notify_one();
            cache_follower
                .read_range_with(hash, total, chunk1_offset, 64, move |_, range| {
                    let data = data_follower.clone();
                    let attempt = Arc::clone(&attempt_follower);
                    async move {
                        attempt.fetch_add(1, Ordering::SeqCst);
                        Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
                    }
                })
                .await
        });

        follower_ready.notified().await;
        tokio::task::yield_now().await;

        leader_handle.abort();
        let _ = leader_handle.await;

        let follower_res = follower_handle.await.unwrap().unwrap();
        assert_eq!(
            follower_res.as_ref(),
            &data[chunk1_offset as usize..chunk1_offset as usize + 64]
        );
        assert_eq!(attempt.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn clear_all_advances_generation_so_stale_in_flight_fetch_cannot_pollute_cache() {
        let cache = Arc::new(LruChunkCache::new(4 * EPUB_CHUNK_SIZE));
        let data = synthetic_bytes(1024);
        let hash = blake3::hash(&data);
        let total = data.len() as u64;

        let started = Arc::new(Notify::new());
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let release_rx = Arc::new(tokio::sync::Mutex::new(Some(release_rx)));

        let cache_bg = Arc::clone(&cache);
        let data_bg = data.clone();
        let started_bg = Arc::clone(&started);
        let task = tokio::spawn(async move {
            cache_bg
                .read_range_with(hash, total, 0, 128, move |_, range| {
                    let data = data_bg.clone();
                    let started = Arc::clone(&started_bg);
                    let release_rx = Arc::clone(&release_rx);
                    async move {
                        started.notify_one();
                        if let Some(rx) = release_rx.lock().await.take() {
                            let _ = rx.await;
                        }
                        Ok(data.slice(range.start() as usize..=range.end_inclusive() as usize))
                    }
                })
                .await
        });

        started.notified().await;
        cache.clear_all();
        assert_eq!(cache.stats(), (0, 0, 1));

        release_tx.send(()).unwrap();
        let res = task.await.unwrap().unwrap();
        assert_eq!(res.as_ref(), &data[..128]);
        // Stale in-flight fetch must NOT be written back into generation 1's cache!
        assert_eq!(cache.stats(), (0, 0, 1));
    }
}
