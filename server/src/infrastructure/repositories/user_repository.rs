use toasty::{Db, Executor, Transaction};

use crate::{
    domain::{
        ChangeTarget, EntityCreate,
        identity::user::{
            entity::User,
            value_objects::{UserId, UserName},
        },
    },
    infrastructure::db::sqlite::sqlite_error,
};

#[derive(Debug, thiserror::Error)]
pub enum UserRepositoryError {
    #[error("user not found")]
    NotFound,

    #[error("user name conflict")]
    NameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct UserRepository {
    db: Db,
}

impl UserRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn session(&self) -> UserSession {
        UserSession {
            db: self.db.clone(),
        }
    }

    pub async fn create(&self, user: User) -> Result<User, UserRepositoryError> {
        user.create()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)
    }

    pub async fn delete(&self, user: User) -> Result<(), UserRepositoryError> {
        user.delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)
    }

    pub async fn find_by_id(&self, id: &UserId) -> Result<User, UserRepositoryError> {
        find_user_by_id(&mut self.db.clone(), id).await
    }

    pub async fn find_by_name(&self, name: UserName) -> Result<Option<User>, UserRepositoryError> {
        User::filter(User::fields().name().eq(name))
            .first()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)
    }

    pub async fn list(&self, ids: &[UserId]) -> Result<Vec<User>, UserRepositoryError> {
        let query = match ids {
            [] => User::all(),
            [id] => User::filter(User::fields().id().eq(id.clone())),
            ids => User::filter(
                User::fields()
                    .id()
                    .inner()
                    .in_list(ids.iter().map(|id| *id.as_ref()).collect::<Vec<uuid::Uuid>>()),
            ),
        };
        query
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)
    }
}

pub struct UserSession {
    db: Db,
}

impl UserSession {
    pub async fn begin_transaction(&mut self) -> Result<UserTransaction<'_>, UserRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_user_db_error)?;
        Ok(UserTransaction { tx })
    }
}

pub struct UserTransaction<'a> {
    tx: Transaction<'a>,
}

impl UserTransaction<'_> {
    pub async fn find_by_id(&mut self, id: &UserId) -> Result<User, UserRepositoryError> {
        find_user_by_id(&mut self.tx, id).await
    }

    pub async fn commit(self) -> Result<(), UserRepositoryError> {
        self.tx.commit().await.map_err(map_user_db_error)
    }
}

impl ChangeTarget<User> for UserTransaction<'_> {
    type Error = UserRepositoryError;

    fn executor(&mut self) -> &mut dyn toasty::codegen_support::Executor {
        &mut self.tx
    }

    fn map_error(error: toasty::Error) -> Self::Error {
        map_user_db_error(error)
    }
}

async fn find_user_by_id(
    executor: &mut impl Executor,
    id: &UserId,
) -> Result<User, UserRepositoryError> {
    User::get_by_id(executor, id)
        .await
        .map_err(map_user_db_error)
}

fn map_user_db_error(error: toasty::Error) -> UserRepositoryError {
    if error.is_record_not_found() {
        return UserRepositoryError::NotFound;
    }
    if let Some(rusqlite::Error::SqliteFailure(code, message)) = sqlite_error(&error)
        && code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        && message
            .as_deref()
            .is_some_and(|msg| msg.contains("users.name"))
    {
        return UserRepositoryError::NameConflict;
    }
    UserRepositoryError::Storage(error.into())
}
