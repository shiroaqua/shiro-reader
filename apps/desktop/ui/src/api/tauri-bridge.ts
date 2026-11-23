import { Channel, invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { notifyLocalError, type RustErrorEvent } from "@/lib/error-notifications"
import type { Locale } from "@/lib/i18n"
import type {
  Book,
  BookTag,
  Tag,
  TagDefinitionInput,
  TagTreeNode,
  TrashedBook,
  UndefineTagStrategy,
} from "@/lib/types"

export const RUST_ERROR_EVENT = "shiro://error"
export const RUST_RUNTIME_STATE_EVENT = "shiro://runtime-state"

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
  configuredLibraryRoot: string
  defaultLibraryRoot: string
  cloudOrigin: string | null
  canPickLibraryDir: boolean
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

export type ImportProgressEvent =
  | {
      stage: "started"
      operationId: string
      total: number
    }
  | {
      stage: "itemProgress"
      operationId: string
      index: number
      total: number
      fileName: string
      bytesRead: number
      totalBytes: number | null
    }
  | {
      stage: "itemSucceeded"
      operationId: string
      index: number
      total: number
      book: Book
    }
  | {
      stage: "itemFailed"
      operationId: string
      index: number
      total: number
      fileName: string
      error: CommandErrorPayload
    }
  | {
      stage: "finished"
      operationId: string
      cancelled: boolean
      importedCount: number
      failedCount: number
      skippedCount: number
    }

export interface ImportBookItemFailure {
  fileName: string
  error: CommandErrorPayload
}

export interface ImportBooksResult {
  operationId: string
  cancelled: boolean
  imported: Book[]
  failed: ImportBookItemFailure[]
  skipped: string[]
  abortedReason: CommandErrorPayload | null
  summaryMessage: string | null
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
    typeof record.operationId === "string" &&
    typeof record.errorId === "string" &&
    typeof record.code === "number" &&
    typeof record.key === "string" &&
    (record.itemId === null || typeof record.itemId === "string")
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

export const tauriBridge = {
  // Lifecycle, settings & platform
  getBootstrapState: () => invoke<BootstrapState>("get_bootstrap_state"),
  completeInitialSetup: (request: InitialSetupRequest) =>
    invokeTauri<BootstrapState>("complete_initial_setup", { request }),
  updateSettings: (request: UpdateSettingsRequest) =>
    invokeTauri<BootstrapState>("update_settings", { request }),
  setAppLocale: (locale: Locale) =>
    invokeTauri<BootstrapState>("set_app_locale", { locale }),
  pickLibraryDir: () => invokeTauri<string | null>("pick_library_dir"),
  repairLocalLibraryRoot: (path: string) =>
    invokeTauri<BootstrapState>("repair_local_library_root", { path }),
  retryStartup: () => invokeTauri<BootstrapState>("retry_startup"),
  exitApp: () => invokeTauri<void>("exit_app"),

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
      allowsValue: definition.allows_value,
      valueRegex: definition.value_regex,
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

  // Native file picker + Rust streaming import (Local & Cloud)
  pickAndImportBooks: (onEvent: (event: ImportProgressEvent) => void) => {
    const channel = new Channel<ImportProgressEvent>()
    channel.onmessage = onEvent
    return invokeTauri<ImportBooksResult>("pick_and_import_books", {
      onEvent: channel,
    })
  },
  cancelBookImport: () => invokeTauri<boolean>("cancel_book_import"),
}
