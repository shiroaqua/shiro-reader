pub mod auth;
pub mod error;
pub mod library;
pub mod meta;
pub mod paths;

pub use auth::{LoginRequest, LogoutRequest, RefreshAccessTokenRequest};
pub use error::{ErrorDetail, ErrorResponse};
pub use library::{
    BOOK_FILE_FIELD_FILE, BOOK_FILE_FIELD_HASH, BOOK_FILE_FIELD_SIZE, BindBookTagRequest,
    BookFileQuery, BookIdsRequest, BooksQuery, ChangeTagColorRequest, CreateBookRequest,
    CreateChapterRequest, DefineTagRequest, DeleteTagQuery, DeleteTagStrategy, MoveTagRequest,
    RenameBookRequest, RenameTagRequest, UnbindBookTagRequest,
};
pub use meta::ServerMetaResponse;

/// Canonical service identifier returned by `/api/v1/meta` to prevent connecting to non-Shiro servers.
pub const SERVICE_NAME: &str = "shiro-reader";

/// Supported HTTP API protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Root prefix for all v1 API routes.
pub const API_PREFIX: &str = "/api/v1";

/// Default maximum net book file upload size in Cloud Mode (512 MiB).
pub const DEFAULT_MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_roundtrip_and_protocol_constants() {
        assert_eq!(DEFAULT_MAX_UPLOAD_BYTES, 512 * 1024 * 1024);
        let meta = ServerMetaResponse {
            service: SERVICE_NAME.to_owned(),
            api_version: PROTOCOL_VERSION,
            server_version: "0.0.1".to_owned(),
            max_upload_bytes: DEFAULT_MAX_UPLOAD_BYTES,
        };
        let encoded = serde_json::to_string(&meta).unwrap();
        let decoded: ServerMetaResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, meta);
        assert!(decoded.is_compatible());
    }

    #[test]
    fn sensitive_auth_dtos_redact_secrets_in_debug() {
        let login_req = LoginRequest {
            username: "alice".to_owned(),
            password: "super-secret-password".to_owned(),
        };
        let req_debug = format!("{login_req:?}");
        assert!(req_debug.contains("alice"));
        assert!(!req_debug.contains("super-secret-password"));

        let refresh_req = RefreshAccessTokenRequest {
            token: "refresh-token-secret".to_owned(),
        };
        assert!(!format!("{refresh_req:?}").contains("refresh-token-secret"));

        let logout_req = LogoutRequest {
            token: "logout-refresh-secret".to_owned(),
        };
        assert!(!format!("{logout_req:?}").contains("logout-refresh-secret"));
    }

    #[test]
    fn delete_tag_strategy_and_path_builders_match_v1_contract() {
        assert_eq!(DeleteTagStrategy::Cascade.as_query_str(), "CASCADE");
        assert_eq!(DeleteTagStrategy::Promote.as_query_str(), "PROMOTE");
        assert_eq!(paths::book("b1"), "/api/v1/library/bookshelf/books/b1");
        assert_eq!(
            paths::book_file("b1"),
            "/api/v1/library/bookshelf/books/b1/file"
        );
        assert_eq!(
            paths::BOOK_FILE_ROUTE,
            "/bookshelf/books/{book_id}/file"
        );
        assert_eq!(
            paths::book_cover("b1"),
            "/api/v1/library/bookshelf/books/b1/cover"
        );
        assert_eq!(
            paths::book_tags("b1"),
            "/api/v1/library/bookshelf/books/b1/tags"
        );
        assert_eq!(
            paths::trash_restore("b1"),
            "/api/v1/library/bookshelf/trash/b1/restore"
        );
        assert_eq!(paths::tag("t1"), "/api/v1/library/tags/t1");
        assert_eq!(paths::tag_name("t1"), "/api/v1/library/tags/t1/name");
        assert_eq!(paths::tag_color("t1"), "/api/v1/library/tags/t1/color");
        assert_eq!(paths::tag_parent("t1"), "/api/v1/library/tags/t1/parent");
    }

    #[test]
    fn deny_unknown_fields_rejects_unexpected_keys() {
        let parsed = serde_json::from_str::<CreateBookRequest>(
            r#"{"title":"Book","hash":"abc","extra":true}"#,
        );
        assert!(parsed.is_err());
    }
}
