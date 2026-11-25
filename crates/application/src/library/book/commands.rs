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
pub struct CreateChapterCommand {
    pub book_id: String,
    pub position: u32,
    pub title: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct DeleteChapterCommand {
    pub book_id: String,
    pub chapter_id: String,
}
