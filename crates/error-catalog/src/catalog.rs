use crate::AppLocale;

/// Static error catalog metadata for a single [`ErrorCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    pub code: u32,
    pub key: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
}

impl CatalogEntry {
    pub const fn localize(self, locale: AppLocale) -> &'static str {
        match locale {
            AppLocale::Zh => self.zh,
            AppLocale::En => self.en,
        }
    }
}

macro_rules! define_error_catalog {
    (
        $(
            $variant:ident => ($code:expr, $key:expr, $zh:expr, $en:expr)
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ErrorCode {
            $( $variant, )*
        }

        impl ErrorCode {
            pub const ALL: &'static [Self] = &[
                $( Self::$variant, )*
            ];

            pub const fn entry(self) -> CatalogEntry {
                match self {
                    $(
                        Self::$variant => CatalogEntry {
                            code: $code,
                            key: $key,
                            zh: $zh,
                            en: $en,
                        },
                    )*
                }
            }

            pub const fn code(self) -> u32 {
                self.entry().code
            }

            pub const fn key(self) -> &'static str {
                self.entry().key
            }

            pub const fn localize(self, locale: AppLocale) -> &'static str {
                self.entry().localize(locale)
            }

            pub fn from_code(code: u32) -> Option<Self> {
                match code {
                    $( $code => Some(Self::$variant), )*
                    _ => None,
                }
            }

            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $( $key => Some(Self::$variant), )*
                    _ => None,
                }
            }
        }
    };
}

