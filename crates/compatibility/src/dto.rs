use serde::{Deserialize, Serialize};
use shiro_application::library::book::dto::GetBookOutput;
use shiro_domain::library::tag::value_objects::TagDeleteStrategy;
use shiro_error_catalog::{AppLocale, CommandErrorPayload, ErrorEventPayload};

use crate::{config::LaunchMode, credentials::CloudUserInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteTagStrategyDto {
    Cascade,
    Promote,
}

impl From<DeleteTagStrategyDto> for TagDeleteStrategy {
    fn from(value: DeleteTagStrategyDto) -> Self {
        match value {
            DeleteTagStrategyDto::Cascade => Self::Cascade,
            DeleteTagStrategyDto::Promote => Self::Promote,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBooksItemResult {
    pub book_id: String,
    pub restored: bool,
    pub error: Option<CommandErrorPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBooksResult {
    pub operation_id: String,
    pub restored_ids: Vec<String>,
    pub failed: Vec<RestoreBooksItemResult>,
    pub skipped_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum ImportProgressEvent {
    Started {
        #[serde(rename = "operationId")]
        operation_id: String,
        total: usize,
    },
    ItemProgress {
        #[serde(rename = "operationId")]
        operation_id: String,
        index: usize,
        total: usize,
        #[serde(rename = "fileName")]
        file_name: String,
        #[serde(rename = "bytesRead")]
        bytes_read: u64,
        #[serde(rename = "totalBytes")]
        total_bytes: Option<u64>,
    },
    ItemSucceeded {
        #[serde(rename = "operationId")]
        operation_id: String,
        index: usize,
        total: usize,
        book: GetBookOutput,
    },
    ItemFailed {
        #[serde(rename = "operationId")]
        operation_id: String,
        index: usize,
        total: usize,
        #[serde(rename = "fileName")]
        file_name: String,
        error: CommandErrorPayload,
    },
    Finished {
        #[serde(rename = "operationId")]
        operation_id: String,
        cancelled: bool,
        #[serde(rename = "importedCount")]
        imported_count: usize,
        #[serde(rename = "failedCount")]
        failed_count: usize,
        #[serde(rename = "skippedCount")]
        skipped_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBookItemFailure {
    pub file_name: String,
    pub error: CommandErrorPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBooksResult {
    pub operation_id: String,
    pub cancelled: bool,
    pub imported: Vec<GetBookOutput>,
    pub failed: Vec<ImportBookItemFailure>,
    pub skipped: Vec<String>,
    pub aborted_reason: Option<CommandErrorPayload>,
    pub summary_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupPhase {
    SetupRequired,
    Ready,
    StartupFailed,
    CloudProtocolIncompatible,
    CloudUnreachable,
    CloudAuthRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnostic {
    pub error_id: String,
    pub code: u32,
    pub key: String,
    pub message: String,
    pub detail: Option<String>,
    pub path: Option<String>,
}

/// Result of validating and probing a candidate cloud server origin before saving/switching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudOriginCheckResult {
    pub ok: bool,
    pub message: Option<String>,
}

/// Public, token-free cloud session and connection status exposed to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatusDto {
    pub origin: String,
    pub user: Option<CloudUserInfo>,
    pub has_saved_session: bool,
    pub protocol_version: Option<u32>,
    pub diagnostic: Option<StartupDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStateDto {
    pub phase: StartupPhase,
    pub active_mode: Option<LaunchMode>,
    pub configured_mode: Option<LaunchMode>,
    pub pending_restart: bool,
    pub locale: AppLocale,
    pub configured_library_root: String,
    pub default_library_root: String,
    pub cloud_origin: Option<String>,
    pub can_pick_library_dir: bool,
    pub cover_url_base: String,
    pub startup_error: Option<StartupDiagnostic>,
    pub cloud: Option<CloudStatusDto>,
    pub buffered_errors: Vec<ErrorEventPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialSetupRequest {
    pub mode: LaunchMode,
    pub locale: AppLocale,
    pub local_library_root: Option<String>,
    pub cloud_origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub mode: Option<LaunchMode>,
    pub locale: Option<AppLocale>,
    pub local_library_root: Option<String>,
    pub cloud_origin: Option<String>,
}
