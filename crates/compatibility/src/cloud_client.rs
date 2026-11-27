use std::{
    error::Error as StdError,
    io::{self, SeekFrom},
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::{
    Body as ReqwestBody, Client, Method, Response, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
    multipart::{Form, Part},
    redirect::Policy as RedirectPolicy,
};
use serde::{Serialize, de::DeserializeOwned};
use shiro_application::{
    identity::{
        authentication::dto::{LoginOutput, RefreshAccessTokenOutput},
        user::dto::GetUserOutput,
    },
    library::{
        book::dto::{
            BookMetadataOutput, BookNavigationOutput, CreateBookOutput, GetBookOutput,
            TrashedBookOutput,
        },
        ports::CoverByteStream,
        tag::dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
    },
};
use shiro_domain::library::book::value_objects::BookFileType;
use shiro_error_catalog::ErrorCode;
use shiro_http_contract::{
    BindBookTagRequest, BookIdsRequest, ChangeBookAuthorsRequest, ChangeBookDescriptionRequest,
    ChangeBookIdentifierRequest, ChangeBookLanguageRequest, ChangeBookPublishedDateRequest,
    ChangeBookPublisherRequest, ChangeBookSubtitleRequest, ChangeTagColorRequest,
    CreateBookRequest, DEFAULT_MAX_UPLOAD_BYTES, DefineTagRequest, DeleteTagStrategy,
    ErrorResponse, LoginRequest, LogoutRequest, MoveTagRequest, RefreshAccessTokenRequest,
    RenameBookRequest, RenameTagRequest, ServerMetaResponse, UnbindBookTagRequest, paths,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeekExt},
    sync::Mutex as AsyncMutex,
};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use tracing::{debug, warn};
use url::Url;

