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
