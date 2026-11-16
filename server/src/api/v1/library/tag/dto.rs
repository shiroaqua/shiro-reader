use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefineTagRequest {
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub allows_value: bool,
    pub value_regex: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameTagRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeTagColorRequest {
    pub color: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveTagRequest {
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindBookTagRequest {
    pub tag_id: String,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnbindBookTagRequest {
    pub tag_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteTagQuery {
    pub strategy: DeleteTagStrategy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteTagStrategy {
    Cascade,
    Promote,
}

#[derive(Debug, Serialize)]
pub struct BookTagResponse {
    pub tag_id: String,
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TagResponse {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub allows_value: bool,
    pub value_regex: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Serialize)]
pub struct TagTreeNodeResponse {
    #[serde(flatten)]
    pub tag: TagResponse,
    pub children: Vec<TagTreeNodeResponse>,
}