use crate::{
    credentials::{CloudCredentials, CloudUserInfo, CredentialStore, CredentialStoreError},
    dto::DeleteTagStrategyDto,
    epub_reader::ByteRange,
    local_import::{
        ImportProgressCallback, STREAM_CHUNK_BYTES, StagedBook, derive_book_title,
        resolve_staged_book_title, stage_book,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteBookFileMeta {
    pub hash: blake3::Hash,
    pub file_type: BookFileType,
    pub size: u64,
}

/// Default maximum net book file upload size in Cloud Mode before server probe (512 MiB).
pub const DEFAULT_CLOUD_MAX_UPLOAD_BYTES: u64 = DEFAULT_MAX_UPLOAD_BYTES;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STANDARD_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
#[error("cross-origin or insecure redirect blocked")]
struct CrossOriginRedirectError;

/// Outcome of probing a configured cloud origin (`GET /api/v1/meta`).
#[derive(Debug, Clone)]
pub enum CloudProbeOutcome {
    Compatible(ServerMetaResponse),
    ProtocolIncompatible {
        detail: String,
    },
    Unreachable {
        code: ErrorCode,
        detail: String,
    },
}

/// Outcome of checking an existing saved session against `/api/v1/auth/me`.
#[derive(Debug, Clone)]
pub enum SessionVerifyOutcome {
    Valid(CloudUserInfo),
    Invalidated(ErrorCode),
    TransientFailure { code: ErrorCode, detail: String },
}

enum RefreshError {
    Revoked(ErrorCode),
    Transient(ErrorCode),
}

/// Rust HTTP/HTTPS client for ShiroReader v1 Cloud Mode.
///
/// Design invariants:
/// - Tokens live solely inside Rust (`session` + encrypted `CredentialStore`), never in IPC/WebView.
/// - Cross-origin redirects (including scheme/host/port changes) are blocked before forwarding `Authorization`.
/// - Concurrent 401 refresh requests coalesce via `refresh_gate` + `refresh_epoch`; replayable
///   requests retry at most once; streaming write uploads NEVER blindly replay consumed streams.
/// - Transient network errors preserve recoverable sessions; only confirmed server auth rejection
///   clears saved credentials.
pub struct CloudClient {
    origin: String,
    temp_dir: PathBuf,
    credential_store: Arc<dyn CredentialStore>,
    http: Client,
    session: RwLock<Option<CloudCredentials>>,
    refresh_gate: AsyncMutex<()>,
    refresh_epoch: AtomicU64,
    max_upload_bytes: AtomicU64,
    on_session_invalidated: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginKey {
    scheme: String,
    host: String,
    port: u16,
}

impl OriginKey {
    fn from_url(url: &Url) -> Option<Self> {
        Some(Self {
            scheme: url.scheme().to_ascii_lowercase(),
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default()?,
        })
    }
}

impl CloudClient {
    pub fn new(
        origin: String,
        temp_dir: PathBuf,
        credential_store: Arc<dyn CredentialStore>,
        on_session_invalidated: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> anyhow::Result<Self> {
        let parsed = Url::parse(&origin)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            anyhow::bail!("cloud client requires an http:// or https:// origin");
        }
        let origin_key =
            OriginKey::from_url(&parsed).ok_or_else(|| anyhow::anyhow!("invalid cloud origin"))?;
        let http = build_http_client(&origin_key)?;

        Ok(Self {
            origin,
            temp_dir,
            credential_store,
            http,
            session: RwLock::new(None),
            refresh_gate: AsyncMutex::new(()),
            refresh_epoch: AtomicU64::new(0),
            max_upload_bytes: AtomicU64::new(DEFAULT_CLOUD_MAX_UPLOAD_BYTES),
            on_session_invalidated,
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_bytes.load(Ordering::Relaxed)
    }

    pub fn current_user(&self) -> Option<CloudUserInfo> {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(CloudUserInfo::from)
    }

    pub fn has_saved_session(&self) -> bool {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Loads previously encrypted credentials for `self.origin` into memory.
    pub fn load_saved_credentials(&self) -> Result<Option<CloudUserInfo>, ErrorCode> {
        match self.credential_store.load(&self.origin) {
            Ok(Some(creds)) => {
                let info = CloudUserInfo::from(&creds);
                *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(creds);
                Ok(Some(info))
            }
            Ok(None) => {
                *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
                Ok(None)
            }
            Err(CredentialStoreError::KeyMissing(_)) => {
                *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
                Err(ErrorCode::CloudCredentialsKeyMissing)
            }
            Err(CredentialStoreError::Corrupted(_)) => {
                *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
                Err(ErrorCode::CloudCredentialsCorrupted)
            }
            Err(_) => {
                *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
                Err(ErrorCode::CloudCredentialsCorrupted)
            }
        }
    }

    /// Probes `GET /api/v1/meta` to verify connectivity and protocol version compatibility.
    pub async fn probe(&self) -> CloudProbeOutcome {
        let url = self.url(paths::META);
        let resp = match self.http.get(&url).timeout(PROBE_TIMEOUT).send().await {
            Ok(resp) => resp,
            Err(err) => {
                if is_cross_origin_redirect(&err) {
                    return CloudProbeOutcome::Unreachable {
                        code: ErrorCode::CloudCrossOriginRedirect,
                        detail: err.to_string(),
                    };
                }
                if err.is_timeout() {
                    return CloudProbeOutcome::Unreachable {
                        code: ErrorCode::CloudRequestTimeout,
                        detail: err.to_string(),
                    };
                }
                return CloudProbeOutcome::Unreachable {
                    code: ErrorCode::CloudNetworkUnreachable,
                    detail: err.to_string(),
                };
            }
        };

        if resp.status().is_redirection() {
            return CloudProbeOutcome::Unreachable {
                code: ErrorCode::CloudCrossOriginRedirect,
                detail: format!("unexpected redirect HTTP {}", resp.status().as_u16()),
            };
        }

        if resp.status() != StatusCode::OK {
            return CloudProbeOutcome::ProtocolIncompatible {
                detail: format!(
                    "GET {} returned HTTP {} (expected 200 with ShiroReader v1 metadata)",
                    paths::META,
                    resp.status().as_u16()
                ),
            };
        }

        match resp.json::<ServerMetaResponse>().await {
            Ok(meta) if meta.is_compatible() => {
                if meta.max_upload_bytes > 0 {
                    self.max_upload_bytes
                        .store(meta.max_upload_bytes, Ordering::Relaxed);
                }
                CloudProbeOutcome::Compatible(meta)
            }
            Ok(meta) => CloudProbeOutcome::ProtocolIncompatible {
                detail: format!(
                    "incompatible server metadata: service='{}', api_version={}",
                    meta.service, meta.api_version
                ),
            },
            Err(err) => CloudProbeOutcome::ProtocolIncompatible {
                detail: format!("invalid v1 metadata JSON: {err}"),
            },
        }
    }

    /// Verifies the currently loaded session via `GET /api/v1/auth/me` (with auto-refresh).
    pub async fn verify_session(&self) -> SessionVerifyOutcome {
        if !self.has_saved_session() {
            return SessionVerifyOutcome::Invalidated(ErrorCode::CloudNotAuthenticated);
        }

        match self
            .send_authorized_json::<(), GetUserOutput>(Method::GET, paths::AUTH_ME, None)
            .await
        {
            Ok(me) => {
                // Keep canonical username synchronized with server.
                let updated = {
                    let mut guard = self.session.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(creds) = guard.as_mut() {
                        if creds.username != me.name || creds.user_id != me.id {
                            creds.user_id = me.id.clone();
                            creds.username = me.name.clone();
                            Some(creds.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(creds) = updated {
                    let _ = self.credential_store.save(&self.origin, &creds);
                }
                SessionVerifyOutcome::Valid(CloudUserInfo {
                    user_id: me.id,
                    username: me.name,
                })
            }
            Err(code) if is_session_invalid_code(code) => SessionVerifyOutcome::Invalidated(code),
            Err(code) => SessionVerifyOutcome::TransientFailure {
                code,
                detail: code.key().to_owned(),
            },
        }
    }

    /// Authenticates against `POST /api/v1/auth/login` and encrypts returned tokens on disk.
    pub async fn login(
        &self,
        username: String,
        password: String,
    ) -> Result<CloudUserInfo, ErrorCode> {
        let resp = self
            .http
            .post(self.url(paths::AUTH_LOGIN))
            .timeout(STANDARD_REQUEST_TIMEOUT)
            .json(&LoginRequest { username, password })
            .send()
            .await
            .map_err(classify_transport_error)?;

        if resp.status().is_redirection() {
            return Err(ErrorCode::CloudCrossOriginRedirect);
        }
        if !resp.status().is_success() {
            return Err(parse_server_error(resp).await);
        }

        let payload: LoginOutput = resp
            .json()
            .await
            .map_err(|_| ErrorCode::CloudInvalidResponse)?;

        let creds = CloudCredentials {
            origin: self.origin.clone(),
            user_id: payload.user_id.clone(),
            username: payload.username.clone(),
            access_token: payload.access_token,
            refresh_token: payload.refresh_token,
        };

        self.credential_store
            .save(&self.origin, &creds)
            .map_err(|_| ErrorCode::CloudCredentialsSaveFailed)?;

        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(creds);
        self.refresh_epoch.fetch_add(1, Ordering::AcqRel);

        Ok(CloudUserInfo {
            user_id: payload.user_id,
            username: payload.username,
        })
    }

    /// Revokes the cloud session on the server (best-effort) and ALWAYS wipes local credentials.
    pub async fn logout(&self) -> Result<(), ErrorCode> {
        let refresh_token = {
            let mut guard = self.session.write().unwrap_or_else(|e| e.into_inner());
            guard.take().map(|creds| creds.refresh_token.clone())
        };

        if let Some(token) = refresh_token {
            let _ = self
                .http
                .post(self.url(paths::AUTH_LOGOUT))
                .timeout(LOGOUT_TIMEOUT)
                .json(&LogoutRequest { token })
                .send()
                .await;
        }

        self.credential_store
            .clear(&self.origin)
            .map_err(|_| ErrorCode::CloudCredentialsSaveFailed)?;
        self.refresh_epoch.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// Test helper / diagnostic: forces an access token overwrite in memory without altering refresh token.
    #[doc(hidden)]
    pub fn replace_access_token_for_test(&self, token: String) {
        if let Some(creds) = self
            .session
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            creds.access_token = token;
        }
    }

    // -------------------------------------------------------------------------
    // Bookshelf & Trash Operations
    // -------------------------------------------------------------------------

    pub async fn list_books(&self) -> Result<Vec<GetBookOutput>, ErrorCode> {
        self.send_authorized_json::<(), Vec<GetBookOutput>>(Method::GET, paths::BOOKS, None)
            .await
    }

    pub async fn get_book_navigation(
        &self,
        book_id: &str,
    ) -> Result<BookNavigationOutput, ErrorCode> {
        self.send_authorized_json::<(), BookNavigationOutput>(
            Method::GET,
            &paths::book_navigation(book_id),
            None,
        )
        .await
    }

    pub async fn get_book_metadata(&self, book_id: &str) -> Result<BookMetadataOutput, ErrorCode> {
        self.send_authorized_json::<(), BookMetadataOutput>(
            Method::GET,
            &paths::book_metadata(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_subtitle(
        &self,
        book_id: &str,
        subtitle: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_subtitle(book_id),
            Some(&ChangeBookSubtitleRequest { subtitle }),
        )
        .await
    }

    pub async fn clear_book_subtitle(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_subtitle(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_authors(
        &self,
        book_id: &str,
        authors: Vec<String>,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_authors(book_id),
            Some(&ChangeBookAuthorsRequest { authors }),
        )
        .await
    }

    pub async fn clear_book_authors(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_authors(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_publisher(
        &self,
        book_id: &str,
        publisher: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_publisher(book_id),
            Some(&ChangeBookPublisherRequest { publisher }),
        )
        .await
    }

    pub async fn clear_book_publisher(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_publisher(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_published_date(
        &self,
        book_id: &str,
        published_date: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_published_date(book_id),
            Some(&ChangeBookPublishedDateRequest { published_date }),
        )
        .await
    }

    pub async fn clear_book_published_date(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_published_date(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_language(
        &self,
        book_id: &str,
        language: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_language(book_id),
            Some(&ChangeBookLanguageRequest { language }),
        )
        .await
    }

    pub async fn clear_book_language(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_language(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_description(
        &self,
        book_id: &str,
        description: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_description(book_id),
            Some(&ChangeBookDescriptionRequest { description }),
        )
        .await
    }

    pub async fn clear_book_description(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_description(book_id),
            None,
        )
        .await
    }

    pub async fn change_book_identifier(
        &self,
        book_id: &str,
        identifier: String,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book_metadata_identifier(book_id),
            Some(&ChangeBookIdentifierRequest { identifier }),
        )
        .await
    }

    pub async fn clear_book_identifier(&self, book_id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(
            Method::DELETE,
            &paths::book_metadata_identifier(book_id),
            None,
        )
        .await
    }

    pub async fn create_book(
        &self,
        title: String,
        hash: String,
    ) -> Result<CreateBookOutput, ErrorCode> {
        self.send_authorized_json(
            Method::POST,
            paths::BOOKS,
            Some(&CreateBookRequest { title, hash }),
        )
        .await
    }

    pub async fn rename_book(&self, id: String, title: String) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::book(&id),
            Some(&RenameBookRequest { title }),
        )
        .await
    }

    pub async fn delete_books(&self, ids: Vec<String>) -> Result<(), ErrorCode> {
        self.send_authorized_empty(Method::DELETE, paths::BOOKS, Some(&BookIdsRequest { ids }))
            .await
    }

    pub async fn list_trash(&self) -> Result<Vec<TrashedBookOutput>, ErrorCode> {
        self.send_authorized_json::<(), Vec<TrashedBookOutput>>(Method::GET, paths::TRASH, None)
            .await
    }

    pub async fn restore_book(&self, id: &str) -> Result<(), ErrorCode> {
        self.send_authorized_empty::<()>(Method::POST, &paths::trash_restore(id), None)
            .await
    }

    pub async fn permanently_delete_books(&self, ids: Vec<String>) -> Result<(), ErrorCode> {
        self.send_authorized_empty(Method::DELETE, paths::TRASH, Some(&BookIdsRequest { ids }))
            .await
    }

    pub async fn get_cover_stream(
        &self,
        book_id: &str,
    ) -> Result<(CoverByteStream, String), ErrorCode> {
        let resp = self
            .send_authorized_replayable(
                Method::GET,
                &paths::book_cover(book_id),
                None::<&()>,
                false,
            )
            .await?;
        let mime = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let stream = resp
            .bytes_stream()
            .map_err(|err| io::Error::other(err.to_string()))
            .boxed();
        Ok((stream, mime))
    }

    pub async fn head_book_file(&self, book_id: &str) -> Result<RemoteBookFileMeta, ErrorCode> {
        let resp = self
            .send_authorized_replayable_with_headers(
                Method::HEAD,
                &paths::book_file(book_id),
                None::<&()>,
                &[],
                true,
            )
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(ErrorCode::CloudInvalidResponse);
        }

        // reqwest 0.13 `Response::content_length()` returns `Some(0)` for bodyless HEAD responses,
        // so parse the raw Content-Length header directly.
        let size = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .ok_or(ErrorCode::CloudInvalidResponse)?;

        let hash = resp
            .headers()
            .get("x-book-hash")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| blake3::Hash::from_hex(s.trim()).ok())
            .ok_or(ErrorCode::CloudInvalidResponse)?;

        let file_type = match resp
            .headers()
            .get("x-book-type")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
        {
            Some("EPUB") => BookFileType::EPUB,
            Some("PDF") => BookFileType::PDF,
            _ => return Err(ErrorCode::CloudInvalidResponse),
        };

        Ok(RemoteBookFileMeta {
            hash,
            file_type,
            size,
        })
    }

    /// Fetches exact closed byte slice `Range: bytes=start-end` with strict anti-downgrade and
    /// hash/range integrity checks.
    pub async fn get_book_file_range(
        &self,
        book_id: &str,
        expected_hash: &blake3::Hash,
        range: ByteRange,
    ) -> Result<Bytes, ErrorCode> {
        let range_header = format!("bytes={}-{}", range.start(), range.end_inclusive());
        let resp = self
            .send_authorized_replayable_with_headers(
                Method::GET,
                &paths::book_file(book_id),
                None::<&()>,
                &[("range", &range_header)],
                true,
            )
            .await?;

        if resp.status() != StatusCode::PARTIAL_CONTENT {
            return Err(ErrorCode::CloudInvalidResponse);
        }

        let mut hash_verified = false;
        if let Some(raw_etag) = resp.headers().get(reqwest::header::ETAG) {
            let etag_str = raw_etag
                .to_str()
                .map_err(|_| ErrorCode::CloudInvalidResponse)?
                .trim();
            let hex = etag_str
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or(ErrorCode::CloudInvalidResponse)?;
            let parsed =
                blake3::Hash::from_hex(hex).map_err(|_| ErrorCode::CloudInvalidResponse)?;
            if parsed != *expected_hash {
                return Err(ErrorCode::CloudInvalidResponse);
            }
            hash_verified = true;
        }

        if let Some(raw_hash) = resp.headers().get("x-book-hash") {
            let hash_str = raw_hash
                .to_str()
                .map_err(|_| ErrorCode::CloudInvalidResponse)?
                .trim();
            let parsed =
                blake3::Hash::from_hex(hash_str).map_err(|_| ErrorCode::CloudInvalidResponse)?;
            if parsed != *expected_hash {
                return Err(ErrorCode::CloudInvalidResponse);
            }
            hash_verified = true;
        }

        if !hash_verified {
            return Err(ErrorCode::CloudInvalidResponse);
        }

        let content_range = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .ok_or(ErrorCode::CloudInvalidResponse)?;
        let (cr_start, cr_end, cr_total) =
            parse_content_range_header(content_range).ok_or(ErrorCode::CloudInvalidResponse)?;
        if cr_start != range.start() || cr_end != range.end_inclusive() || cr_end >= cr_total {
            return Err(ErrorCode::CloudInvalidResponse);
        }

        let bytes = resp.bytes().await.map_err(classify_transport_error)?;
        if bytes.len() != range.len() {
            return Err(ErrorCode::CloudInvalidResponse);
        }
        Ok(bytes)
    }

    // -------------------------------------------------------------------------
    // Tag Tree & Book-Tag Bindings
    // -------------------------------------------------------------------------

    pub async fn get_tag_tree(&self) -> Result<Vec<TagTreeNodeOutput>, ErrorCode> {
        self.send_authorized_json::<(), Vec<TagTreeNodeOutput>>(Method::GET, paths::TAGS, None)
            .await
    }

    pub async fn define_tag(
        &self,
        name: String,
        parent_id: Option<String>,
        color: String,
        allows_value: bool,
        value_regex: Option<String>,
    ) -> Result<TagDefinitionOutput, ErrorCode> {
        self.send_authorized_json(
            Method::POST,
            paths::TAGS,
            Some(&DefineTagRequest {
                name,
                parent_id,
                color,
                allows_value,
                value_regex,
            }),
        )
        .await
    }

    pub async fn rename_tag(&self, id: String, name: String) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::tag_name(&id),
            Some(&RenameTagRequest { name }),
        )
        .await
    }

    pub async fn change_tag_color(&self, id: String, color: String) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::tag_color(&id),
            Some(&ChangeTagColorRequest { color }),
        )
        .await
    }

    pub async fn move_tag(&self, id: String, parent_id: Option<String>) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::PATCH,
            &paths::tag_parent(&id),
            Some(&MoveTagRequest { parent_id }),
        )
        .await
    }

    pub async fn undefine_tag(
        &self,
        id: String,
        strategy: DeleteTagStrategyDto,
    ) -> Result<(), ErrorCode> {
        let contract_strategy = match strategy {
            DeleteTagStrategyDto::Cascade => DeleteTagStrategy::Cascade,
            DeleteTagStrategyDto::Promote => DeleteTagStrategy::Promote,
        };
        let path = format!(
            "{}?strategy={}",
            paths::tag(&id),
            contract_strategy.as_query_str()
        );
        self.send_authorized_empty::<()>(Method::DELETE, &path, None)
            .await
    }

    pub async fn get_book_tags(&self, book_id: &str) -> Result<Vec<BookTagOutput>, ErrorCode> {
        self.send_authorized_json::<(), Vec<BookTagOutput>>(
            Method::GET,
            &paths::book_tags(book_id),
            None,
        )
        .await
    }

    pub async fn bind_book_tag(
        &self,
        book_id: &str,
        tag_id: String,
        value: Option<String>,
    ) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::POST,
            &paths::book_tags(book_id),
            Some(&BindBookTagRequest { tag_id, value }),
        )
        .await
    }

    pub async fn unbind_book_tag(&self, book_id: &str, tag_id: String) -> Result<(), ErrorCode> {
        self.send_authorized_empty(
            Method::DELETE,
            &paths::book_tags(book_id),
            Some(&UnbindBookTagRequest { tag_id }),
        )
        .await
    }

    // -------------------------------------------------------------------------
    // Files: Hash Probe & Streaming Upload
    // -------------------------------------------------------------------------

    /// Probes whether a file hash already exists on the server.
    /// Returns `Ok(false)` on 404 (expected dedup miss, NOT an error).
    pub async fn file_exists(&self, hash: &str) -> Result<bool, ErrorCode> {
        let path = format!("{}?hash={hash}", paths::BOOK_FILES);
        match self
            .send_authorized_replayable(Method::GET, &path, None::<&()>, true)
            .await
        {
            Ok(_) => Ok(true),
            Err(ErrorCode::BookFileNotFound) => Ok(false),
            Err(code) => Err(code),
        }
    }

    /// Streams an arbitrary async reader (desktop file or Android SAF URI) into the cloud library:
    /// 1. Single-pass read into bounded temp file while computing BLAKE3 and enforcing the server's
    ///    advertised `max_upload_bytes` (defaulting to 512 MiB before probe).
    /// 2. Dedup hash probe (`GET /api/v1/library/files/book?hash=...`), which also refreshes token if needed.
    /// 3. Streaming multipart upload (`POST /api/v1/library/files/book`) if missing — NEVER blindly replayed.
    /// 4. Creates bookshelf record (`POST /api/v1/library/bookshelf/books`).
    pub async fn import_reader_into_cloud<R>(
        &self,
        file_name: &str,
        reader: R,
        cancel: Option<&CancellationToken>,
        mut on_progress: Option<ImportProgressCallback<'_>>,
    ) -> Result<GetBookOutput, ErrorCode>
    where
        R: AsyncRead + Unpin + Send,
    {
        let uncancelled = CancellationToken::new();
        let cancel = cancel.unwrap_or(&uncancelled);
        let _ = derive_book_title(file_name)?;
        let max_bytes = self.max_upload_bytes();
        let limit_error = staging_limit_error(max_bytes);

        let track_progress = on_progress.is_some();
        let mut last_reported = 0u64;
        let mut staged = {
            let mut staging_cb = |staged_bytes: u64| {
                let half = staged_bytes / 2;
                if half > last_reported {
                    last_reported = half;
                }
                if let Some(cb) = on_progress.as_deref_mut() {
                    cb(last_reported);
                }
            };
            stage_book(
                &self.temp_dir,
                reader,
                Some((max_bytes, limit_error)),
                cancel,
                track_progress.then_some(&mut staging_cb as ImportProgressCallback<'_>),
            )
            .await?
        };
        let title = resolve_staged_book_title(file_name, &mut staged).await?;
        let StagedBook { file, hash, size } = staged;
        let staged_base = size / 2;
        let upload_budget = size.saturating_sub(staged_base);

        let transfer = async {
            if !self.file_exists(&hash).await? {
                let mut upload_cb = |uploaded_bytes: u64| {
                    let portion = if size == 0 {
                        0
                    } else {
                        ((uploaded_bytes.min(size) as u128 * upload_budget as u128) / size as u128)
                            as u64
                    };
                    let total_reported = (staged_base + portion).min(size);
                    if total_reported >= last_reported {
                        last_reported = total_reported;
                        if let Some(cb) = on_progress.as_deref_mut() {
                            cb(last_reported);
                        }
                    }
                };
                self.upload_staged_book_file(
                    &hash,
                    file_name,
                    file,
                    size,
                    cancel,
                    track_progress.then_some(&mut upload_cb as ImportProgressCallback<'_>),
                )
                .await?;
            }
            if last_reported < size {
                last_reported = size;
                if let Some(cb) = on_progress.as_deref_mut() {
                    cb(size);
                }
            }
            Ok::<(), ErrorCode>(())
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
            result = transfer => result?,
        }

        // Once record creation starts, wait for its outcome so a committed book is reported.
        Ok(self.create_book(title, hash).await?.into())
    }

    /// Streams a locally staged book file to the server. If the access token expired before or
    /// during upload (HTTP 401), performs a single-flight token refresh and rewinds the local
    /// staged temp file at most once without ever re-reading the caller's source stream.
    pub(crate) async fn upload_staged_book_file(
        &self,
        hash_hex: &str,
        file_name: &str,
        mut file: File,
        file_len: u64,
        cancel: &CancellationToken,
        mut on_progress: Option<ImportProgressCallback<'_>>,
    ) -> Result<(), ErrorCode> {
        let mut retried = false;
        loop {
            if cancel.is_cancelled() {
                return Err(ErrorCode::ImportCancelled);
            }
            let (access_token, failed_epoch) = self.current_access_token_and_epoch()?;
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|_| ErrorCode::CommonInternalServerError)?;
            let attempt_file = file
                .try_clone()
                .await
                .map_err(|_| ErrorCode::CommonInternalServerError)?;

            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
            let reader_stream = ReaderStream::with_capacity(attempt_file, STREAM_CHUNK_BYTES);
            let mut uploaded: u64 = 0;
            let tracked_stream = reader_stream.map(move |chunk| {
                if let Ok(ref bytes) = chunk {
                    uploaded = uploaded.saturating_add(bytes.len() as u64);
                    let _ = progress_tx.send(uploaded);
                }
                chunk
            });
            let body = ReqwestBody::wrap_stream(tracked_stream);
            let file_part = Part::stream_with_length(body, file_len)
                .file_name(file_name.to_owned())
                .mime_str("application/octet-stream")
                .map_err(|_| ErrorCode::CommonInternalServerError)?;
            let form = Form::new()
                .text(shiro_http_contract::BOOK_FILE_FIELD_HASH, hash_hex.to_owned())
                .text(shiro_http_contract::BOOK_FILE_FIELD_SIZE, file_len.to_string())
                .part(shiro_http_contract::BOOK_FILE_FIELD_FILE, file_part);

            // No 30s hard timeout on streaming upload so slow-but-active large files complete cleanly.
            let send_fut = self
                .http
                .post(self.url(paths::BOOK_FILES))
                .header(AUTHORIZATION, format!("Bearer {access_token}"))
                .multipart(form)
                .send();
            tokio::pin!(send_fut);

            let mut progress_open = true;
            let resp = loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
                    resp = &mut send_fut => {
                        while let Ok(sent) = progress_rx.try_recv() {
                            if let Some(cb) = on_progress.as_deref_mut() {
                                cb(sent);
                            }
                        }
                        break resp.map_err(classify_transport_error)?;
                    }
                    maybe_sent = progress_rx.recv(), if progress_open => {
                        match maybe_sent {
                            Some(sent) => {
                                if let Some(cb) = on_progress.as_deref_mut() {
                                    cb(sent);
                                }
                            }
                            None => progress_open = false,
                        }
                    }
                }
            };

            if resp.status().is_redirection() {
                return Err(ErrorCode::CloudCrossOriginRedirect);
            }

            if resp.status().is_success() {
                return Ok(());
            }

            if resp.status() == StatusCode::UNAUTHORIZED {
                let code = parse_server_error(resp).await;
                if !retried && is_token_refresh_trigger(code) {
                    if cancel.is_cancelled() {
                        return Err(ErrorCode::ImportCancelled);
                    }
                    retried = true;
                    let refresh_res = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
                        res = self.refresh_session_single_flight(failed_epoch) => res,
                    };
                    match refresh_res {
                        Ok(_) => continue,
                        Err(RefreshError::Revoked(revoked_code)) => return Err(revoked_code),
                        Err(RefreshError::Transient(net_code)) => return Err(net_code),
                    }
                }
                if is_session_invalid_code(code) {
                    self.invalidate_session_if_epoch_matches(failed_epoch).await;
                }
                return Err(code);
            }

            let code = parse_server_error(resp).await;
            if code == ErrorCode::BookFileAlreadyExists {
                // Expected dedup race on concurrent upload.
                return Ok(());
            }
            return Err(code);
        }
    }

    // -------------------------------------------------------------------------
    // Authenticated Request Helpers & Single-Flight Refresh
    // -------------------------------------------------------------------------

    async fn send_authorized_json<Req, Resp>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
    ) -> Result<Resp, ErrorCode>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
    {
        let resp = self
            .send_authorized_replayable(method, path, body, true)
            .await?;
        resp.json::<Resp>()
            .await
            .map_err(|_| ErrorCode::CloudInvalidResponse)
    }

    async fn send_authorized_empty<Req>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
    ) -> Result<(), ErrorCode>
    where
        Req: Serialize + ?Sized,
    {
        let _ = self
            .send_authorized_replayable(method, path, body, true)
            .await?;
        Ok(())
    }

    /// Sends a replayable authorized request. On HTTP 401 caused by an expired/invalid access
    /// token, performs single-flight refresh and retries the request AT MOST ONCE.
    async fn send_authorized_replayable<Req>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
        apply_standard_timeout: bool,
    ) -> Result<Response, ErrorCode>
    where
        Req: Serialize + ?Sized,
    {
        self.send_authorized_replayable_with_headers(
            method,
            path,
            body,
            &[],
            apply_standard_timeout,
        )
        .await
    }

    async fn send_authorized_replayable_with_headers<Req>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
        extra_headers: &[(&str, &str)],
        apply_standard_timeout: bool,
    ) -> Result<Response, ErrorCode>
    where
        Req: Serialize + ?Sized,
    {
        let serialized_body: Option<Bytes> = match body {
            Some(payload) => Some(Bytes::from(
                serde_json::to_vec(payload).map_err(|_| ErrorCode::CommonInternalServerError)?,
            )),
            None => None,
        };

        let mut retried = false;
        loop {
            let (access_token, attempt_epoch) = self.current_access_token_and_epoch()?;
            let mut builder = self
                .http
                .request(method.clone(), self.url(path))
                .header(AUTHORIZATION, format!("Bearer {access_token}"));
            for &(name, value) in extra_headers {
                builder = builder.header(name, value);
            }
            if apply_standard_timeout {
                builder = builder.timeout(STANDARD_REQUEST_TIMEOUT);
            }
            if let Some(ref bytes) = serialized_body {
                builder = builder
                    .header(CONTENT_TYPE, "application/json")
                    .body(bytes.clone());
            }

            let resp = builder.send().await.map_err(classify_transport_error)?;
            if resp.status().is_redirection() {
                return Err(ErrorCode::CloudCrossOriginRedirect);
            }
            if resp.status().is_success() {
                return Ok(resp);
            }

            if resp.status() == StatusCode::UNAUTHORIZED {
                let code = parse_server_error_for_method(&method, resp).await;
                if !retried && is_token_refresh_trigger(code) {
                    retried = true;
                    match self.refresh_session_single_flight(attempt_epoch).await {
                        Ok(()) => continue,
                        Err(RefreshError::Revoked(revoked)) => return Err(revoked),
                        Err(RefreshError::Transient(net_err)) => return Err(net_err),
                    }
                }
                if is_session_invalid_code(code) {
                    self.invalidate_session_if_epoch_matches(attempt_epoch)
                        .await;
                }
                return Err(code);
            }

            let code = parse_server_error_for_method(&method, resp).await;
            if code == ErrorCode::AuthenticationUserNotFound {
                self.invalidate_session_if_epoch_matches(attempt_epoch)
                    .await;
            }
            return Err(code);
        }
    }

    async fn refresh_session_single_flight(&self, failed_epoch: u64) -> Result<(), RefreshError> {
        let _guard = self.refresh_gate.lock().await;

        // If another concurrent request already refreshed the session while we waited,
        // skip the duplicate HTTP refresh call and immediately reuse the fresh token.
        if self.refresh_epoch.load(Ordering::Acquire) != failed_epoch {
            if self.has_saved_session() {
                debug!("coalesced concurrent cloud token refresh");
                return Ok(());
            }
            return Err(RefreshError::Revoked(ErrorCode::AuthenticationInvalidToken));
        }

        let refresh_token = {
            let guard = self.session.read().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(creds) => creds.refresh_token.clone(),
                None => {
                    return Err(RefreshError::Revoked(ErrorCode::CloudNotAuthenticated));
                }
            }
        };

        let resp = match self
            .http
            .post(self.url(paths::AUTH_REFRESH))
            .timeout(STANDARD_REQUEST_TIMEOUT)
            .json(&RefreshAccessTokenRequest {
                token: refresh_token,
            })
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                // Transient network error: preserve recoverable session!
                return Err(RefreshError::Transient(classify_transport_error(err)));
            }
        };

        if resp.status().is_redirection() {
            return Err(RefreshError::Transient(ErrorCode::CloudCrossOriginRedirect));
        }

        if resp.status().is_success() {
            let payload: RefreshAccessTokenOutput = resp
                .json()
                .await
                .map_err(|_| RefreshError::Transient(ErrorCode::CloudInvalidResponse))?;

            let updated_creds = {
                let mut guard = self.session.write().unwrap_or_else(|e| e.into_inner());
                let Some(creds) = guard.as_mut() else {
                    return Err(RefreshError::Revoked(ErrorCode::CloudNotAuthenticated));
                };
                creds.access_token = payload.access_token;
                creds.clone()
            };

            if let Err(err) = self.credential_store.save(&self.origin, &updated_creds) {
                warn!(error = %err, "failed to persist refreshed cloud access token");
            }
            self.refresh_epoch.fetch_add(1, Ordering::AcqRel);
            return Ok(());
        }

        let status = resp.status();
        let code = parse_server_error(resp).await;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND {
            // Server explicitly confirmed the session/user is invalid -> wipe local credentials!
            self.clear_session_and_notify();
            return Err(RefreshError::Revoked(code));
        }

        // Server 5xx or transient non-auth error: preserve session for retry.
        Err(RefreshError::Transient(code))
    }

    async fn invalidate_session_if_epoch_matches(&self, failed_epoch: u64) {
        let _guard = self.refresh_gate.lock().await;
        if self.refresh_epoch.load(Ordering::Acquire) != failed_epoch {
            return;
        }
        self.clear_session_and_notify();
    }

    fn clear_session_and_notify(&self) {
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = self.credential_store.clear(&self.origin);
        self.refresh_epoch.fetch_add(1, Ordering::AcqRel);
        if let Some(cb) = &self.on_session_invalidated {
            cb();
        }
    }

    fn current_access_token_and_epoch(&self) -> Result<(String, u64), ErrorCode> {
        let epoch = self.refresh_epoch.load(Ordering::Acquire);
        let guard = self.session.read().unwrap_or_else(|e| e.into_inner());
        let Some(creds) = guard.as_ref() else {
            return Err(ErrorCode::CloudNotAuthenticated);
        };
        Ok((creds.access_token.clone(), epoch))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.origin, path)
    }
}

