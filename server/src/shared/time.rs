use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc};


pub fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_secs()
}
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_millis() as i64
}

pub fn now_ns() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos() as i128
}

pub fn ms_to_datetime(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms)
        .expect("invalid timestamp")
}

pub fn ns_to_datetime(ns: i128) -> DateTime<Utc> {
    let seconds = ns.div_euclid(1_000_000_000);
    let nanos = ns.rem_euclid(1_000_000_000) as u32;
    DateTime::from_timestamp(seconds as i64, nanos).expect("invalid timestamp")
}

#[cfg(test)]
mod tests {
    use super::ns_to_datetime;

    #[test]
    fn nanosecond_timestamp_preserves_submillisecond_precision() {
        let timestamp = 1_700_000_000_123_456_789;
        assert_eq!(ns_to_datetime(timestamp).timestamp_subsec_nanos(), 123_456_789);
    }
}
