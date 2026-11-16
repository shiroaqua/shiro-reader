use crate::domain::RequiresVerificationEntity;
use crate::domain::identity::user::{
    errors::UserDomainError,
    value_objects::{UserId, UserName, UserPasswordHash},
};
use entity_derive::Entity;
use jiff::Timestamp;

#[derive(toasty::Model, Entity)]
#[table = "users"]
pub struct User {
    #[key]
    id: UserId,
    #[unique]
    name: UserName,
    password_hash: UserPasswordHash,
    created_at: Timestamp,
    updated_at: Timestamp,
}
impl User {
    pub fn new(id: UserId, name: UserName, password_hash: UserPasswordHash) -> Self {
        let created_at = Timestamp::now();
        Self {
            id,
            name,
            password_hash,
            created_at,
            updated_at: created_at,
        }
    }

    pub fn name(&self) -> &UserName {
        &self.name
    }

    pub fn password_hash(&self) -> &UserPasswordHash {
        &self.password_hash
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn rename(&mut self, new_name: UserName) -> Result<UserChange<'_>, UserDomainError> {
        if self.name.eq(&new_name) {
            return Err(UserDomainError::NameUnchanged);
        }
        self.name = new_name;
        self.updated_at = Timestamp::now();
        Ok(UserChange {
            entity: self,
            fields: UserFields::NAME | UserFields::UPDATED_AT,
        })
    }

    pub fn change_password_hash(
        &mut self,
        new_passsword_hash: UserPasswordHash,
    ) -> Result<UserChange<'_>, UserDomainError> {
        if self.password_hash.eq(&new_passsword_hash) {
            return Err(UserDomainError::PasswordHashUnchanged);
        }

        self.password_hash = new_passsword_hash;
        self.updated_at = Timestamp::now();
        Ok(UserChange {
            entity: self,
            fields: UserFields::PASSWORD_HASH | UserFields::UPDATED_AT,
        })
    }
}

impl crate::domain::EntityId for User {
    type Id = UserId;

    fn id(&self) -> &UserId {
        &self.id
    }
}

impl RequiresVerificationEntity<UserDomainError> for User {
    fn verify(self) -> Result<Self, UserDomainError> {
        Ok(self)
    }
}
