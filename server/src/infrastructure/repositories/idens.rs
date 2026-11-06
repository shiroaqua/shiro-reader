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

#[derive(Iden)]
pub(super) enum Users {
    #[iden = "users"]
    Table,
    Id,
    Name,
    PasswordHash,
    CreatedAtHigh,
    CreatedAtLow,
    UpdatedAtHigh,
    UpdatedAtLow,
    Version,
}

pub(super) const FULL_USERS_TABLE_COLUMNS: [Users; 8] = [
    Users::Id,
    Users::Name,
    Users::PasswordHash,
    Users::CreatedAtHigh,
    Users::CreatedAtLow,
    Users::UpdatedAtHigh,
    Users::UpdatedAtLow,
    Users::Version,
];

pub(super) const INSERT_USERS_COLUMNS: [Users; 8] = [
    Users::Id,
    Users::Name,
    Users::PasswordHash,
    Users::CreatedAtHigh,
    Users::CreatedAtLow,
    Users::UpdatedAtHigh,
    Users::UpdatedAtLow,
    Users::Version,
];

#[derive(Iden)]
pub(super) enum AuthSessions {
    #[iden = "auth_sessions"]
    Table,
    RefreshHash,
    UserId,
}

#[derive(Iden)]
pub(super) enum AuthAccessTokens {
    #[iden = "auth_access_tokens"]
    Table,
    AccessHash,
    RefreshHash,
}

#[derive(Iden)]
pub(super) enum TagDefinitions {
    Table,
    Id,
    Name,
    ParentId,
    AllowsValue,
    ValueRegex,
    Version,
}

pub(super) const FULL_TAG_DEFINITION_COLUMNS: [TagDefinitions; 6] = [
    TagDefinitions::Id,
    TagDefinitions::Name,
    TagDefinitions::ParentId,
    TagDefinitions::AllowsValue,
    TagDefinitions::ValueRegex,
    TagDefinitions::Version,
];

#[derive(Iden)]
pub(super) enum BookTags {
    Table,
    BookId,
    TagId,
    Value,
}
