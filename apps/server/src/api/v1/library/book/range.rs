use std::cmp::Ordering;

/// Validated non-empty inclusive byte range `[start, end_inclusive]` within `[0, total_size)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedByteRange {
    start: u64,
    end_inclusive: u64,
}

impl ResolvedByteRange {
    pub const fn new_within(start: u64, end_inclusive: u64, total_size: u64) -> Option<Self> {
        if start <= end_inclusive && end_inclusive < total_size {
            Some(Self {
                start,
                end_inclusive,
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

    pub const fn len(self) -> u64 {
        self.end_inclusive - self.start + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeParseError {
    InvalidSyntax,
    NotSatisfiable { total_size: u64 },
}

/// Parses a single RFC 9110 §14.1.2 `Range` byte-range specifier.
pub fn parse_single_byte_range(
    header_value: &str,
    total_size: u64,
) -> Result<ResolvedByteRange, RangeParseError> {
    let trimmed = trim_ows(header_value);
    let spec = trimmed
        .strip_prefix("bytes=")
        .ok_or(RangeParseError::InvalidSyntax)?;
    if spec.contains(',') {
        return Err(RangeParseError::InvalidSyntax);
    }

    let (start_str, end_str) = spec.split_once('-').ok_or(RangeParseError::InvalidSyntax)?;
    if start_str.is_empty() && end_str.is_empty() {
        return Err(RangeParseError::InvalidSyntax);
    }
    if !start_str.is_empty() && !start_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RangeParseError::InvalidSyntax);
    }
    if !end_str.is_empty() && !end_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RangeParseError::InvalidSyntax);
    }

    if start_str.is_empty() {
        // Suffix byte range: `bytes=-<suffix-length>`
        if total_size == 0 || end_str.bytes().all(|b| b == b'0') {
            return Err(RangeParseError::NotSatisfiable { total_size });
        }
        let start = match end_str.parse::<u64>() {
            Ok(suffix) => total_size.saturating_sub(suffix),
            Err(_) => 0,
        };
        return ResolvedByteRange::new_within(start, total_size - 1, total_size)
            .ok_or(RangeParseError::NotSatisfiable { total_size });
    }

    if end_str.is_empty() {
        // Open-ended byte range: `bytes=<first-pos>-`
        let Ok(start) = start_str.parse::<u64>() else {
            return Err(RangeParseError::NotSatisfiable { total_size });
        };
        if total_size == 0 || start >= total_size {
            return Err(RangeParseError::NotSatisfiable { total_size });
        }
        return ResolvedByteRange::new_within(start, total_size - 1, total_size)
            .ok_or(RangeParseError::NotSatisfiable { total_size });
    }

    // Closed byte range: `bytes=<first-pos>-<last-pos>`.
    // Compare decimal digit strings before `u64::from_str` so `start > end` above `u64::MAX`
    // is still reported as `InvalidSyntax` (400) rather than `NotSatisfiable` (416).
    if cmp_decimal_digits(start_str, end_str) == Ordering::Greater {
        return Err(RangeParseError::InvalidSyntax);
    }

    let Ok(start) = start_str.parse::<u64>() else {
        return Err(RangeParseError::NotSatisfiable { total_size });
    };
    if total_size == 0 || start >= total_size {
        return Err(RangeParseError::NotSatisfiable { total_size });
    }

    let clamped_end = match end_str.parse::<u64>() {
        Ok(end) => end.min(total_size - 1),
        Err(_) => total_size - 1,
    };

    ResolvedByteRange::new_within(start, clamped_end, total_size)
        .ok_or(RangeParseError::NotSatisfiable { total_size })
}

/// Evaluates `If-Range` using RFC 9110 §13.1.5 strong comparison against the current strong ETag.
pub fn if_range_matches_strong_etag(if_range_value: &str, strong_etag: &str) -> bool {
    let trimmed = trim_ows(if_range_value);
    if trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        return false;
    }
    trimmed == strong_etag
}

fn trim_ows(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

fn cmp_decimal_digits(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_single_byte_ranges_and_clamps_overflow_end() {
        assert_eq!(
            parse_single_byte_range("bytes=0-99", 1000),
            Ok(ResolvedByteRange::new_within(0, 99, 1000).unwrap())
        );
        assert_eq!(
            parse_single_byte_range(" bytes=100-\t", 1000),
            Ok(ResolvedByteRange::new_within(100, 999, 1000).unwrap())
        );
        assert_eq!(
            parse_single_byte_range("bytes=-64", 1000),
            Ok(ResolvedByteRange::new_within(936, 999, 1000).unwrap())
        );
        assert_eq!(
            parse_single_byte_range("bytes=-9999", 1000),
            Ok(ResolvedByteRange::new_within(0, 999, 1000).unwrap())
        );
        assert_eq!(
            parse_single_byte_range("bytes=0-999999999999999999999999", 1000),
            Ok(ResolvedByteRange::new_within(0, 999, 1000).unwrap())
        );
        assert_eq!(
            parse_single_byte_range("bytes=-999999999999999999999999", 1000),
            Ok(ResolvedByteRange::new_within(0, 999, 1000).unwrap())
        );
    }

    #[test]
    fn rejects_invalid_syntax_with_400_classification() {
        for raw in [
            "items=0-10",
            "0-10",
            "bytes=",
            "bytes=-",
            "bytes=0-1, 2-3",
            "bytes=10",
            "bytes=50-10",
            "bytes=999999999999999999999999-999999999999999999999998",
            "bytes=0-1-2",
            "bytes=+0-1",
            "bytes=0-abc",
            "bytes= 0-1",
        ] {
            assert_eq!(
                parse_single_byte_range(raw, 1000),
                Err(RangeParseError::InvalidSyntax),
                "expected InvalidSyntax for {raw:?}"
            );
        }
    }

    #[test]
    fn rejects_unsatisfiable_ranges_with_416_classification() {
        for (raw, total) in [
            ("bytes=1000-1001", 1000),
            ("bytes=1000-", 1000),
            ("bytes=-0", 1000),
            ("bytes=999999999999999999999999-999999999999999999999999", 1000),
            ("bytes=999999999999999999999999-", 1000),
            ("bytes=0-0", 0),
            ("bytes=0-", 0),
            ("bytes=-1", 0),
        ] {
            assert_eq!(
                parse_single_byte_range(raw, total),
                Err(RangeParseError::NotSatisfiable { total_size: total }),
                "expected NotSatisfiable for {raw:?} (total={total})"
            );
        }
    }

    #[test]
    fn if_range_requires_exact_strong_etag_match() {
        let etag = "\"0123456789abcdef\"";
        assert!(if_range_matches_strong_etag("\"0123456789abcdef\"", etag));
        assert!(if_range_matches_strong_etag(" \"0123456789abcdef\"\t", etag));
        assert!(!if_range_matches_strong_etag("W/\"0123456789abcdef\"", etag));
        assert!(!if_range_matches_strong_etag("\"fedcba9876543210\"", etag));
        assert!(!if_range_matches_strong_etag("Wed, 21 Oct 2015 07:28:00 GMT", etag));
    }
}
