use crate::{
    api::v1::library::book::dto::{CreateBookResponse, GetBookResponse},
    application::library::book::commands::{CreateBookOutput, GetBookOutput},
    shared::time,
};

impl From<CreateBookOutput> for CreateBookResponse {
    fn from(output: CreateBookOutput) -> Self {
        Self {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        }
    }
}

impl From<GetBookOutput> for GetBookResponse {
    fn from(output: GetBookOutput) -> Self {
        Self {
            id: output.id.to_string(),
            title: output.title.to_string(),
            hash: output.hash.to_hex().to_string(),
            bookshelf_id: output.bookshelf_id.to_string(),
            folder_id: output.folder_id.map(|id| id.to_string()),
            created_at: time::ms_to_datetime(output.created_at),
            updated_at: time::ms_to_datetime(output.updated_at),
        }
    }
}
