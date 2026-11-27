import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { notifyLocalError, type RustErrorEvent } from "@shiro/ui-shared/lib/error-notifications"
import type { Locale } from "@shiro/ui-shared/lib/i18n"
import type { AppTheme } from "@shiro/ui-shared/lib/theme"
import type {
  Book,
  BookMetadata,
  BookTag,
  Tag,
  TagDefinitionInput,
  TagTreeNode,
  TrashedBook,
  UndefineTagStrategy,
} from "@shiro/ui-shared/lib/types"

export const RUST_ERROR_EVENT = "shiro://error"
export const RUST_RUNTIME_STATE_EVENT = "shiro://runtime-state"
export const RUST_UPLOAD_STATE_EVENT = "shiro://upload-state"
export const RUST_UPLOAD_OPEN_UI_EVENT = "shiro://upload-open-ui"

export type LaunchMode = "local" | "cloud"
export type StartupPhase =
  | "setup_required"
  | "ready"
  | "startup_failed"
  | "cloud_protocol_incompatible"
  | "cloud_unreachable"
  | "cloud_auth_required"

export interface CommandErrorPayload {
  operationId: string
  errorId: string
  code: number
  key: string
  itemId: string | null
}

export interface StartupDiagnostic {
  errorId: string
  code: number
  key: string
  message: string
  detail: string | null
  path: string | null
}

export interface CloudUserInfo {
  userId: string
  username: string
}

export interface CloudOriginCheckResult {
  ok: boolean
  message: string | null
}

export interface CloudStatus {
  origin: string
  user: CloudUserInfo | null
  hasSavedSession: boolean
  protocolVersion: number | null
  diagnostic: StartupDiagnostic | null
}

export interface BootstrapState {
  phase: StartupPhase
  activeMode: LaunchMode | null
  configuredMode: LaunchMode | null
  pendingRestart: boolean
  locale: Locale
  theme: AppTheme
  configuredLibraryRoot: string
  defaultLibraryRoot: string
  cloudOrigin: string | null
  canPickLibraryDir: boolean
  canImportBooks: boolean
  coverUrlBase: string
  startupError: StartupDiagnostic | null
  cloud: CloudStatus | null
  bufferedErrors: RustErrorEvent[]
}

export interface InitialSetupRequest {
  mode: LaunchMode
  locale: Locale
  localLibraryRoot: string | null
  cloudOrigin: string | null
}

export interface UpdateSettingsRequest {
  mode: LaunchMode | null
  locale: Locale | null
  localLibraryRoot: string | null
  cloudOrigin: string | null
}

export interface RestoreBooksItemResult {
  bookId: string
  restored: boolean
  error: CommandErrorPayload | null
}

export interface RestoreBooksResult {
  operationId: string
  restoredIds: string[]
  failed: RestoreBooksItemResult[]
  skippedIds: string[]
}

export type UploadActiveStage = "staging" | "uploading" | "finalizing"

export type UploadTaskStatus =
  | { state: "queued" }
  | {
      state: "active"
      stage: UploadActiveStage
      progressPercent: number
      bytesProcessed: number
      totalBytes: number | null
      speedBytesPerSec: number | null
      etaSeconds: number | null
    }
  | {
      state: "succeeded"
      bookId: string
      title: string
    }
  | {
      state: "failed"
      error: RustErrorEvent
    }

export interface UploadTask {
  id: number
  fileName: string
  status: UploadTaskStatus
}

export interface UploadSessionStats {
  total: number
  remaining: number
  active: number
  succeeded: number
  failed: number
}

export interface UploadPanelSnapshot {
  revision: number
  visible: boolean
  concurrency: number
  completedCounter: number
  stats: UploadSessionStats
  hasDismissibleErrors: boolean
  tasks: UploadTask[]
}

let activeCoverUrlBase = "shiro-cover://localhost/book/"

export function isTauriRuntime(): boolean {
  return isTauri()
}

export function setCoverUrlBase(urlBase: string) {
  if (urlBase.trim().length > 0) {
    activeCoverUrlBase = urlBase.endsWith("/") ? urlBase : `${urlBase}/`
  }
}

export function getBookCoverProtocolUrl(bookId: string): string {
  return `${activeCoverUrlBase}${encodeURIComponent(bookId)}`
}

