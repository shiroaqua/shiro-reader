use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    CreatedAtHigh,
    CreatedAtLow,
    UpdatedAtHigh,
    UpdatedAtLow,
    Version,
}

pub(super) const FULL_BOOKS_TABLE_COLUMNS: [Books; 8] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::CreatedAtHigh,
    Books::CreatedAtLow,
    Books::UpdatedAtHigh,
    Books::UpdatedAtLow,
    Books::Version,
];

pub(super) const INSERT_BOOKS_COLUMNS: [Books; 8] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::CreatedAtHigh,
    Books::CreatedAtLow,
    Books::UpdatedAtHigh,
    Books::UpdatedAtLow,
    Books::Version,
];