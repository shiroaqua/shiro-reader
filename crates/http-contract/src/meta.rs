use serde::{Deserialize, Serialize};

use crate::{PROTOCOL_VERSION, SERVICE_NAME};

/// Lightweight unauthenticated server protocol probe response (`GET /api/v1/meta`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMetaResponse {
    pub service: String,
    pub api_version: u32,
    pub server_version: String,
    pub max_upload_bytes: u64,
}

impl ServerMetaResponse {
    pub fn is_compatible(&self) -> bool {
        self.service == SERVICE_NAME && self.api_version == PROTOCOL_VERSION
    }
}
