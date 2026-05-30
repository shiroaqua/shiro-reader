use crate::domain::library::bookshelf::folder::value_objects::FolderId;

#[derive(Debug)]
pub struct CreateFolderCommand {
    pub bookshelf_id: String,
    pub parent_id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct RenameFolderCommand {
    pub bookshelf_id: String,
    pub folder_id: String,
    pub name: String,
}


#[derive(Debug)]
pub struct MoveFolderCommand {
    pub bookshelf_id: String,
    pub folder_id: String,
    pub parent_id: String,
}


#[derive(Debug)]
pub struct GetFoldersCommand {
    pub bookshelf_id: String,
    pub id: Option<String>,
    pub recursive: bool,
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


#[derive(Debug)]
pub struct GetFoldersOutput(pub Vec<Node>);


#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub children: Vec<Node>
}
