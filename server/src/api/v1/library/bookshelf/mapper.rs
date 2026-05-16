use crate::{
    api::v1::library::bookshelf::dto::{CreateBookshelfResponse, GetBookshelfResponse},
    application::library::bookshelf::commands::{CreateBookshelfOutput, GetBookshelfOutput},
    shared::time,
};

impl From<CreateBookshelfOutput> for CreateBookshelfResponse {
    fn from(output: CreateBookshelfOutput) -> Self {
        Self {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        }
    }
}

impl From<GetBookshelfOutput> for GetBookshelfResponse {
    fn from(output: GetBookshelfOutput) -> Self {
        Self {
            id: output.id.to_string(),
            name: output.name.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
            updated_at: time::ms_to_datetime(output.updated_at),
        }
    }
}
