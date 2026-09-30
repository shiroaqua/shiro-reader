use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    BookshelfId,
    CreatedAtHigh,
    CreatedAtLow,
    UpdatedAtHigh,
    UpdatedAtLow,
    Version,
}
#[derive(Iden)]
pub(super) enum Bookshelves {
    #[iden = "bookshelves"]
    Table,
    Id,
    Name,
    CreatedAtHigh,
    CreatedAtLow,
    UpdatedAtHigh,
    UpdatedAtLow,
    Version
}

pub(super) const FULL_BOOKS_TABLE_COLUMNS: [Books; 9] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::BookshelfId,
    Books::CreatedAtHigh,
    Books::CreatedAtLow,
    Books::UpdatedAtHigh,
    Books::UpdatedAtLow,
    Books::Version,
];

pub(super) const INSERT_BOOKS_COLUMNS: [Books; 9] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::BookshelfId,
    Books::CreatedAtHigh,
    Books::CreatedAtLow,
    Books::UpdatedAtHigh,
    Books::UpdatedAtLow,
    Books::Version,
];

pub(super) const FULL_BOOKSHELVES_TABLE_COLUMNS: [Bookshelves; 7] = [
    Bookshelves::Id,
    Bookshelves::Name,
    Bookshelves::CreatedAtHigh,
    Bookshelves::CreatedAtLow,
    Bookshelves::UpdatedAtHigh,
    Bookshelves::UpdatedAtLow,
    Bookshelves::Version,
];
