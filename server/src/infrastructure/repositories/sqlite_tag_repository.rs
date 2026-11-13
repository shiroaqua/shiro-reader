use std::collections::HashMap;

use async_trait::async_trait;
use sea_query::{Expr, ExprTrait, Iden, OnConflict, Order, Query, SelectStatement, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::tag::{dto::BookTagOutput, ports::{TagDeleteTransaction, TagMoveTransaction, TagRepository}},
    domain::library::{
        book::value_objects::BookId,
        tag::{
            entity::{BookTag, TagDefinition},
            tree::{TagDeletion, TagParentChange},
            value_objects::{TagId, TagName, TagValueRule},
        },
    },
    infrastructure::repositories::{
        errors::TagRepositoryError,
        idens::{BookTags, Books, FULL_TAG_DEFINITION_COLUMNS, TagDefinitions},
        sqlite::{
            join_timestamp_ns, split_timestamp_ns, SqliteExecutor, SqliteRowExt,
            SqliteTransaction, VersionedRecord, map_invalid_data,
        },
    },
};

#[derive(Clone)]
pub struct SqliteTagRepository {
    db: SqliteExecutor,
}

impl SqliteTagRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { db: SqliteExecutor::new(pool) }
    }
}

#[async_trait]
impl TagRepository for SqliteTagRepository {
    async fn create(&self, definition: &TagDefinition) -> Result<(), TagRepositoryError> {
        let created_at = split_timestamp_ns(definition.created_at());
        let updated_at = split_timestamp_ns(definition.updated_at());
        let mut tx = self.db.begin(map_sqlx_error).await?;
        if let Some(parent_id) = definition.parent_id() {
            if !tag_exists(&mut tx, parent_id).await? {
                return Err(TagRepositoryError::ParentNotFound);
            }
        }
        let (sql, values) = Query::insert()
            .into_table(TagDefinitions::Table)
            .columns(FULL_TAG_DEFINITION_COLUMNS)
            .values_panic([
                Expr::val(definition.id().to_string()),
                Expr::val(definition.name().to_string()),
                Expr::val(definition.parent_id().map(ToString::to_string)),
                Expr::val(definition.value_rule().allows_value()),
                Expr::val(definition.value_rule().value_regex().map(str::to_owned)),
                Expr::val(created_at.0),
                Expr::val(created_at.1),
                Expr::val(updated_at.0),
                Expr::val(updated_at.1),
                Expr::val(0_i64),
            ])
            .build_sqlx(SqliteQueryBuilder);
        tx.execute(&sql, values, map_sqlx_error).await?;
        tx.commit(map_sqlx_error).await
    }

