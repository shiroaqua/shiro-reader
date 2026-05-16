use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateBookshelfRequest {
    pub name: String,
}


#[derive(Debug, Deserialize)]
pub struct RenameBookshelfRequest {
    pub name: String
}

#[derive(Debug, Deserialize)]
pub struct BookshelvesQuery {
    pub id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetBookshelfResponse {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>
}

#[derive(Debug, Serialize)]
pub struct CreateBookshelfResponse {
    pub id: String,
    pub created_at: DateTime<Utc>
}