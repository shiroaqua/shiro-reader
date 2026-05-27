use crate::api::v1::library::bookshelf::folder::dto::{
    CreateFolderResponse, FolderNode, GetFoldersResponse,
};
use crate::application::library::bookshelf::folder::commands::{
    CreateFolderOutput, GetFoldersOutput, Node,
};
use crate::shared::time;

impl From<CreateFolderOutput> for CreateFolderResponse {
    fn from(output: CreateFolderOutput) -> Self {
        Self {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        }
    }
}
impl From<GetFoldersOutput> for GetFoldersResponse {
    fn from(value: GetFoldersOutput) -> Self {
        GetFoldersResponse(map(value.0))
    }
}

fn map(raw_node: Vec<Node>) -> Vec<FolderNode> {
    let mut nodes: Vec<FolderNode> = Vec::new();
    for node in raw_node {
        nodes.push(FolderNode {
            id: node.id,
            name: node.name,
            created_at: time::ms_to_datetime(node.created_at),
            updated_at: time::ms_to_datetime(node.updated_at),
            children: map(node.children),
        });
    }
    nodes
}