    async fn find_by_id(&self, id: &TagId) -> Result<VersionedRecord<TagDefinition>, TagRepositoryError> {
        let (sql, values) = definitions_query()
            .and_where(Expr::col(TagDefinitions::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);
        self.db.fetch_optional(&sql, values, TagRepositoryError::NotFound, map_sqlx_error).await
    }

    async fn commit(&self, definition: VersionedRecord<TagDefinition>) -> Result<(), TagRepositoryError> {
        let mut tx = self.db.begin(map_sqlx_error).await?;
        let updated_at = split_timestamp_ns(definition.updated_at());
        let (sql, values) = Query::update()
            .table(TagDefinitions::Table)
            .values([
                (TagDefinitions::Name, definition.name().to_string().into()),
                (TagDefinitions::UpdatedAtHigh, updated_at.0.into()),
                (TagDefinitions::UpdatedAtLow, updated_at.1.into()),
                (TagDefinitions::Version, (definition.version+1).into()),
            ])
            .and_where(Expr::col(TagDefinitions::Id).eq(definition.id().to_string()))
            .and_where(Expr::col(TagDefinitions::Version).eq(definition.version))
            .build_sqlx(SqliteQueryBuilder);
        let result = tx.execute(&sql, values, map_sqlx_error).await?;
        if result.rows_affected() == 0 {
            return Err(if tag_exists(&mut tx, definition.id()).await? {
                TagRepositoryError::VersionConflict
            } else {
                TagRepositoryError::NotFound
            });
        }
        tx.commit(map_sqlx_error).await
    }

    async fn list(&self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        let (sql, values) = definitions_query().build_sqlx(SqliteQueryBuilder);
        let records: Vec<VersionedRecord<TagDefinition>> = self.db.fetch_all(&sql, values, map_sqlx_error).await?;
        Ok(records.into_iter().map(|record| record.value).collect())
    }

    async fn list_book_tags(&self, book_id: &BookId) -> Result<Vec<BookTagOutput>, TagRepositoryError> {
        let mut tx = self.db.begin(map_sqlx_error).await?;
        let (sql, values) = Query::select()
            .column(Books::Id)
            .from(Books::Table)
            .and_where(Expr::col(Books::Id).eq(book_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);
        if !tx.fetch_exists(&sql, values, map_sqlx_error).await? {
            return Err(TagRepositoryError::BookNotFound);
        }
        let (sql, values) = Query::select()
            .columns([BookTags::TagId, BookTags::Value])
            .from(BookTags::Table)
            .and_where(Expr::col(BookTags::BookId).eq(book_id.to_string()))
            .order_by(BookTags::TagId, Order::Asc)
            .build_sqlx(SqliteQueryBuilder);
        let bindings = tx.fetch_all(&sql, values, map_sqlx_error).await?;
        tx.commit(map_sqlx_error).await?;
        Ok(bindings)
    }

    async fn bind_tag(&self, binding: &BookTag) -> Result<(), TagRepositoryError> {
        let mut tx = self.db.begin(map_sqlx_error).await?;
        let (sql, values) = Query::insert()
            .into_table(BookTags::Table)
            .columns([BookTags::BookId, BookTags::TagId, BookTags::Value])
            .values_panic([
                Expr::val(binding.book_id().to_string()),
                Expr::val(binding.tag_id().to_string()),
                Expr::val(binding.value().map(str::to_owned)),
            ])
            .on_conflict(OnConflict::columns([BookTags::BookId, BookTags::TagId])
                .update_column(BookTags::Value).to_owned())
            .build_sqlx(SqliteQueryBuilder);
        if let Err(error) = tx.execute(&sql, values, std::convert::identity).await {
            if error.as_database_error().is_some_and(|error| error.is_foreign_key_violation()) {
                check_binding_target(&mut tx, binding.book_id(), binding.tag_id()).await?;
            }
            return Err(map_sqlx_error(error));
        }
        tx.commit(map_sqlx_error).await
    }

    async fn unbind_tag(&self, book_id: &BookId, tag_id: &TagId) -> Result<(), TagRepositoryError> {
        let mut tx = self.db.begin(map_sqlx_error).await?;
        let (sql, values) = Query::delete()
            .from_table(BookTags::Table)
            .and_where(Expr::col(BookTags::BookId).eq(book_id.to_string()))
            .and_where(Expr::col(BookTags::TagId).eq(tag_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);
        let result = tx.execute(&sql, values, map_sqlx_error).await?;
        if result.rows_affected() == 0 {
            check_binding_target(&mut tx, book_id, tag_id).await?;
        }
        tx.commit(map_sqlx_error).await
    }

    async fn begin_delete(&self) -> Result<Box<dyn TagDeleteTransaction + '_>, TagRepositoryError> {
        Ok(Box::new(SqliteTagDeleteTransaction {
            tx: self.db.begin(map_sqlx_error).await?,
            versions: HashMap::new(),
        }))
    }

    async fn begin_move(&self) -> Result<Box<dyn TagMoveTransaction + '_>, TagRepositoryError> {
        Ok(Box::new(SqliteTagMoveTransaction {
            tx: self.db.begin(map_sqlx_error).await?,
            versions: HashMap::new(),
        }))
    }
}

struct SqliteTagDeleteTransaction<'a> {
    tx: SqliteTransaction<'a>,
    versions: HashMap<TagId, i64>,
}

