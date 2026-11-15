use jiff::Timestamp;

#[derive(Debug)]
pub struct CreateBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub file_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug)]
pub struct GetBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub file_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
