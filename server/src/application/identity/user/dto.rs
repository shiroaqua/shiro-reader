use jiff::Timestamp;

use crate::domain::identity::user::entity::User;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserOutput {
    pub id: String,
    pub name: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl From<User> for GetUserOutput {
    fn from(user: User) -> Self {
        let user = user.into_inner();
        Self {
            id: user.id.to_string(),
            name: user.name.into(),
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}
