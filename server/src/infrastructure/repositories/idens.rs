use sea_query::Iden;

#[derive(Iden)]
pub(super) enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    BookshelfId,
    FolderId,
    CreatedAt,
    UpdatedAt,
}
#[derive(Iden)]
pub(super)  enum Bookshelves {
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