use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        bookshelf::folder::{
            commands::{
                CreateFolderCommand, CreateFolderOutput, DeleteFolderCommand, GetFoldersCommand,
                GetFoldersOutput, Node, RenameFolderCommand,
            },
            errors::FolderApplicationError,
            ports::FolderRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::bookshelf::{
        folder::{
            entity::Folder,
            errors::FolderDomainError,
            value_objects::{FolderId, FolderName},
        },
        value_objects::BookshelfId,
    },
    shared::time::now_ms,
};

#[derive(new)]
pub struct FolderService {
    repository: Arc<dyn FolderRepository>,
}

impl FolderService {
    pub async fn create_folder(
        &self,
        command: CreateFolderCommand,
    ) -> Result<CreateFolderOutput, LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let parent_id = command
            .parent_id
            .map(FolderId::parse_parent_id)
            .transpose()?;
        let name = FolderName::parse(command.name)?;
        let now = now_ms();

        let folder = Folder::new(FolderId::new(), bookshelf_id, parent_id, name, now, now);
        let created = self.repository.create(folder).await?;

        Ok(CreateFolderOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }

    pub async fn rename_folder(
        &self,
        command: RenameFolderCommand,
    ) -> Result<(), LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = FolderId::parse_folder_id(command.folder_id)?;
        let new_name = FolderName::parse(command.name)?;
        self.repository
            .rename(&bookshelf_id, &folder_id, &new_name)
            .await?;
        Ok(())
    }

    pub async fn delete_folder(
        &self,
        commmand: DeleteFolderCommand,
    ) -> Result<(), LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(commmand.bookshelf_id)?;
        let folder_id = FolderId::parse_folder_id(commmand.folder_id)?;
        self.repository.delete(&bookshelf_id, &folder_id).await?;
        Ok(())
    }

    pub async fn get_folders(
        &self,
        command: GetFoldersCommand,
    ) -> Result<GetFoldersOutput, LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        if !command.recursive {
            if let Some(folder_id) = command.id {
                let folder_id = FolderId::parse_folder_id(folder_id)?;     
                let folder = self
                    .repository
                    .find_by_id(&bookshelf_id, &folder_id)
                    .await?;

                return Ok(GetFoldersOutput(vec![folder.into()]));
            }
            else {
                let folders = self.repository.list_root(&bookshelf_id).await?;
                return Ok(GetFoldersOutput(folders.into_iter().map(|f| f.into()).collect()));
            }
        }
        else {
            // 不要骂我，我知道这么写效率很低，以后一定会改的...
            let folders = self.repository.list(&bookshelf_id).await?;
            if let Some(folder_id) = command.id {
                let node = find_node(build_tree(folders, None).0, &folder_id.to_string());
                if let Some(node) = node {
                    return Ok(GetFoldersOutput(vec![node]));
                } else {
                    return Err(LibraryApplicationError::Folder(
                        FolderApplicationError::NotFound,
                    ));
                }
            }
            else {
                return Ok(GetFoldersOutput(build_tree(folders, None).0));
            }
        }
    }
}

fn find_node(tree: Vec<Node>, target_id: &String) -> Option<Node> {
    for node in tree {
        if &node.id == target_id {
            return Some(node);
        } else if node.children.len() > 0 {
            if let Some(node)  = find_node(node.children, target_id) {
                return Some(node);
            }
        }
    }
    None
}

fn build_tree(folders: Vec<Folder>, parent_id: Option<FolderId>) -> (Vec<Node>, Vec<Folder>) {
    let mut nodes: Vec<Node> = Vec::new();
    if folders.is_empty() {
        return (Vec::new(), folders);
    }

    let (children, mut other): (Vec<_>, Vec<_>) =
        folders.into_iter().partition(|f| f.parent_id == parent_id);
    for folder in children {
        let id = folder.id.clone();
        let mut node: Node = folder.into();
        if !other.is_empty() {
            let (c, o) = build_tree(other, Some(id));
            other = o;
            node.children = c;
        }

        nodes.push(node);
    }
    (nodes, other)
}

impl From<Folder> for Node {
    fn from(value: Folder) -> Self {
        Node {
            id: value.id.to_string(),
            name: value.name.to_string(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            children: Vec::new(),
        }
    }
}

impl From<FolderDomainError> for FolderApplicationError {
    fn from(value: FolderDomainError) -> Self {
        match value {
            FolderDomainError::InvalidId => Self::InvalidId,
            FolderDomainError::InvalidParentId => Self::InvalidParentId,
            FolderDomainError::MissingId => Self::MissingFolderId,
            FolderDomainError::MissingName => Self::MissingName,
            FolderDomainError::InvalidNameFormat => Self::InvalidNameFormat,
        }
    }
}