export function isCommandErrorPayload(
  value: unknown
): value is CommandErrorPayload {
  if (typeof value !== "object" || value === null) return false
  const record = value as Record<string, unknown>
  return (
    typeof record["operationId"] === "string" &&
    typeof record["errorId"] === "string" &&
    typeof record["code"] === "number" &&
    typeof record["key"] === "string" &&
    (record["itemId"] === null || typeof record["itemId"] === "string")
  )
}

export class CommandError extends Error {
  readonly payload: CommandErrorPayload

  constructor(payload: CommandErrorPayload) {
    super(payload.key)
    this.name = "CommandError"
    this.payload = payload
  }
}

async function invokeTauri<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    if (isCommandErrorPayload(error)) {
      throw new CommandError(error)
    }
    notifyLocalError("local.common.error.unknown")
    throw error
  }
}

export function listenRustErrors(
  handler: (event: RustErrorEvent) => void
): Promise<UnlistenFn> {
  return listen<RustErrorEvent>(RUST_ERROR_EVENT, (event) => {
    handler(event.payload)
  })
}

export function listenRuntimeState(
  handler: (state: BootstrapState) => void
): Promise<UnlistenFn> {
  return listen<BootstrapState>(RUST_RUNTIME_STATE_EVENT, (event) => {
    handler(event.payload)
  })
}

export function listenUploadState(
  handler: (snapshot: UploadPanelSnapshot) => void
): Promise<UnlistenFn> {
  return listen<UploadPanelSnapshot>(RUST_UPLOAD_STATE_EVENT, (event) => {
    handler(event.payload)
  })
}

export function listenUploadOpenUi(handler: () => void): Promise<UnlistenFn> {
  return listen<void>(RUST_UPLOAD_OPEN_UI_EVENT, () => {
    handler()
  })
}

