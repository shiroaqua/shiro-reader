use serde::{Deserialize, Serialize};

/// Canonical JSON error envelope returned by the ShiroReader v1 HTTP API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: u32,
    pub message: String,
}