#[async_trait]
impl TagDeleteTransaction for SqliteTagDeleteTransaction<'_> {
    async fn list(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        let (sql, values) = definitions_query().build_sqlx(SqliteQueryBuilder);
        let records: Vec<VersionedRecord<TagDefinition>> = self.tx.fetch_all(&sql, values, map_sqlx_error).await?;
        self.versions = records.iter().map(|record| (record.id().clone(), record.version)).collect();
        Ok(records.into_iter().map(|record| record.value).collect())
    }

    async fn commit(self: Box<Self>, deletion: TagDeletion) -> Result<(), TagRepositoryError> {
        let Self { mut tx, versions } = *self;
        for change in deletion.promoted_children() {
            let version = *versions.get(change.id()).ok_or(TagRepositoryError::VersionConflict)?;
            let updated_at = split_timestamp_ns(change.updated_at());
            let (sql, values) = Query::update()
                .table(TagDefinitions::Table)
                .values([
                    (TagDefinitions::ParentId, change.parent_id().map(ToString::to_string).into()),
                    (TagDefinitions::UpdatedAtHigh, updated_at.0.into()),
                    (TagDefinitions::UpdatedAtLow, updated_at.1.into()),
                    (TagDefinitions::Version, (version+1).into()),
                ])
                .and_where(Expr::col(TagDefinitions::Id).eq(change.id().to_string()))
                .and_where(Expr::col(TagDefinitions::Version).eq(version))
                .build_sqlx(SqliteQueryBuilder);
            tx.execute_affected(&sql, values, TagRepositoryError::VersionConflict, map_sqlx_error).await?;
        }
        for id in deletion.deleted_ids() {
            let version = *versions.get(id).ok_or(TagRepositoryError::VersionConflict)?;
            let (sql, values) = Query::delete()
                .from_table(TagDefinitions::Table)
                .and_where(Expr::col(TagDefinitions::Id).eq(id.to_string()))
                .and_where(Expr::col(TagDefinitions::Version).eq(version))
                .build_sqlx(SqliteQueryBuilder);
            // 仅这些标签的绑定通过外键级联清理；上提标签的绑定保持原样。
            tx.execute_affected(&sql, values, TagRepositoryError::VersionConflict, map_sqlx_error).await?;
        }
        tx.commit(map_sqlx_error).await
    }
}

struct SqliteTagMoveTransaction<'a> {
    tx: SqliteTransaction<'a>,
    versions: HashMap<TagId, i64>,
}

#[async_trait]
impl TagMoveTransaction for SqliteTagMoveTransaction<'_> {
    async fn list(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        let (sql, values) = definitions_query().build_sqlx(SqliteQueryBuilder);
        let records: Vec<VersionedRecord<TagDefinition>> = self.tx.fetch_all(&sql, values, map_sqlx_error).await?;
        self.versions = records.iter().map(|record| (record.id().clone(), record.version)).collect();
        Ok(records.into_iter().map(|record| record.value).collect())
    }

    async fn commit(self: Box<Self>, change: TagParentChange) -> Result<(), TagRepositoryError> {
        let Self { mut tx, versions } = *self;
        let version = *versions.get(change.id()).ok_or(TagRepositoryError::VersionConflict)?;
        let updated_at = split_timestamp_ns(change.updated_at());
        let (sql, values) = Query::update()
            .table(TagDefinitions::Table)
            .values([
                (TagDefinitions::ParentId, change.parent_id().map(ToString::to_string).into()),
                (TagDefinitions::UpdatedAtHigh, updated_at.0.into()),
                (TagDefinitions::UpdatedAtLow, updated_at.1.into()),
                (TagDefinitions::Version, (version + 1).into()),
            ])
            .and_where(Expr::col(TagDefinitions::Id).eq(change.id().to_string()))
            .and_where(Expr::col(TagDefinitions::Version).eq(version))
            .build_sqlx(SqliteQueryBuilder);
        tx.execute_affected(&sql, values, TagRepositoryError::VersionConflict, map_sqlx_error).await?;
        tx.commit(map_sqlx_error).await
    }
}

