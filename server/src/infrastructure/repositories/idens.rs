use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    BookshelfId,
    CreatedAt,
    UpdatedAt,
}
#[derive(Iden)]
pub(super) enum Bookshelves {
    #[iden = "bookshelves"]
    Table,
    Id,
    Name,
    CreatedAt,
    UpdatedAt,
    Version
}

pub(super) const FULL_BOOKS_TABLE_COLUMNS: [Books; 6] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::BookshelfId,
    Books::CreatedAt,
    Books::UpdatedAt,
];

pub(super) const FULL_BOOKSHELVES_TABLE_COLUMNS: [Bookshelves; 4] = [
    Bookshelves::Id,
    Bookshelves::Name,
    Bookshelves::CreatedAt,
    Bookshelves::UpdatedAt,
];

