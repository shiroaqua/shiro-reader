#[derive(Debug)]
pub struct CreateBookshelfOutput {
    pub id: String,
    pub created_at: i128,
}

#[derive(Debug)]
pub struct GetBookshelfOutput {
    pub id: String,
    pub name: String,
    pub created_at: i128,
    pub updated_at: i128,
}