fn build_http_client(origin_key: &OriginKey) -> anyhow::Result<Client> {
    let expected_origin = origin_key.clone();
    let redirect_policy = RedirectPolicy::custom(move |attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many redirects");
        }
        let target_origin = OriginKey::from_url(attempt.url());
        if target_origin.as_ref() != Some(&expected_origin) {
            // Strictly block cross-origin redirects (including scheme/host/port changes) so
            // Authorization headers are NEVER forwarded to another origin.
            return attempt.error(CrossOriginRedirectError);
        }
        attempt.follow()
    });

    Ok(Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(redirect_policy)
        .tls_backend_rustls()
        .build()?)
}

fn is_cross_origin_redirect(err: &reqwest::Error) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(source) = current {
        if source.is::<CrossOriginRedirectError>() {
            return true;
        }
        current = source.source();
    }
    false
}

fn classify_transport_error(err: reqwest::Error) -> ErrorCode {
    if is_cross_origin_redirect(&err) {
        return ErrorCode::CloudCrossOriginRedirect;
    }
    if err.is_timeout() {
        return ErrorCode::CloudRequestTimeout;
    }
    ErrorCode::CloudNetworkUnreachable
}

async fn parse_server_error(resp: Response) -> ErrorCode {
    parse_server_error_for_method(&Method::GET, resp).await
}

