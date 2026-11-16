pub mod authentication;
pub mod crypto;
pub mod errors;
pub mod user;

use self::{
    errors::{IdentityApplicationError, user_storage},
    user::errors::UserApplicationError,
};
use crate::{
    application::MapDbResult,
    domain::{RequiresVerificationEntity, identity::user::entity::User},
};

impl MapDbResult<User, IdentityApplicationError> for Result<User, toasty::Error> {
    fn map_result(self) -> Result<User, IdentityApplicationError> {
        let user = self.map_err(user_storage)?;
        user.verify()
            .map_err(|error| UserApplicationError::Storage(error.into()).into())
    }
}