define_error_catalog! {
    // Identity authentication (1001..1005)
    AuthenticationInvalidCredentials => (1001, "identity.authentication.invalid_credentials", "用户名或密码错误。", "Incorrect username or password."),
    AuthenticationMissingBearerToken => (1002, "identity.authentication.missing_bearer_token", "请先登录。", "Please sign in first."),
    AuthenticationInvalidToken => (1003, "identity.authentication.invalid_token", "登录已失效，请重新登录。", "Your session has expired. Please sign in again."),
    AuthenticationUserNotFound => (1004, "identity.authentication.user_not_found", "用户不存在，请重新登录。", "User not found. Please sign in again."),
    AuthenticationInvalidTokenType => (1005, "identity.authentication.invalid_token_type", "登录已失效，请重新登录。", "Your session has expired. Please sign in again."),

    // Identity user (1101..1108)
    UserInvalidId => (1101, "identity.user.invalid_id", "用户 ID 无效。", "Invalid user ID."),
    UserMissingName => (1102, "identity.user.missing_user_name", "请输入用户名。", "Please enter a username."),
    UserInvalidNameFormat => (1103, "identity.user.invalid_user_name_format", "用户名格式无效。", "Invalid username format."),
    UserInvalidPasswordHash => (1104, "identity.user.invalid_password_hash", "密码哈希无效。", "Invalid password hash."),
    UserNotFound => (1105, "identity.user.not_found", "用户不存在。", "User does not exist."),
    UserNameConflict => (1106, "identity.user.name_conflict", "用户名已存在。", "That username is already in use."),
    UserNameUnchanged => (1107, "identity.user.name_unchanged", "用户名未发生变化。", "The username has not changed."),
    UserPasswordHashUnchanged => (1108, "identity.user.password_hash_unchanged", "密码未发生变化。", "The password has not changed."),

    // Library book (2001..2011)
    BookInvalidId => (2001, "library.book.invalid_id", "书籍 ID 无效。", "Invalid book ID."),
    BookMissingTitle => (2002, "library.book.missing_title", "请输入书籍标题。", "Please enter a book title."),
    BookInvalidTitleFormat => (2003, "library.book.invalid_title_format", "书籍标题格式无效。", "Invalid book title format."),
    BookInvalidChapterLocation => (2004, "library.book.invalid_location", "定位信息无效。", "Invalid location."),
    BookPositionConflict => (2005, "library.book.position_conflict", "该顺序已被占用。", "A book item already exists at this position."),
    BookNotFound => (2006, "library.book.not_found", "书籍不存在。", "Book not found."),
    BookTitleConflict => (2007, "library.book.title_conflict", "已存在同名书籍。", "A book with this title already exists."),
    BookTitleUnchanged => (2008, "library.book.title_unchanged", "书籍标题未发生变化。", "The book title has not changed."),
    BookAlreadyDeleted => (2009, "library.book.already_deleted", "书籍已在回收站中。", "The book is already in the recycle bin."),
    BookNotDeleted => (2010, "library.book.not_deleted", "书籍不在回收站中，无法恢复。", "The book is not in the recycle bin and cannot be restored."),
    BookInvalidChapterTitle => (2011, "library.book.invalid_chapter_title", "章节标题格式无效。", "Invalid chapter title format."),

    // Library book file (2101..2108, 2110..2117)
    BookFileUploadConflict => (2101, "library.book.file.upload.conflict", "文件上传冲突，请重试。", "File upload conflict. Please try again."),
    BookFileAlreadyExists => (2102, "library.book.file.already_exists", "书籍文件已存在。", "The book file already exists."),
    BookFileInvalidHashFormat => (2103, "library.book.file.invalid_hash_format", "文件哈希格式无效。", "Invalid file hash format."),
    BookFileHashMismatch => (2104, "library.book.file.hash_mismatch", "文件哈希与提供的哈希不一致。", "The file hash does not match the supplied hash."),
    BookFileNotFound => (2105, "library.book.file.not_found", "书籍文件不存在。", "Book file not found."),
    BookFileUnsupported => (2106, "library.book.file.unsupported", "不支持此书籍文件格式。", "This book file format is not supported."),
    BookFileUploadTimeout => (2107, "library.book.file.upload.timeout", "文件上传超时，请检查网络或重试。", "File upload timed out. Check your connection and try again."),
    BookFileSizeMismatch => (2108, "library.book.file.size_mismatch", "上传文件实际大小与声明大小不一致。", "The uploaded file size does not match the declared size."),
    BookFileUploadDuplicateHash => (2110, "library.book.file.upload.duplicate_hash", "上传请求包含重复的哈希字段。", "The upload contains duplicate hash fields."),
    BookFileUploadMissingHash => (2111, "library.book.file.upload.missing_hash", "上传请求缺少文件哈希。", "The upload is missing the file hash."),
    BookFileUploadMissingFile => (2112, "library.book.file.upload.missing_file", "请选择要上传的文件。", "Please select a file to upload."),
    BookFileUploadDuplicateSize => (2113, "library.book.file.upload.duplicate_size", "上传请求包含重复的文件大小字段。", "The upload contains duplicate file size fields."),
    BookFileUploadMissingSize => (2114, "library.book.file.upload.missing_size", "上传请求缺少文件大小字段。", "The upload is missing the file size field."),
    BookFileUploadInvalidSize => (2115, "library.book.file.upload.invalid_size", "上传请求的文件大小格式无效。", "Invalid file size in the upload request."),
    BookFileRangeNotSatisfiable => (2116, "library.book.file.range_not_satisfiable", "请求的文件字节范围超出有效长度。", "The requested file byte range is not satisfiable."),
    BookFileInvalidRange => (2117, "library.book.file.invalid_range", "请求的字节范围格式无效或不受支持。", "The requested byte range is invalid or unsupported."),

    // Library tag (2201..2218)
    TagInvalidId => (2201, "library.tag.invalid_id", "标签 ID 无效。", "Invalid tag ID."),
    TagMissingName => (2202, "library.tag.missing_name", "请输入标签名称。", "Please enter a tag name."),
    TagInvalidNameFormat => (2203, "library.tag.invalid_name_format", "标签名称只能包含文字、数字和点号。", "Tag names can only contain letters, numbers, and periods."),
    TagNameUnchanged => (2204, "library.tag.name_unchanged", "标签名称未发生变化。", "The tag name has not changed."),
    TagInvalidColor => (2205, "library.tag.invalid_color", "标签颜色格式无效，请使用 #RRGGBB。", "Invalid tag color. Use the #RRGGBB format."),
    TagColorUnchanged => (2206, "library.tag.color_unchanged", "标签颜色未发生变化。", "The tag color has not changed."),
    TagInvalidValueRegex => (2207, "library.tag.invalid_value_regex", "标签值的正则表达式无效。", "Invalid tag value pattern."),
    TagValueRegexNotAllowed => (2208, "library.tag.value_regex_not_allowed", "不允许填写值的标签不能设置正则表达式。", "Tags without values cannot have a value pattern."),
    TagValueRequired => (2209, "library.tag.value_required", "请填写标签值。", "Please enter a tag value."),
    TagValueNotAllowed => (2210, "library.tag.value_not_allowed", "此标签不允许填写值。", "This tag does not allow a value."),
    TagValueRegexMismatch => (2211, "library.tag.value_regex_mismatch", "标签值不符合指定格式。", "The tag value does not match the required pattern."),
    TagAlreadyDeleted => (2212, "library.tag.already_deleted", "标签绑定已删除。", "The tag binding is already deleted."),
    TagNotDeleted => (2213, "library.tag.not_deleted", "标签绑定尚未删除。", "The tag binding is not deleted."),
    TagNotFound => (2214, "library.tag.not_found", "标签不存在。", "Tag not found."),
    TagParentNotFound => (2215, "library.tag.parent_not_found", "父标签不存在，请刷新后重试。", "Parent tag not found. Please refresh and try again."),
    TagInvalidMove => (2216, "library.tag.invalid_move", "不能将标签移动到自身或其子标签下。", "A tag cannot be moved under itself or one of its descendants."),
    TagInvalidDeleteStrategy => (2217, "library.tag.invalid_delete_strategy", "标签删除方式无效。", "Invalid tag deletion strategy."),
    TagSiblingNameConflict => (2218, "library.tag.sibling_name_conflict", "同级标签不能使用相同名称。", "Tags at the same level must have different names."),

    // Client lifecycle, settings, local runtime, file import & platform (3001..3015)
    SetupRequired => (3001, "client.setup.required", "尚未完成初始设置，请先选择运行模式并保存配置。", "Initial setup is required before using ShiroReader."),
    ConfigInvalid => (3002, "client.config.invalid", "客户端配置文件损坏或格式无效，请检查或重新完成设置。", "The client configuration file is corrupted or invalid."),
    ConfigSaveFailed => (3003, "client.config.save_failed", "保存客户端设置失败，请检查配置目录写入权限。", "Failed to save client settings. Check directory write permissions."),
    LaunchModeLocked => (3004, "client.mode.locked", "当前进程的运行模式已固定，模式变更将在应用重启后生效。", "The launch mode is fixed for this session. Changes take effect after restarting."),
    RuntimeNotReady => (3005, "client.runtime.not_ready", "书库尚未就绪，请先完成设置或处理启动错误。", "The library runtime is not ready. Complete setup or resolve the startup error first."),
    LocalRootInvalid => (3006, "client.local.root_invalid", "本地书库目录无效、无法访问或缺少写入权限。", "The local library directory is invalid, inaccessible, or not writable."),
    LocalDatabaseCorrupted => (3007, "client.local.database_corrupted", "本地书库数据库损坏或格式无效，已阻止静默新建空书库，请修复或更换目录。", "The local library database is corrupted or invalid. No empty library was created; repair or select another directory."),
    LocalStartupFailed => (3008, "client.local.startup_failed", "本地书库初始化失败，请查看详情后重试或更换书库目录。", "Failed to initialize the local library. Review the details, retry, or choose another directory."),
    FolderPickerNotImplemented => (3009, "client.platform.folder_picker_not_implemented", "当前平台固定使用应用私有目录，不支持选择自定义书库目录。", "This platform uses the fixed private app directory and does not support selecting a custom library folder."),
    ImportFileTooLarge => (3010, "client.import.file_too_large", "所选文件超过 512 MiB 大小限制。", "The selected file exceeds the 512 MiB size limit."),
    ImportReadFailed => (3011, "client.import.read_failed", "读取所选书籍文件失败，请检查文件权限后重试。", "Failed to read the selected book file. Check file permissions and try again."),
    ImportCancelled => (3012, "client.import.cancelled", "文件导入已取消。", "File import was cancelled."),
    CloudInvalidOrigin => (3013, "client.cloud.invalid_origin", "服务端地址无效，请输入有效的 HTTP 或 HTTPS 地址。", "Invalid server URL. Please enter a valid HTTP or HTTPS origin."),
    CloudProtocolIncompatible => (3016, "client.cloud.protocol_incompatible", "服务端协议版本不受支持，请升级服务端或检查云端地址。", "The server protocol version is unsupported. Upgrade the server or check the server URL."),
    CloudNetworkUnreachable => (3019, "client.cloud.network_unreachable", "无法连接到云端服务端，请检查网络连接或服务状态后重试。", "Cannot reach the cloud server. Check your network connection or server status and try again."),
    CloudRequestTimeout => (3020, "client.cloud.request_timeout", "云端请求超时，请检查网络连接后重试。", "The cloud request timed out. Check your connection and try again."),
    CloudCredentialsCorrupted => (3021, "client.cloud.credentials_corrupted", "本地加密凭据文件已损坏，请重新登录。", "The local encrypted credential file is corrupted. Please sign in again."),
    CloudCredentialsKeyMissing => (3022, "client.cloud.credentials_key_missing", "本地凭据密钥缺失，无法解密已保存会话，请重新登录。", "The local credential encryption key is missing. Please sign in again."),
    CloudCredentialsSaveFailed => (3023, "client.cloud.credentials_save_failed", "保存加密凭据失败，请检查应用目录写入权限。", "Failed to save encrypted credentials. Check directory write permissions."),
    CloudCrossOriginRedirect => (3024, "client.cloud.cross_origin_redirect", "服务端返回了跨源或不安全重定向，已出于安全原因拦截请求。", "The server attempted an unsafe or cross-origin redirect, which was blocked for security."),
    CloudNotAuthenticated => (3025, "client.cloud.not_authenticated", "当前尚未登录云端账号，请先登录。", "You are not signed in to the cloud server. Please sign in first."),
    CloudInvalidResponse => (3026, "client.cloud.invalid_response", "服务端返回了无法解析的响应格式。", "The server returned an unexpected response format."),
    ImportNotSupportedOnPlatform => (3030, "client.platform.import_not_supported", "当前平台暂不支持上传书籍。", "Book upload is not currently supported on this platform."),
    ImportNoSupportedBooksFound => (3031, "client.import.no_supported_books_found", "所选文件夹中未找到受支持的电子书文件（PDF / EPUB）。", "No supported e-book files (PDF / EPUB) were found in the selected folder."),
    ImportNotificationPermissionRequired => (3032, "client.import.notification_permission_required", "需要授予通知权限才能在移动端上传书籍，请在系统设置中开启通知后重试。", "Notification permission is required to upload books on mobile. Please enable notifications and try again."),

    // Client EPUB reader (3101..3105)
    ReaderSessionNotFound => (3101, "client.reader.session_not_found", "阅读会话不存在或已关闭。", "The reader session was not found or has been closed."),
    ReaderEntryNotFound => (3102, "client.reader.entry_not_found", "书籍内部找不到指定资源路径。", "The requested entry was not found in the book archive."),
    ReaderUnsupportedFormat => (3103, "client.reader.unsupported_format", "当前阅读器仅支持打开 EPUB 格式书籍。", "The reader currently only supports EPUB books."),
    ReaderCorruptedEpub => (3104, "client.reader.corrupted_epub", "EPUB 文件结构损坏或包含不支持的压缩格式。", "The EPUB archive is corrupted or uses an unsupported compression method."),
    ReaderEntryTooLarge => (3105, "client.reader.entry_too_large", "书籍内部条目或目录体积超过安全限制。", "An archive entry or directory exceeds the safety size limit."),

    // Common (9001, 9002, 9999)
    CommonPayloadTooLarge => (9001, "common.payload_too_large", "上传内容过大。", "The upload is too large."),
    CommonInternalServerError => (9002, "common.internal_server_error", "内部处理失败，请稍后重试。", "Internal error. Please try again later."),
    Unknown => (9999, "common.unknown_error", "发生未知错误，请重试。", "An unknown error occurred. Please try again."),
}
