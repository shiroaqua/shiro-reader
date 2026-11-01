use crate::{domain::identity::user::{errors::UserDomainError, value_objects::{UserId, UserName, UserPasswordHash}}, shared::time::now_ns};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct User {
    id: UserId,
    name: UserName,
    password_hash: UserPasswordHash,
    created_at: i128,
    updated_at: i128,
}

impl User {
    pub fn new(
        id: UserId,
        name: UserName,
        password_hash: UserPasswordHash,
        created_at: i128,
    ) -> Self {
        Self {
            id,
            name,
            password_hash,
            created_at,
            updated_at: created_at,
        }
    }

    pub(crate) fn reconstitute(
        id: UserId,
        name: UserName,
        password_hash: UserPasswordHash,
        created_at: i128,
        updated_at: i128,
    ) -> Self {
        Self {
            id,
            name,
            password_hash,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> &UserId {
        &self.id
    }

    pub fn name(&self) -> &UserName {
        &self.name
    }

    pub fn password_hash(&self) -> &UserPasswordHash {
        &self.password_hash
    }

    pub fn created_at(&self) -> SystemTime {
        UNIX_EPOCH
            + Duration::new(
                self.created_at.div_euclid(1_000_000_000) as u64,
                self.created_at.rem_euclid(1_000_000_000) as u32,
            )
    }
    pub fn updated_at(&self) -> SystemTime {
        UNIX_EPOCH
            + Duration::new(
                self.updated_at.div_euclid(1_000_000_000) as u64,
                self.updated_at.rem_euclid(1_000_000_000) as u32,
            )
    }

    pub fn rename(&mut self, new_name: UserName) -> Result<(), UserDomainError>{
        if self.name.eq(&new_name) {
            return Err(UserDomainError::NameUnchanged);
        }
        self.name = new_name;
        self.updated_at = now_ns();
        Ok(())
    }

    pub fn change_password_hash(&mut self, new_passsword_hash: UserPasswordHash) -> Result<(), UserDomainError>{
        if self.password_hash.eq(&new_passsword_hash) {
            return Err(UserDomainError::PasswordHashUnchanged);
        }
        self.password_hash = new_passsword_hash;
        self.updated_at = now_ns();
        Ok(())
    }
}
