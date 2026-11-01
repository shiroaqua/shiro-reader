#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserOutput {
    pub id: String,
    pub name: String,
    pub created_at: i128,
    pub updated_at: i128,
}