export const tauriBridge = {
  // Lifecycle, settings & platform
  getBootstrapState: () => invoke<BootstrapState>("get_bootstrap_state"),
  completeInitialSetup: (request: InitialSetupRequest) =>
    invokeTauri<BootstrapState>("complete_initial_setup", { request }),
  updateSettings: (request: UpdateSettingsRequest) =>
    invokeTauri<BootstrapState>("update_settings", { request }),
  setAppLocale: (locale: Locale) =>
    invokeTauri<BootstrapState>("set_app_locale", { locale }),
  setAppTheme: (theme: AppTheme) =>
    invokeTauri<BootstrapState>("set_app_theme", { theme }),
  pickLibraryDir: () => invokeTauri<string | null>("pick_library_dir"),
  repairLocalLibraryRoot: (path: string) =>
    invokeTauri<BootstrapState>("repair_local_library_root", { path }),
  retryStartup: () => invokeTauri<BootstrapState>("retry_startup"),
  exitApp: () => invokeTauri<void>("exit_app"),
  triggerLongPressHaptic: () => invoke<void>("trigger_long_press_haptic"),

  // Cloud mode connection & authentication (tokens stay strictly in Rust)
  cloudCheckOrigin: (origin: string) =>
    invokeTauri<CloudOriginCheckResult>("cloud_check_origin", { origin }),
  cloudProbe: () => invokeTauri<BootstrapState>("cloud_probe"),
  cloudUpdateOrigin: (origin: string) =>
    invokeTauri<BootstrapState>("cloud_update_origin", { origin }),
  cloudLogin: (username: string, password: string) =>
    invokeTauri<BootstrapState>("cloud_login", { username, password }),
  cloudLogout: () => invokeTauri<BootstrapState>("cloud_logout"),

  // Bookshelf & trash
  booksList: () => invokeTauri<Book[]>("books_list"),
  bookRename: (id: string, title: string) =>
    invokeTauri<void>("book_rename", { id, title }),
  bookMetadataGet: (id: string) =>
    invokeTauri<BookMetadata>("book_metadata_get", { id }),
  bookSubtitleChange: (id: string, subtitle: string) =>
    invokeTauri<void>("book_subtitle_change", { id, subtitle }),
  bookSubtitleClear: (id: string) =>
    invokeTauri<void>("book_subtitle_clear", { id }),
  bookAuthorsChange: (id: string, authors: string[]) =>
    invokeTauri<void>("book_authors_change", { id, authors }),
  bookAuthorsClear: (id: string) =>
    invokeTauri<void>("book_authors_clear", { id }),
  bookPublisherChange: (id: string, publisher: string) =>
    invokeTauri<void>("book_publisher_change", { id, publisher }),
  bookPublisherClear: (id: string) =>
    invokeTauri<void>("book_publisher_clear", { id }),
  bookPublishedDateChange: (id: string, publishedDate: string) =>
    invokeTauri<void>("book_published_date_change", { id, publishedDate }),
  bookPublishedDateClear: (id: string) =>
    invokeTauri<void>("book_published_date_clear", { id }),
  bookLanguageChange: (id: string, language: string) =>
    invokeTauri<void>("book_language_change", { id, language }),
  bookLanguageClear: (id: string) =>
    invokeTauri<void>("book_language_clear", { id }),
  bookDescriptionChange: (id: string, description: string) =>
    invokeTauri<void>("book_description_change", { id, description }),
  bookDescriptionClear: (id: string) =>
    invokeTauri<void>("book_description_clear", { id }),
  bookIdentifierChange: (id: string, identifier: string) =>
    invokeTauri<void>("book_identifier_change", { id, identifier }),
  bookIdentifierClear: (id: string) =>
    invokeTauri<void>("book_identifier_clear", { id }),
  booksDelete: (ids: string[]) => invokeTauri<void>("books_delete", { ids }),
  trashList: () => invokeTauri<TrashedBook[]>("trash_list"),
  trashRestore: (ids: string[]) =>
    invokeTauri<RestoreBooksResult>("trash_restore", { ids }),
  trashPermanentlyDelete: (ids: string[]) =>
    invokeTauri<void>("trash_permanently_delete", { ids }),

  // Tags & book-tag bindings
  tagsList: () => invokeTauri<TagTreeNode[]>("tags_list"),
  tagDefine: (definition: TagDefinitionInput) =>
    invokeTauri<Tag>("tag_define", {
      name: definition.name,
      parentId: definition.parent_id,
      color: definition.color,
      valueType: definition.value_type,
    }),
  tagRename: (id: string, name: string) =>
    invokeTauri<void>("tag_rename", { id, name }),
  tagRecolor: (id: string, color: string) =>
    invokeTauri<void>("tag_recolor", { id, color }),
  tagMove: (id: string, parentId: string | null) =>
    invokeTauri<void>("tag_move", { id, parentId }),
  tagUndefine: (id: string, strategy: UndefineTagStrategy) =>
    invokeTauri<void>("tag_undefine", { id, strategy }),
  bookTagsList: (bookId: string) =>
    invokeTauri<BookTag[]>("book_tags_list", { bookId }),
  bookTagBind: (bookId: string, binding: BookTag) =>
    invokeTauri<void>("book_tag_bind", {
      bookId,
      tagId: binding.tag_id,
      value: binding.value,
    }),
  bookTagUnbind: (bookId: string, tagId: string) =>
    invokeTauri<void>("book_tag_unbind", { bookId, tagId }),

  // Concurrent upload queue (Local & Cloud)
  listenUploadState,
  listenUploadOpenUi,
  uploadGetState: () => invokeTauri<UploadPanelSnapshot>("upload_get_state"),
  uploadPickAndEnqueue: () =>
    invokeTauri<UploadPanelSnapshot>("upload_pick_and_enqueue"),
  uploadEnqueuePaths: (paths: string[]) =>
    invokeTauri<UploadPanelSnapshot>("upload_enqueue_paths", { paths }),
  uploadRetryTask: (taskId: number) =>
    invokeTauri<UploadPanelSnapshot>("upload_retry_task", { taskId }),
  uploadCancelTask: (taskId: number) =>
    invokeTauri<UploadPanelSnapshot>("upload_cancel_task", { taskId }),
  uploadCancelAll: () => invokeTauri<UploadPanelSnapshot>("upload_cancel_all"),
  uploadDismissTask: (taskId: number) =>
    invokeTauri<UploadPanelSnapshot>("upload_dismiss_task", { taskId }),
  uploadDismissErrors: () =>
    invokeTauri<UploadPanelSnapshot>("upload_dismiss_errors"),
  uploadSetConcurrency: (concurrency: number) =>
    invokeTauri<UploadPanelSnapshot>("upload_set_concurrency", { concurrency }),
  uploadConsumePendingOpenUi: () =>
    invokeTauri<boolean>("upload_consume_pending_open_ui"),
  uploadSetMobileUiOpen: (open: boolean) =>
    invokeTauri<void>("upload_set_mobile_ui_open", { open }),
}
