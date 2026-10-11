#[derive(Debug)]
pub struct CreateBookOutput {
    pub id: String,
    pub created_at: i128,
}

#[derive(Debug)]
pub struct GetBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub file_type: String,
    pub created_at: i128,
    pub updated_at: i128,
}
