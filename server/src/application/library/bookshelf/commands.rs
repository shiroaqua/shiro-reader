
#[derive(Debug)]
pub struct CreateBookshelfCommand {
    pub name: String,
}

#[derive(Debug)]
pub struct RenameBookshelfCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct DeleteBookshelfCommand {
    pub id: String,
}
