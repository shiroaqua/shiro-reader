use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBookRequest {
    pub title: String,
    pub hash: String,
}


#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameBookRequest {
    pub title: String,
}


#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BooksQuery {
    #[serde(default)]
    pub id: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BookResponse {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
