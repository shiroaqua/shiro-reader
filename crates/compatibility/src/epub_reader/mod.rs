pub(crate) mod byte_source;
pub(crate) mod chunk_cache;
pub(crate) mod session;
pub(crate) mod zip_index;

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const EPUB_CHUNK_SIZE: usize = 256 * 1024;
pub const MAX_CHUNK_CONCURRENCY: usize = 4;
pub const MAX_OPEN_SESSIONS: usize = 8;
pub const MAX_CENTRAL_DIR_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ZIP_ENTRIES: usize = 50_000;
pub const MAX_ENTRY_COMPRESSED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_ENTRY_UNCOMPRESSED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DEFLATE_PREALLOC_BYTES: usize = 256 * 1024;
pub const OFFLOAD_COMPRESSED_THRESHOLD: u64 = 64 * 1024;
pub const OFFLOAD_UNCOMPRESSED_THRESHOLD: u64 = 256 * 1024;

/// Non-empty inclusive byte range `[start, end_inclusive]` within `[0, total_size)` whose byte
/// length fits in `usize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    start: u64,
    end_inclusive: u64,
    len: usize,
}

impl ByteRange {
    pub fn try_within(start: u64, end_inclusive: u64, total_size: u64) -> Option<Self> {
        if start <= end_inclusive && end_inclusive < total_size {
            let len = usize::try_from(end_inclusive - start + 1).ok()?;
            Some(Self {
                start,
                end_inclusive,
                len,
            })
        } else {
            None
        }
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end_inclusive(self) -> u64 {
        self.end_inclusive
    }

    pub const fn len(self) -> usize {
        self.len
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Strongly-typed opaque identifier for an opened EPUB reader session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EpubSessionId(Uuid);

impl EpubSessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EpubSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EpubSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenEpubResponseDto {
    pub session_id: EpubSessionId,
    pub book_id: String,
    pub file_size: u64,
    pub entries: BTreeMap<String, u64>,
}
