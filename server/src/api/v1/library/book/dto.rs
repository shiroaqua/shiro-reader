use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize)]
pub struct CreateBookRequest {
    pub title: String,
    pub hash: String,
}


#[derive(Debug, Deserialize)]
pub struct RenameBookRequest {
    pub title: String,
}


#[derive(Debug, Deserialize)]
pub struct BooksQuery {
    #[serde(default)]
    pub id: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateBookResponse {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GetBookResponse {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
