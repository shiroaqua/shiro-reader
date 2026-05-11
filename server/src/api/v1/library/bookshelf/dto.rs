use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateBookshelfRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateBookshelfResponse {
    pub id: String,
    pub created_at: DateTime<Utc>
}