pub mod cloud_client;
pub mod config;
pub mod credentials;
pub mod dto;
pub mod epub_reader;
pub mod error_sink;
pub mod local_import;
pub mod reader_config;
pub mod reader_fonts;
pub mod runtime;
pub mod tag_value;
pub mod upload_manager;

pub use cloud_client::{
    CloudClient, CloudProbeOutcome, DEFAULT_CLOUD_MAX_UPLOAD_BYTES, RemoteBookFileMeta,
    SessionVerifyOutcome, is_session_invalid_code,
};
pub use config::{
    AppTheme, ClientConfig, ClientLogLevel, ConfigLoadError, ConfigStore,
    DEFAULT_UPLOAD_CONCURRENCY, LaunchMode, MAX_UPLOAD_CONCURRENCY, PlatformCapabilities,
    clamp_upload_concurrency, normalize_cloud_origin,
};
pub use credentials::{
    CloudCredentials, CloudUserInfo, CredentialStore, CredentialStoreError,
    EncryptedFileCredentialStore,
};
pub use dto::{
    BootstrapStateDto, CloudOriginCheckResult, CloudStatusDto, DeleteTagStrategyDto,
    InitialSetupRequest, RestoreBooksItemResult, RestoreBooksResult, StartupDiagnostic,
    StartupPhase, TagDefinitionDto, TagTreeNodeDto, UpdateSettingsRequest, UploadActiveStage,
    UploadPanelSnapshotDto, UploadSessionStatsDto, UploadTaskDto, UploadTaskStatusDto,
};
pub use epub_reader::{EpubSessionId, OpenEpubResponseDto};
pub use error_sink::{ErrorDispatcher, ErrorSink, NoopErrorSink};
pub use local_import::{
    ImportProgressCallback, import_reader_into_local, is_opaque_book_file_name,
};
pub use reader_config::{
    CustomFontFormat, CustomFontMetaDto, READER_FONTS_DIR_NAME, READER_SETTINGS_FILE_NAME,
    READER_SETTINGS_SCHEMA_VERSION, ReaderAppearancePatchDto, ReaderAppearanceSettingsDto,
    ReaderBehaviorPatchDto, ReaderBehaviorSettingsDto, ReaderColumnCountDto, ReaderConfigStore,
    ReaderFlowMode, ReaderFontChoiceDto, ReaderFontPreset, ReaderFontWeightDto,
    ReaderHyphenation, ReaderLayoutPatchDto, ReaderLayoutSettingsDto, ReaderParagraphIndentDto,
    ReaderProfileDto, ReaderProfilePatchDto, ReaderSettingsConfig, ReaderSettingsSnapshotDto,
    ReaderSurfacePreset, ReaderTextAlign, ReaderTypographyPatchDto, ReaderTypographySettingsDto,
    normalize_book_hash,
};
pub use reader_fonts::{MAX_CUSTOM_FONT_BYTES, SystemFontFamilyDto};
pub use runtime::ClientRuntime;
pub use shiro_application::library::{
    book::dto::{
        BookAnchorDto, BookMetadataOutput, BookNavigationOutput, BookmarkLocatorDto,
        CreateBookOutput, EpubNavTargetDto, EpubPointTargetDto, EpubRangeTargetDto,
        GetBookOutput, HighlightLocatorDto, LinearUnitOutput, NavigationLocatorDto,
        NormalizedRectDto, PdfDestDto, PdfPageRectsDto, PdfSelectionTargetDto, TocNodeOutput,
        TrashedBookOutput,
    },
    tag::dto::BookTagOutput,
};
pub use tag_value::TagValueType;
pub use upload_manager::{
    BoxedUploadReader, EnqueueUploadItem, ExpandedUploadPaths, OpenUploadSourceFuture,
    StdFsSourceOpener, UploadManager, UploadSourceLocator, UploadSourceOpener,
    expand_upload_paths,
};
