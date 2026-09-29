#[derive(Debug)]
pub struct CreateBookshelfOutput {
    pub id: String,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct GetBookshelfOutput {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}