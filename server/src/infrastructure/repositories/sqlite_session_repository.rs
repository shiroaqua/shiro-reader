use async_trait::async_trait;
use sea_query::{Expr, ExprTrait, Query, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;
use sqlx::SqlitePool;

use crate::{
    application::identity::authentication::ports::SessionRepository,
    infrastructure::repositories::{
        idens::{AuthAccessTokens, AuthSessions},
        sqlite::{SqliteExecutor, map_storage_error},
    },
};

#[derive(Clone)]
pub struct SqliteSessionRepository {
    db: SqliteExecutor,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: SqliteExecutor::new(pool),
        }
    }
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn create(&self, user_id: &str, refresh_hash: &str, access_hash: &str) -> anyhow::Result<()> {
        let mut tx = self.db.begin(map_storage_error).await?;

        let (sql, values) = Query::insert()
            .into_table(AuthSessions::Table)
            .columns([AuthSessions::RefreshHash, AuthSessions::UserId])
            .values_panic([Expr::val(refresh_hash), Expr::val(user_id)])
            .build_sqlx(SqliteQueryBuilder);
        tx.execute(&sql, values, map_storage_error).await?;

        let (sql, values) = Query::insert()
            .into_table(AuthAccessTokens::Table)
            .columns([AuthAccessTokens::AccessHash, AuthAccessTokens::RefreshHash])
            .values_panic([Expr::val(access_hash), Expr::val(refresh_hash)])
            .build_sqlx(SqliteQueryBuilder);
        tx.execute(&sql, values, map_storage_error).await?;

        tx.commit(map_storage_error).await?;
        Ok(())
    }

    async fn add_access(&self, user_id: &str, refresh_hash: &str, access_hash: &str) -> anyhow::Result<bool> {
        // A concurrent logout either prevents this insert or cascades to the inserted row.
        let (sql, values) = Query::insert()
            .into_table(AuthAccessTokens::Table)
            .columns([AuthAccessTokens::AccessHash, AuthAccessTokens::RefreshHash])
            .select_from(
                Query::select()
                    .expr(Expr::val(access_hash))
                    .column(AuthSessions::RefreshHash)
                    .from(AuthSessions::Table)
                    .and_where(Expr::col(AuthSessions::RefreshHash).eq(refresh_hash))
                    .and_where(Expr::col(AuthSessions::UserId).eq(user_id))
                    .to_owned(),
            )?
            .build_sqlx(SqliteQueryBuilder);

        let result = self.db.execute(&sql, values, map_storage_error).await?;
        Ok(result.rows_affected() == 1)
    }

    async fn contains_access(&self, user_id: &str, access_hash: &str) -> anyhow::Result<bool> {
        let (sql, values) = Query::select()
            .expr(Expr::val(1))
            .from(AuthAccessTokens::Table)
            .inner_join(
                AuthSessions::Table,
                Expr::col((AuthAccessTokens::Table, AuthAccessTokens::RefreshHash))
                    .equals((AuthSessions::Table, AuthSessions::RefreshHash)),
            )
            .and_where(Expr::col((AuthAccessTokens::Table, AuthAccessTokens::AccessHash)).eq(access_hash))
            .and_where(Expr::col((AuthSessions::Table, AuthSessions::UserId)).eq(user_id))
            .limit(1)
            .build_sqlx(SqliteQueryBuilder);

        self.db.fetch_exists(&sql, values, map_storage_error).await
    }

    async fn revoke(&self, user_id: &str, refresh_hash: &str) -> anyhow::Result<()> {
        let (sql, values) = Query::delete()
            .from_table(AuthSessions::Table)
            .and_where(Expr::col(AuthSessions::RefreshHash).eq(refresh_hash))
            .and_where(Expr::col(AuthSessions::UserId).eq(user_id))
            .build_sqlx(SqliteQueryBuilder);

        self.db.execute(&sql, values, map_storage_error).await?;
        Ok(())
    }
}
