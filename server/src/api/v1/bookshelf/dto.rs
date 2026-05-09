use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateDirectoryRequest {
    pub parent_id: Option<String>, // 默认为根目录
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveDirectoryRequest {
    pub parent_id: Option<String>, // 默认为根目录
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteDirectoryQuery {
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Deserialize)]
pub struct DirectoryContentsQuery {
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateDirectoryResponse {
    pub id: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct MoveDirectoryResponse {
    pub updated_at: String,
}


#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BookshelfNodeType {
    Book,
    Directory,
}

#[derive(Debug, Serialize)]
pub struct BookshelfNode {
    pub uuid: Uuid,

    #[serde(rename = "type")]
    pub node_type: BookshelfNodeType,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<BookshelfNode>,
}