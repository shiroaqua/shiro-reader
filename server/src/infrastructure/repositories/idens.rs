use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    FileType,
    BookshelfId,
    CreatedAt,
    UpdatedAt,
    Version,
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

pub(super) const FULL_BOOKS_TABLE_COLUMNS: [Books; 8] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::FileType,
    Books::BookshelfId,
    Books::CreatedAt,
    Books::UpdatedAt,
    Books::Version,
];

pub(super) const FULL_BOOKSHELVES_TABLE_COLUMNS: [Bookshelves; 5] = [
    Bookshelves::Id,
    Bookshelves::Name,
    Bookshelves::CreatedAt,
    Bookshelves::UpdatedAt,
    Bookshelves::Version,
];
