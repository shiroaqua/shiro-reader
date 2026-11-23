use shiro_application::identity::ports::{
    UserRepository, UserRepositoryError, UserSession, UserTransaction,
};
use shiro_domain::identity::user::{
    entity::{User, UserChange, UserFields},
    value_objects::{UserId, UserName},
};
use toasty::{Db, Executor, Transaction};
use uuid::Uuid;

use crate::db::{models::UserRecord, scalars::DbTimestamp, sqlite::sqlite_error};

#[derive(Clone)]
pub struct SqliteUserRepository {
    db: Db,
}

impl SqliteUserRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl UserRepository for SqliteUserRepository {
    type Session = SqliteUserSession;

    fn session(&self) -> Self::Session {
        SqliteUserSession {
            db: self.db.clone(),
        }
    }

    async fn create(&self, user: User) -> Result<User, UserRepositoryError> {
        let record = UserRecord::from(user)
            .into_create()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)?;
        User::try_from(record).map_err(UserRepositoryError::Storage)
    }

    async fn delete(&self, user: User) -> Result<(), UserRepositoryError> {
        UserRecord::filter(UserRecord::fields().id().eq(user.id().as_uuid()))
            .delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)
    }

    async fn find_by_id(&self, id: &UserId) -> Result<User, UserRepositoryError> {
        find_user_by_id(&mut self.db.clone(), id).await
    }

    async fn find_by_name(&self, name: UserName) -> Result<Option<User>, UserRepositoryError> {
        let record = UserRecord::filter(UserRecord::fields().name().eq(name.as_str().to_owned()))
            .first()
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)?;
        record
            .map(User::try_from)
            .transpose()
            .map_err(UserRepositoryError::Storage)
    }

    async fn list(&self, ids: &[UserId]) -> Result<Vec<User>, UserRepositoryError> {
        let query = match ids {
            [] => UserRecord::all(),
            [id] => UserRecord::filter(UserRecord::fields().id().eq(id.as_uuid())),
            ids => UserRecord::filter(
                UserRecord::fields()
                    .id()
                    .in_list(ids.iter().map(UserId::as_uuid).collect::<Vec<Uuid>>()),
            ),
        };
        query
            .exec(&mut self.db.clone())
            .await
            .map_err(map_user_db_error)?
            .into_iter()
            .map(User::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(UserRepositoryError::Storage)
    }
}

pub struct SqliteUserSession {
    db: Db,
}

impl UserSession for SqliteUserSession {
    type Tx<'a> = SqliteUserTransaction<'a>;

    async fn begin_transaction(
        &mut self,
    ) -> Result<SqliteUserTransaction<'_>, UserRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_user_db_error)?;
        Ok(SqliteUserTransaction { tx })
    }
}

pub struct SqliteUserTransaction<'a> {
    tx: Transaction<'a>,
}

impl UserTransaction for SqliteUserTransaction<'_> {
    async fn find_by_id(&mut self, id: &UserId) -> Result<User, UserRepositoryError> {
        find_user_by_id(&mut self.tx, id).await
    }

    async fn apply_user_change(
        &mut self,
        change: UserChange<'_>,
    ) -> Result<(), UserRepositoryError> {
        if change.is_empty() {
            return Ok(());
        }
        let entity = change.entity();
        let fields = change.fields();
        let mut update =
            UserRecord::filter(UserRecord::fields().id().eq(entity.id().as_uuid())).update();
        if fields.contains(UserFields::NAME) {
            update.set_name(entity.name().as_str().to_owned());
        }
        if fields.contains(UserFields::PASSWORD_HASH) {
            update.set_password_hash(entity.password_hash().as_str().to_owned());
        }
        if fields.contains(UserFields::UPDATED_AT) {
            update.set_updated_at(DbTimestamp(entity.updated_at()));
        }
        update.exec(&mut self.tx).await.map_err(map_user_db_error)
    }

    async fn commit(self) -> Result<(), UserRepositoryError> {
        self.tx.commit().await.map_err(map_user_db_error)
    }
}

async fn find_user_by_id(
    executor: &mut impl Executor,
    id: &UserId,
) -> Result<User, UserRepositoryError> {
    let record = UserRecord::get_by_id(executor, id.as_uuid())
        .await
        .map_err(map_user_db_error)?;
    User::try_from(record).map_err(UserRepositoryError::Storage)
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
