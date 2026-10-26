use async_trait::async_trait;
use sea_query::{Expr, ExprTrait, Iden, Query, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::identity::user::ports::UserRepository,
    domain::identity::user::{
        entity::User,
        errors::UserDomainError,
        value_objects::{Email, Phone, UserId, UserName, UserPasswordHash},
    },
    infrastructure::repositories::{
        errors::UserRepositoryError,
        idens::{FULL_USERS_TABLE_COLUMNS, INSERT_USERS_COLUMNS, Users},
        sqlite::{
            SqliteExecutor, SqliteRowExt, VersionedRecord, join_timestamp_ns,
            map_database_error, map_invalid_data, message_contains_columns, split_system_time,
        },
    },
};

#[derive(Clone)]
pub struct SqliteUserRepository {
    db: SqliteExecutor,
}

impl SqliteUserRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: SqliteExecutor::new(pool),
        }
    }
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn create(&self, user: &User) -> Result<(), UserRepositoryError> {
        let created_at = split_system_time(user.created_at());
        let updated_at = split_system_time(user.updated_at());

        let (sql, values) = Query::insert()
            .into_table(Users::Table)
            .columns(INSERT_USERS_COLUMNS)
            .values_panic([
                Expr::val(user.id().to_string()),
                Expr::val(user.name().as_str()),
                Expr::val(user.email().as_ref().map(Email::as_str)),
                Expr::val(user.phone().as_ref().map(Phone::as_e164)),
                Expr::val(user.phone().as_ref().and_then(Phone::extension)),
                Expr::val(user.password_hash().as_str()),
                Expr::val(created_at.0),
                Expr::val(created_at.1),
                Expr::val(updated_at.0),
                Expr::val(updated_at.1),
                Expr::val(0),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.db.execute(&sql, values, map_sqlx_error).await?;
        Ok(())
    }

    async fn delete(&self, id: &UserId) -> Result<(), UserRepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Users::Table)
            .and_where(Expr::col(Users::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(&sql, values, UserRepositoryError::UserNotFound, map_sqlx_error)
            .await
    }

    async fn password_hash(&self, id: &UserId) -> Result<String, UserRepositoryError> {
        let (sql, values) = Query::select()
            .column(Users::PasswordHash)
            .from(Users::Table)
            .and_where(Expr::col(Users::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        let password_hash: UserPasswordHash = self.db
            .fetch_optional(&sql, values, UserRepositoryError::UserNotFound, map_sqlx_error)
            .await?;
        Ok(password_hash.as_str().to_owned())
    }

    async fn list_ids(&self) -> Result<Vec<String>, UserRepositoryError> {
        let (sql, values) = Query::select()
            .column(Users::Id)
            .from(Users::Table)
            .build_sqlx(SqliteQueryBuilder);

        let ids: Vec<UserId> = self.db.fetch_all(&sql, values, map_sqlx_error).await?;
        Ok(ids.into_iter().map(|id| id.to_string()).collect())
    }

    async fn find_by_name(&self, name: &UserName) -> Result<User, UserRepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_USERS_TABLE_COLUMNS)
            .from(Users::Table)
            .and_where(Expr::col(Users::Name).eq(name.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        let record: VersionedRecord<User> = self.db
            .fetch_optional(&sql, values, UserRepositoryError::UserNotFound, map_sqlx_error)
            .await?;
        Ok(record.value)
    }

    async fn find_by_id(&self, id: &UserId) -> Result<VersionedRecord<User>, UserRepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_USERS_TABLE_COLUMNS)
            .from(Users::Table)
            .and_where(Expr::col(Users::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .fetch_optional(&sql, values, UserRepositoryError::UserNotFound, map_sqlx_error)
            .await
    }

    async fn commit(&self, user: VersionedRecord<User>) -> Result<(), UserRepositoryError> {
        let updated_at = split_system_time(user.updated_at());
        let next_version = user.version.checked_add(1)
            .ok_or_else(|| map_invalid_data(anyhow::anyhow!("user version overflow")))?;
        let (sql, values) = Query::update()
            .table(Users::Table)
            .values([
                (Users::Name, user.name().as_str().into()),
                (Users::Email, user.email().as_ref().map(Email::as_str).into()),
                (Users::Phone, user.phone().as_ref().map(Phone::as_e164).into()),
                (Users::PhoneExtension, user.phone().as_ref().and_then(Phone::extension).into()),
                (Users::PasswordHash, user.password_hash().as_str().into()),
                (Users::UpdatedAtHigh, updated_at.0.into()),
                (Users::UpdatedAtLow, updated_at.1.into()),
                (Users::Version, next_version.into()),
            ])
            .and_where(Expr::col(Users::Id).eq(user.id().to_string()))
            .and_where(Expr::col(Users::Version).eq(user.version))
            .build_sqlx(SqliteQueryBuilder);

        let mut tx = self.db.begin(map_sqlx_error).await?;
        let result = tx.execute(&sql, values, map_sqlx_error).await?;

        if result.rows_affected() == 0 {
            let (sql, values) = Query::select()
                .column(Users::Id)
                .from(Users::Table)
                .and_where(Expr::col(Users::Id).eq(user.id().to_string()))
                .build_sqlx(SqliteQueryBuilder);

            return Err(if tx.fetch_exists(&sql, values, map_sqlx_error).await? {
                UserRepositoryError::UserVersionConflict
            } else {
                UserRepositoryError::UserNotFound
            });
        }

        tx.commit(map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for VersionedRecord<User> {
    type Error = UserRepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        let email = row.get::<Option<String>>(&Users::Email.to_string())?
            .map(Email::parse)
            .transpose()
            .map_err(map_invalid_data)?;
        let phone = match (
            row.get::<Option<String>>(&Users::Phone.to_string())?,
            row.get::<Option<String>>(&Users::PhoneExtension.to_string())?,
        ) {
            (Some(number), extension) => Some(
                Phone::parse_with_region(number, None, extension.as_deref())
                    .map_err(map_invalid_data)?,
            ),
            (None, None) => None,
            (None, Some(_)) => return Err(map_invalid_data(UserDomainError::InvalidPhoneFormat).into()),
        };

        Ok(VersionedRecord {
            value: User::reconstitute(
                row.get_uuid(&Users::Id.to_string())?,
                UserName::parse(row.get_string::<String>(&Users::Name.to_string())?)
                    .map_err(map_invalid_data)?,
                email,
                phone,
                UserPasswordHash::try_from(row)?,
                join_timestamp_ns(
                    row.get(&Users::CreatedAtHigh.to_string())?,
                    row.get(&Users::CreatedAtLow.to_string())?,
                ),
                join_timestamp_ns(
                    row.get(&Users::UpdatedAtHigh.to_string())?,
                    row.get(&Users::UpdatedAtLow.to_string())?,
                ),
            ),
            version: row.get(&Users::Version.to_string())?,
        })
    }
}

impl TryFrom<&SqliteRow> for UserId {
    type Error = UserRepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(row.get_uuid(&Users::Id.to_string())?)
    }
}

impl TryFrom<&SqliteRow> for UserPasswordHash {
    type Error = UserRepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(UserPasswordHash::parse(row.get_string::<String>(&Users::PasswordHash.to_string())?)
            .map_err(map_invalid_data)?)
    }
}

fn map_sqlx_error(error: sqlx::Error) -> UserRepositoryError {
    map_database_error(error, |database_error| {
        if database_error.is_unique_constraint()
            && message_contains_columns(database_error.message(), Users::Table, [Users::Name])
        {
            Some(UserRepositoryError::UserNameConflict)
        } else {
            None
        }
    })
}
