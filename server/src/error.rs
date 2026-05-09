use axum::{
    response::{IntoResponse, Response},
};

#[derive(Debug)]

pub enum AppError {
    BadRequest {
        message: &'static str,
    },
    NotFound {
        message: &'static str,
    },
    Conflict {
        message: &'static str,
    },
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        todo!()
    }
}