async fn parse_server_error_for_method(method: &Method, resp: Response) -> ErrorCode {
    let status = resp.status();
    if let Ok(body) = resp.json::<ErrorResponse>().await {
        if let Some(code) = ErrorCode::from_code(body.error.code) {
            return code;
        }
        if let Some(code) = ErrorCode::from_key(&body.error.message) {
            return code;
        }
    }
    match status {
        StatusCode::UNAUTHORIZED => ErrorCode::AuthenticationInvalidToken,
        StatusCode::PAYLOAD_TOO_LARGE => ErrorCode::CommonPayloadTooLarge,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => ErrorCode::CloudRequestTimeout,
        StatusCode::NOT_FOUND if *method == Method::HEAD => ErrorCode::BookNotFound,
        StatusCode::CONFLICT if *method == Method::HEAD => ErrorCode::BookAlreadyDeleted,
        StatusCode::BAD_REQUEST if *method == Method::HEAD => ErrorCode::BookInvalidId,
        s if s.is_server_error() => ErrorCode::CommonInternalServerError,
        _ => ErrorCode::CloudInvalidResponse,
    }
}

fn parse_content_range_header(raw: &str) -> Option<(u64, u64, u64)> {
    let body = raw.trim().strip_prefix("bytes ")?;
    let (span, total_str) = body.split_once('/')?;
    let (start_str, end_str) = span.split_once('-')?;
    if start_str.is_empty()
        || end_str.is_empty()
        || total_str.is_empty()
        || !start_str.bytes().all(|b| b.is_ascii_digit())
        || !end_str.bytes().all(|b| b.is_ascii_digit())
        || !total_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let start = start_str.parse::<u64>().ok()?;
    let end = end_str.parse::<u64>().ok()?;
    let total = total_str.parse::<u64>().ok()?;
    (start <= end && end < total).then_some((start, end, total))
}

fn is_token_refresh_trigger(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::AuthenticationInvalidToken
            | ErrorCode::AuthenticationInvalidTokenType
            | ErrorCode::AuthenticationMissingBearerToken
    )
}

pub fn is_session_invalid_code(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::AuthenticationInvalidToken
            | ErrorCode::AuthenticationInvalidTokenType
            | ErrorCode::AuthenticationMissingBearerToken
            | ErrorCode::AuthenticationUserNotFound
            | ErrorCode::CloudNotAuthenticated
    )
}

pub(crate) fn staging_limit_error(max_bytes: u64) -> ErrorCode {
    if max_bytes == DEFAULT_CLOUD_MAX_UPLOAD_BYTES {
        ErrorCode::ImportFileTooLarge
    } else {
        ErrorCode::CommonPayloadTooLarge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_limit_error_matches_default_or_server_limit() {
        assert_eq!(
            staging_limit_error(DEFAULT_CLOUD_MAX_UPLOAD_BYTES),
            ErrorCode::ImportFileTooLarge
        );
        assert_eq!(
            staging_limit_error(128),
            ErrorCode::CommonPayloadTooLarge
        );
    }
}
