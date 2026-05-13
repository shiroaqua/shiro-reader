use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub bookshelf_id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateFolderResponse {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

