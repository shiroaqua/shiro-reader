pub mod catalog;
#[cfg(feature = "application")]
pub mod mapping;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use catalog::{CatalogEntry, ErrorCode};

/// Supported application UI & error-message locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppLocale {
    Zh,
    #[default]
    En,
}

impl AppLocale {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    /// Detects the best supported locale from a system BCP-47/POSIX language tag (e.g. `zh-CN`, `en_US`).
    pub fn detect_from_tag(tag: &str) -> Self {
        let base = tag
            .trim()
            .split(['-', '_', '.'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match base.as_str() {
            "zh" => Self::Zh,
            _ => Self::En,
        }
    }
}

/// Machine-readable command failure returned by IPC commands.
///
/// Shares `operation_id`, `error_id`, and `code` with [`ErrorEventPayload`] so the UI
/// notification layer deduplicates by `error_id` and never shows duplicate alerts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandErrorPayload {
    pub operation_id: String,
    pub error_id: String,
    pub code: u32,
    pub key: String,
    pub item_id: Option<String>,
}

/// Localized error event pushed from Rust to the UI notification channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEventPayload {
    pub operation_id: String,
    pub error_id: String,
    pub code: u32,
    pub key: String,
    pub locale: AppLocale,
    pub message: String,
    pub item_id: Option<String>,
}

/// Paired IPC failure (command payload + localized event payload) generated for one failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedClientError {
    pub command: CommandErrorPayload,
    pub event: ErrorEventPayload,
}

impl PairedClientError {
    pub fn new(
        code: ErrorCode,
        locale: AppLocale,
        operation_id: impl Into<String>,
        item_id: Option<String>,
    ) -> Self {
        let operation_id = operation_id.into();
        let error_id = Uuid::new_v4().to_string();
        let entry = code.entry();
        let command = CommandErrorPayload {
            operation_id: operation_id.clone(),
            error_id: error_id.clone(),
            code: entry.code,
            key: entry.key.to_owned(),
            item_id: item_id.clone(),
        };
        let event = ErrorEventPayload {
            operation_id,
            error_id,
            code: entry.code,
            key: entry.key.to_owned(),
            locale,
            message: entry.localize(locale).to_owned(),
            item_id,
        };
        Self { command, event }
    }
}

/// Bounded FIFO buffer storing early error events emitted before the frontend registers listeners.
#[derive(Debug, Clone)]
pub struct BoundedErrorBuffer {
    capacity: usize,
    entries: VecDeque<ErrorEventPayload>,
}

impl BoundedErrorBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, event: ErrorEventPayload) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(event);
    }

    pub fn snapshot(&self) -> Vec<ErrorEventPayload> {
        self.entries.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_codes_and_keys_are_unique_and_non_empty() {
        let mut codes = HashSet::new();
        let mut keys = HashSet::new();

        for error_code in ErrorCode::ALL {
            let entry = error_code.entry();
            assert!(
                codes.insert(entry.code),
                "duplicate error numeric code: {}",
                entry.code
            );
            assert!(keys.insert(entry.key), "duplicate error key: {}", entry.key);
            assert!(!entry.zh.trim().is_empty(), "empty zh for {}", entry.key);
            assert!(!entry.en.trim().is_empty(), "empty en for {}", entry.key);
            assert_eq!(ErrorCode::from_code(entry.code), Some(*error_code));
            assert_eq!(ErrorCode::from_key(entry.key), Some(*error_code));
        }
        assert!(ErrorCode::ALL.len() >= 70);

        let reader_and_range_cases = [
            (
                ErrorCode::BookFileRangeNotSatisfiable,
                2116,
                "library.book.file.range_not_satisfiable",
            ),
            (
                ErrorCode::BookFileInvalidRange,
                2117,
                "library.book.file.invalid_range",
            ),
            (
                ErrorCode::ReaderSessionNotFound,
                3101,
                "client.reader.session_not_found",
            ),
            (
                ErrorCode::ReaderEntryNotFound,
                3102,
                "client.reader.entry_not_found",
            ),
            (
                ErrorCode::ReaderUnsupportedFormat,
                3103,
                "client.reader.unsupported_format",
            ),
            (
                ErrorCode::ReaderCorruptedEpub,
                3104,
                "client.reader.corrupted_epub",
            ),
            (
                ErrorCode::ReaderEntryTooLarge,
                3105,
                "client.reader.entry_too_large",
            ),
            (
                ErrorCode::ReaderInvalidFontFile,
                3106,
                "client.reader.invalid_font_file",
            ),
            (
                ErrorCode::ReaderFontTooLarge,
                3107,
                "client.reader.font_too_large",
            ),
            (
                ErrorCode::ReaderFontNotFound,
                3108,
                "client.reader.font_not_found",
            ),
        ];
        for (variant, expected_code, expected_key) in reader_and_range_cases {
            let entry = variant.entry();
            assert_eq!(variant.code(), expected_code);
            assert_eq!(variant.key(), expected_key);
            assert_eq!(ErrorCode::from_code(expected_code), Some(variant));
            assert_eq!(ErrorCode::from_key(expected_key), Some(variant));
            assert!(!entry.zh.trim().is_empty());
            assert!(!entry.en.trim().is_empty());
        }

        let import_too_large = ErrorCode::ImportFileTooLarge.entry();
        assert!(import_too_large.zh.contains("512 MiB"));
        assert!(!import_too_large.zh.contains("256 MiB"));
        assert!(import_too_large.en.contains("512 MiB"));
        assert!(!import_too_large.en.contains("256 MiB"));
    }

    #[test]
    fn locale_detection_and_paired_error_share_ids() {
        assert_eq!(AppLocale::detect_from_tag("zh-CN"), AppLocale::Zh);
        assert_eq!(AppLocale::detect_from_tag("zh_Hans_CN"), AppLocale::Zh);
        assert_eq!(AppLocale::detect_from_tag("en-US"), AppLocale::En);
        assert_eq!(AppLocale::detect_from_tag("ja-JP"), AppLocale::En);

        let paired = PairedClientError::new(
            ErrorCode::BookTitleConflict,
            AppLocale::Zh,
            "op-123",
            Some("item-1".to_owned()),
        );
        assert_eq!(paired.command.operation_id, "op-123");
        assert_eq!(paired.event.operation_id, "op-123");
        assert_eq!(paired.command.error_id, paired.event.error_id);
        assert_eq!(paired.command.code, 2007);
        assert_eq!(paired.event.code, 2007);
        assert_eq!(paired.command.item_id.as_deref(), Some("item-1"));
        assert_eq!(paired.event.message, "已存在同名书籍。");

        let en = PairedClientError::new(ErrorCode::BookNotFound, AppLocale::En, "op-456", None);
        assert_eq!(en.command.code, 2006);
        assert_eq!(en.event.message, "Book not found.");
    }

    #[test]
    fn bounded_error_buffer_evicts_oldest_entries() {
        let mut buf = BoundedErrorBuffer::new(2);
        let e1 = PairedClientError::new(ErrorCode::BookNotFound, AppLocale::En, "op-1", None).event;
        let e2 = PairedClientError::new(ErrorCode::TagNotFound, AppLocale::En, "op-2", None).event;
        let e3 = PairedClientError::new(ErrorCode::LocalStartupFailed, AppLocale::En, "op-3", None)
            .event;

        buf.push(e1);
        buf.push(e2.clone());
        buf.push(e3.clone());

        let snap = buf.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].error_id, e2.error_id);
        assert_eq!(snap[1].error_id, e3.error_id);
    }
}
