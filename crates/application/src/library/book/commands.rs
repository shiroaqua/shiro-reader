#[derive(Debug)]
pub struct CreateBookCommand {
    pub title: String,
    pub hash: String,
}

#[derive(Debug)]
pub struct DeleteBookCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct DeleteBooksCommand {
    pub ids: Vec<String>,
}

#[derive(Debug)]
pub struct RestoreBookIdCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct PermanentlyDeleteBooksCommand {
    pub ids: Vec<String>,
}

#[derive(Debug)]
pub struct RenameBookCommand {
    pub id: String,
    pub title: String,
}

#[derive(Debug)]
pub struct GetBookCoverCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct GetBookFileMetaCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct OpenBookFileRangeCommand {
    pub id: String,
    pub offset: u64,
    pub length: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ChangeBookSubtitleCommand {
    pub id: String,
    pub subtitle: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookSubtitleCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookAuthorsCommand {
    pub id: String,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClearBookAuthorsCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookPublisherCommand {
    pub id: String,
    pub publisher: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookPublisherCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookPublishedDateCommand {
    pub id: String,
    pub published_date: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookPublishedDateCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookLanguageCommand {
    pub id: String,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookLanguageCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookDescriptionCommand {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookDescriptionCommand {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ChangeBookIdentifierCommand {
    pub id: String,
    pub identifier: String,
}

#[derive(Debug, Clone)]
pub struct ClearBookIdentifierCommand {
    pub id: String,
}
