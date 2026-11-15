use jiff::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserOutput {
    pub id: String,
    pub name: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
