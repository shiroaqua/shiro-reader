use crate::{
    api::v1::library::book::dto::BookResponse,
    application::library::book::dto::{CreateBookOutput, GetBookOutput},
};

impl From<CreateBookOutput> for BookResponse {
    fn from(output: CreateBookOutput) -> Self {
        Self {
            id: output.id,
            title: output.title,
            hash: output.hash,
            file_type: output.file_type,
            created_at: output.created_at,
            updated_at: output.updated_at,
        }
    }
}

impl From<GetBookOutput> for BookResponse {
    fn from(output: GetBookOutput) -> Self {
        Self {
            id: output.id,
            title: output.title,
            hash: output.hash,
            file_type: output.file_type,
            created_at: output.created_at,
            updated_at: output.updated_at,
        }
    }
}
