use serde::{Deserialize, Serialize};
use shiro_domain::identity::user::entity::User;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetUserOutput {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<User> for GetUserOutput {
    fn from(user: User) -> Self {
        let user = user.into_inner();
        Self {
            id: user.id.to_string(),
            name: user.name.into_inner(),
            created_at: user.created_at.to_string(),
            updated_at: user.updated_at.to_string(),
        }
    }
}
