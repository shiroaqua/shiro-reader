use crate::{
    api::v1::library::book::dto::{CreateBookResponse, GetBookResponse}, application::library::book::dto::{CreateBookOutput, GetBookOutput}, shared::time,
};

impl From<CreateBookOutput> for CreateBookResponse {
    fn from(output: CreateBookOutput) -> Self {
        Self {
            id: output.id.to_string(),
            file_type: output.file_type,
            created_at: time::ns_to_datetime(output.created_at),
        }
    }
}

impl From<GetBookOutput> for GetBookResponse {
    fn from(output: GetBookOutput) -> Self {
        Self {
            id: output.id,
            title: output.title,
            hash: output.hash,
            file_type: output.file_type,
            created_at: time::ns_to_datetime(output.created_at),
            updated_at: time::ns_to_datetime(output.updated_at),
        }
    }
}
