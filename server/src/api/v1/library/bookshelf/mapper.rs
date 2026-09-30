use crate::{
    api::v1::library::bookshelf::dto::{CreateBookshelfResponse, GetBookshelfResponse},
    application::library::bookshelf::dto::{CreateBookshelfOutput, GetBookshelfOutput},
    shared::time,
};

impl From<CreateBookshelfOutput> for CreateBookshelfResponse {
    fn from(output: CreateBookshelfOutput) -> Self {
        Self {
            id: output.id,
            created_at: time::ns_to_datetime(output.created_at),
        }
    }
}

impl From<GetBookshelfOutput> for GetBookshelfResponse {
    fn from(output: GetBookshelfOutput) -> Self {
        Self {
            id: output.id,
            name: output.name,
            created_at: time::ns_to_datetime(output.created_at),
            updated_at: time::ns_to_datetime(output.updated_at),
        }
    }
}
