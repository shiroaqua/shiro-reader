use crate::application::library::bookshelf::folder::commands::CreateFolderOutput;
use crate::shared::time;
use crate::api::v1::library::bookshelf::folder::dto::CreateFolderResponse;

impl From<CreateFolderOutput> for CreateFolderResponse {
    fn from(output: CreateFolderOutput) -> Self {
        Self {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        }
    }
}