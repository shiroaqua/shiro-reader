use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_millis() as i64
}

pub fn ms_to_datetime(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms)
        .expect("invalid timestamp")
}