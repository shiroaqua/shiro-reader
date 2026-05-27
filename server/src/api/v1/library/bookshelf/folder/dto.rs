use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub name: String
}

#[derive(Debug, Deserialize)]
pub struct GetFoldersQuery {
    pub id: Option<String>,
    pub recursive: Option<bool>,
 }


#[derive(Debug, Serialize)]
pub struct CreateFolderResponse {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GetFoldersResponse(pub Vec<FolderNode>);


#[derive(Debug, Serialize)]
pub struct FolderNode {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub children: Vec<FolderNode>
}
