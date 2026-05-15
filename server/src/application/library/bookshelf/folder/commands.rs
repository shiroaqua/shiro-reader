use crate::domain::library::bookshelf::folder::value_objects::FolderId;

#[derive(Debug)]
pub struct CreateFolderCommand {
    pub bookshelf_id: String,
    pub parent_id: Option<String>,
    pub name: String,
}


#[derive(Debug)]
pub struct RenameFolderCommand {
    pub bookshelf_id: String,
    pub folder_id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct DeleteFolderCommand {
    pub bookshelf_id: String,
    pub folder_id: String,
}


#[derive(Debug)]
pub struct CreateFolderOutput {
    pub id: FolderId,
    pub created_at: i64,
}
