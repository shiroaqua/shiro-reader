pub mod cloud_client;
pub mod config;
pub mod credentials;
pub mod dto;
pub mod error_sink;
pub mod local_import;
pub mod runtime;

pub use cloud_client::{
    CloudClient, CloudProbeOutcome, DEFAULT_CLOUD_MAX_UPLOAD_BYTES, SessionVerifyOutcome,
    is_cloud_abort_import_code, is_session_invalid_code,
};
pub use config::{
    ClientConfig, ClientLogLevel, ConfigLoadError, ConfigStore, LaunchMode, PlatformCapabilities,
    normalize_cloud_origin,
};
pub use credentials::{
    CloudCredentials, CloudUserInfo, CredentialStore, CredentialStoreError,
    EncryptedFileCredentialStore,
};
pub use dto::{
    BootstrapStateDto, CloudOriginCheckResult, CloudStatusDto, DeleteTagStrategyDto,
    ImportBookItemFailure, ImportBooksResult, ImportProgressEvent, InitialSetupRequest,
    RestoreBooksItemResult, RestoreBooksResult, StartupDiagnostic, StartupPhase,
    UpdateSettingsRequest,
};
pub use error_sink::{ErrorDispatcher, ErrorSink, NoopErrorSink};
pub use local_import::{
    ImportProgressCallback, import_reader_into_local, is_opaque_book_file_name,
};
pub use runtime::ClientRuntime;
pub use shiro_application::library::{
    book::dto::{CreateBookOutput, GetBookOutput, TrashedBookOutput},
    tag::dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
};
