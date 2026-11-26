use serde::{Deserialize, Serialize};
use shiro_application::library::tag::dto::{TagDefinitionOutput, TagTreeNodeOutput};
use shiro_domain::library::tag::value_objects::TagDeleteStrategy;
use shiro_error_catalog::{AppLocale, CommandErrorPayload, ErrorEventPayload};

use crate::{
    config::{AppTheme, LaunchMode},
    credentials::CloudUserInfo,
    tag_value::TagValueType,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UploadActiveStage {
    Staging,
    Uploading,
    Finalizing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UploadTaskStatusDto {
    Queued,
    Active {
        stage: UploadActiveStage,
        progress_percent: u8,
        bytes_processed: u64,
        total_bytes: Option<u64>,
        speed_bytes_per_sec: Option<u64>,
        eta_seconds: Option<u64>,
    },
    Succeeded {
        book_id: String,
        title: String,
    },
    Failed {
        error: ErrorEventPayload,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadTaskDto {
    pub id: u64,
    pub file_name: String,
    pub status: UploadTaskStatusDto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadSessionStatsDto {
    pub total: usize,
    pub remaining: usize,
    pub active: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPanelSnapshotDto {
    pub revision: u64,
    pub visible: bool,
    pub concurrency: usize,
    pub completed_counter: u64,
    pub stats: UploadSessionStatsDto,
    pub has_dismissible_errors: bool,
    pub tasks: Vec<UploadTaskDto>,
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
    pub theme: AppTheme,
    pub configured_library_root: String,
    pub default_library_root: String,
    pub cloud_origin: Option<String>,
    pub can_pick_library_dir: bool,
    pub can_import_books: bool,
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

/// Client-facing tag definition carrying a resolved [`TagValueType`] and optional regex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagDefinitionDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub value_type: TagValueType,
    pub value_regex: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<TagDefinitionOutput> for TagDefinitionDto {
    fn from(output: TagDefinitionOutput) -> Self {
        let normalized_regex = output.value_regex.filter(|pattern| !pattern.is_empty());
        let value_type =
            TagValueType::from_value_rule(output.allows_value, normalized_regex.as_deref());
        let value_regex = if output.allows_value {
            normalized_regex
        } else {
            None
        };
        Self {
            id: output.id,
            name: output.name,
            parent_id: output.parent_id,
            color: output.color,
            value_type,
            value_regex,
            created_at: output.created_at,
            updated_at: output.updated_at,
        }
    }
}

/// Client-facing tag tree node wrapping [`TagDefinitionDto`] and recursive children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagTreeNodeDto {
    #[serde(flatten)]
    pub tag: TagDefinitionDto,
    pub children: Vec<TagTreeNodeDto>,
}

impl From<TagTreeNodeOutput> for TagTreeNodeDto {
    fn from(node: TagTreeNodeOutput) -> Self {
        Self {
            tag: node.tag.into(),
            children: node.children.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_panel_snapshot_serializes_with_camel_case_tagged_status() {
        let snap = UploadPanelSnapshotDto {
            revision: 3,
            visible: true,
            concurrency: 4,
            completed_counter: 12,
            stats: UploadSessionStatsDto {
                total: 2,
                remaining: 1,
                active: 1,
                succeeded: 1,
                failed: 0,
            },
            has_dismissible_errors: false,
            tasks: vec![UploadTaskDto {
                id: 7,
                file_name: "demo.epub".to_owned(),
                status: UploadTaskStatusDto::Active {
                    stage: UploadActiveStage::Uploading,
                    progress_percent: 42,
                    bytes_processed: 1024,
                    total_bytes: Some(2048),
                    speed_bytes_per_sec: Some(512),
                    eta_seconds: Some(2),
                },
            }],
        };

        let value = serde_json::to_value(&snap).unwrap();
        assert_eq!(value["completedCounter"], 12);
        assert_eq!(value["hasDismissibleErrors"], false);
        assert_eq!(value["tasks"][0]["fileName"], "demo.epub");
        assert_eq!(value["tasks"][0]["status"]["state"], "active");
        assert_eq!(value["tasks"][0]["status"]["stage"], "uploading");
        assert_eq!(value["tasks"][0]["status"]["progressPercent"], 42);
        assert_eq!(value["tasks"][0]["status"]["bytesProcessed"], 1024);
        assert_eq!(value["tasks"][0]["status"]["totalBytes"], 2048);
        assert_eq!(value["tasks"][0]["status"]["speedBytesPerSec"], 512);
        assert_eq!(value["tasks"][0]["status"]["etaSeconds"], 2);
    }

    #[test]
    fn tag_definition_dto_maps_custom_regex_to_text_and_preserves_regex() {
        let custom = TagDefinitionOutput {
            id: "tag-1".to_owned(),
            name: "Code".to_owned(),
            parent_id: None,
            color: "#336699".to_owned(),
            allows_value: true,
            value_regex: Some("[A-Z]{3}".to_owned()),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        let dto = TagDefinitionDto::from(custom);
        assert_eq!(dto.value_type, TagValueType::Text);
        assert_eq!(dto.value_regex.as_deref(), Some("[A-Z]{3}"));

        let empty_regex = TagDefinitionOutput {
            id: "tag-2".to_owned(),
            name: "Note".to_owned(),
            parent_id: None,
            color: "#336699".to_owned(),
            allows_value: true,
            value_regex: Some(String::new()),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        let empty_dto = TagDefinitionDto::from(empty_regex);
        assert_eq!(empty_dto.value_type, TagValueType::Text);
        assert_eq!(empty_dto.value_regex, None);

        let disabled = TagDefinitionOutput {
            id: "tag-3".to_owned(),
            name: "Flag".to_owned(),
            parent_id: None,
            color: "#336699".to_owned(),
            allows_value: false,
            value_regex: Some("[A-Z]+".to_owned()),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        let disabled_dto = TagDefinitionDto::from(disabled);
        assert_eq!(disabled_dto.value_type, TagValueType::None);
        assert_eq!(disabled_dto.value_regex, None);
    }

    #[test]
    fn tag_tree_node_dto_flattens_fields_and_converts_children() {
        let node = TagTreeNodeOutput {
            tag: TagDefinitionOutput {
                id: "root".to_owned(),
                name: "Root".to_owned(),
                parent_id: None,
                color: "#112233".to_owned(),
                allows_value: true,
                value_regex: TagValueType::Date.default_regex().map(str::to_owned),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                updated_at: "2026-01-01T00:00:00Z".to_owned(),
            },
            children: vec![TagTreeNodeOutput {
                tag: TagDefinitionOutput {
                    id: "child".to_owned(),
                    name: "Child".to_owned(),
                    parent_id: Some("root".to_owned()),
                    color: "#445566".to_owned(),
                    allows_value: true,
                    value_regex: Some("^[0-9]{4}$".to_owned()),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                    updated_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                children: vec![],
            }],
        };

        let dto = TagTreeNodeDto::from(node);
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["id"], "root");
        assert_eq!(value["value_type"], "date");
        assert_eq!(
            value["value_regex"],
            TagValueType::Date.default_regex().unwrap()
        );
        assert!(value.get("allows_value").is_none());
        assert_eq!(value["children"][0]["id"], "child");
        assert_eq!(value["children"][0]["parent_id"], "root");
        assert_eq!(value["children"][0]["value_type"], "text");
        assert_eq!(value["children"][0]["value_regex"], "^[0-9]{4}$");
    }
}
