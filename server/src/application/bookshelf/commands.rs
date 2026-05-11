use crate::domain::bookshelf::value_objects::DirectoryId;

#[derive(Debug)]
pub struct CreateDirectoryCommand {
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug)]
pub struct CreateDirectoryOutput {
    pub id: DirectoryId,
    pub created_at: i64,
}