fn definitions_query() -> SelectStatement {
    Query::select()
        .columns(FULL_TAG_DEFINITION_COLUMNS)
        .from(TagDefinitions::Table)
        .order_by(TagDefinitions::Name, Order::Asc)
        .order_by(TagDefinitions::Id, Order::Asc)
        .to_owned()
}

async fn tag_exists(tx: &mut SqliteTransaction<'_>, id: &TagId) -> Result<bool, TagRepositoryError> {
    let (sql, values) = Query::select()
        .column(TagDefinitions::Id)
        .from(TagDefinitions::Table)
        .and_where(Expr::col(TagDefinitions::Id).eq(id.to_string()))
        .build_sqlx(SqliteQueryBuilder);
    tx.fetch_exists(&sql, values, map_sqlx_error).await
}

async fn check_binding_target(
    tx: &mut SqliteTransaction<'_>,
    book_id: &BookId,
    tag_id: &TagId,
) -> Result<(), TagRepositoryError> {
    if !tag_exists(tx, tag_id).await? {
        return Err(TagRepositoryError::NotFound);
    }
    let (sql, values) = Query::select()
        .column(Books::Id)
        .from(Books::Table)
        .and_where(Expr::col(Books::Id).eq(book_id.to_string()))
        .build_sqlx(SqliteQueryBuilder);
    if !tx.fetch_exists(&sql, values, map_sqlx_error).await? {
        return Err(TagRepositoryError::BookNotFound);
    }
    Ok(())
}

impl TryFrom<&SqliteRow> for VersionedRecord<TagDefinition> {
    type Error = TagRepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        let id = TagId::parse(row.get_string::<String>(&TagDefinitions::Id.to_string())?)
            .map_err(map_invalid_data)?;
        let name = TagName::parse(row.get_string::<String>(&TagDefinitions::Name.to_string())?)
            .map_err(map_invalid_data)?;
        let parent_id = row.get::<Option<String>>(&TagDefinitions::ParentId.to_string())?
            .map(TagId::parse).transpose().map_err(map_invalid_data)?;
        let value_rule = TagValueRule::new(
            row.get(&TagDefinitions::AllowsValue.to_string())?,
            row.get(&TagDefinitions::ValueRegex.to_string())?,
        ).map_err(map_invalid_data)?;
        let version = row.get::<i64>(&TagDefinitions::Version.to_string())?;
        let created_at = join_timestamp_ns(
            row.get(&TagDefinitions::CreatedAtHigh.to_string())?,
            row.get(&TagDefinitions::CreatedAtLow.to_string())?,
        );
        let updated_at = join_timestamp_ns(
            row.get(&TagDefinitions::UpdatedAtHigh.to_string())?,
            row.get(&TagDefinitions::UpdatedAtLow.to_string())?,
        );
        if version < 0 {
            return Err(anyhow::anyhow!("invalid tag version").into());
        }
        Ok(Self {
            value: TagDefinition::reconstitute(id, name, parent_id, value_rule, created_at, updated_at).map_err(map_invalid_data)?,
            version,
        })
    }
}

impl TryFrom<&SqliteRow> for BookTagOutput {
    type Error = TagRepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Self {
            tag_id: row.get_string(&BookTags::TagId.to_string())?,
            value: row.get(&BookTags::Value.to_string())?,
        })
    }
}

fn map_sqlx_error(error: sqlx::Error) -> TagRepositoryError {
    if let sqlx::Error::Database(database_error) = &error {
        // SQLite 事务读快照后如遇并发写入，升级为写事务会失败。
        // 包含 SQLITE_BUSY_SNAPSHOT (517)，阻止过期删除计划和绑定写入。
        if database_error.code().as_deref().and_then(|code| code.parse::<i32>().ok())
            .is_some_and(|code| matches!(code & 0xff, 5 | 6))
        {
            return TagRepositoryError::VersionConflict;
        }
    }
    TagRepositoryError::Storage(error.into())
}
