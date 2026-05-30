use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    Type,
    BookshelfId,
    FolderId,
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
}
#[derive(Iden)]
pub(super) enum Folders {
    #[iden = "folders"]
    Table,
    Id,
    BookshelfId,
    ParentId,
    Name,
    CreatedAt,
    UpdatedAt,
}

pub(super) const FULL_BOOKS_TABLE_COLUMNS: [Books; 8] = [
    Books::Id,
    Books::Title,
    Books::Hash,
    Books::Type,
    Books::BookshelfId,
    Books::FolderId,
    Books::CreatedAt,
    Books::UpdatedAt,
];

pub(super) const FULL_BOOKSHELVES_TABLE_COLUMNS: [Bookshelves; 4] = [
    Bookshelves::Id,
    Bookshelves::Name,
    Bookshelves::CreatedAt,
    Bookshelves::UpdatedAt,
];

pub(super) const FULL_FOLDERS_TABLE_COLUMNS: [Folders; 6] = [
    Folders::Id,
    Folders::BookshelfId,
    Folders::ParentId,
    Folders::Name,
    Folders::CreatedAt,
    Folders::UpdatedAt,
];
