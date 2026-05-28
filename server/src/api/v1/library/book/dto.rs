use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize)]
pub struct CreateBookRequest {
    pub title: String,
    pub hash: String,
    pub bookshelf_id: String,
    pub folder_id: Option<String>,
}


#[derive(Debug, Deserialize)]
pub struct RenameBookRequest {
    pub title: String,
}


#[derive(Debug, Deserialize)]
pub struct BooksQuery {
    pub id: String
}

#[derive(Debug, Serialize)]
pub struct CreateBookResponse {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GetBooksResponse(pub Vec<GetBookResponse>);

#[derive(Debug, Serialize)]
pub struct GetBookResponse {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub bookshelf_id: String,
    pub folder_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
