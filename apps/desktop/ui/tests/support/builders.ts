import type {
  BootstrapState,
  CloudStatus,
  CommandErrorPayload,
  ImportBooksResult,
  StartupDiagnostic,
} from "@/api/tauri-bridge"
import type { RustErrorEvent } from "@/lib/error-notifications"
import type { Book, BookTag, Tag, TagTreeNode, TrashedBook } from "@/lib/types"

let idCounter = 0

export function nextId(prefix = "id"): string {
  idCounter += 1
  return `${prefix}-${idCounter}`
}

export function resetBuilderIds() {
  idCounter = 0
}

export function makeBook(overrides: Partial<Book> = {}): Book {
  const id = overrides.id ?? nextId("book")
  const timestamp = overrides.updated_at ?? "2026-01-15T10:00:00.000Z"
  return {
    id,
    title: `Book ${id}`,
    hash: `hash-${id}`,
    type: "epub",
    created_at: overrides.created_at ?? timestamp,
    updated_at: timestamp,
    ...overrides,
  }
}

export function makeTrashedBook(
  overrides: Partial<TrashedBook> = {}
): TrashedBook {
  const book = makeBook(overrides)
  return {
    ...book,
    deleted_at: overrides.deleted_at ?? "2026-01-20T12:00:00.000Z",
    ...overrides,
  }
}

export function makeTag(overrides: Partial<Tag> = {}): Tag {
  const id = overrides.id ?? nextId("tag")
  const timestamp = overrides.updated_at ?? "2026-01-15T10:00:00.000Z"
  return {
    id,
    name: `tag_${id}`,
    parent_id: null,
    color: "#75a9cc",
    allows_value: false,
    value_regex: null,
    created_at: overrides.created_at ?? timestamp,
    updated_at: timestamp,
    ...overrides,
  }
}

export function makeTagNode(overrides: Partial<TagTreeNode> = {}): TagTreeNode {
  const { children = [], ...tagOverrides } = overrides
  const tag = makeTag(tagOverrides)
  return {
    ...tag,
    children: children.map((child) => ({
      ...child,
      parent_id: child.parent_id ?? tag.id,
    })),
  }
}

export function makeBookTag(
  tagId: string,
  value: string | null = null
): BookTag {
  return { tag_id: tagId, value }
}

export function makeCommandErrorPayload(
  overrides: Partial<CommandErrorPayload> = {}
): CommandErrorPayload {
  return {
    operationId: nextId("op"),
    errorId: nextId("err"),
    code: 4000,
    key: "library.book.not_found",
    itemId: null,
    ...overrides,
  }
}

export function makeRustErrorEvent(
  overrides: Partial<RustErrorEvent> = {}
): RustErrorEvent {
  return {
    operationId: nextId("op"),
    errorId: nextId("err"),
    code: 5000,
    key: "client.runtime.error",
    locale: "en",
    message: "Simulated Rust error",
    itemId: null,
    ...overrides,
  }
}

export function makeStartupDiagnostic(
  overrides: Partial<StartupDiagnostic> = {}
): StartupDiagnostic {
  return {
    errorId: nextId("diag"),
    code: 5100,
    key: "client.local.db_corrupted",
    message: "Database file is corrupted",
    detail: "SQLITE_CORRUPT: malformed header",
    path: "/tmp/library/library.db",
    ...overrides,
  }
}

export function makeImportResult(
  overrides: Partial<ImportBooksResult> = {}
): ImportBooksResult {
  return {
    operationId: nextId("import"),
    cancelled: false,
    imported: [],
    failed: [],
    skipped: [],
    abortedReason: null,
    summaryMessage: null,
    ...overrides,
  }
}

export function makeCloudStatus(
  overrides: Partial<CloudStatus> = {}
): CloudStatus {
  return {
    origin: "https://books.example.com",
    user: null,
    hasSavedSession: false,
    protocolVersion: 1,
    diagnostic: null,
    ...overrides,
  }
}

export function makeBootstrapState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  return {
    phase: "ready",
    activeMode: "local",
    configuredMode: "local",
    pendingRestart: false,
    locale: "en",
    theme: "default",
    configuredLibraryRoot: "/tmp/shiro-library",
    defaultLibraryRoot: "/tmp/shiro-library",
    cloudOrigin: null,
    canPickLibraryDir: true,
    coverUrlBase: "shiro-cover://localhost/book/",
    startupError: null,
    cloud: null,
    bufferedErrors: [],
    ...overrides,
  }
}

export function setupRequiredState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  return makeBootstrapState({
    phase: "setup_required",
    activeMode: null,
    configuredMode: null,
    ...overrides,
  })
}

export function readyLocalState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  return makeBootstrapState({
    phase: "ready",
    activeMode: "local",
    configuredMode: "local",
    ...overrides,
  })
}

export function readyCloudState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  const origin = overrides.cloudOrigin ?? "https://books.example.com"
  return makeBootstrapState({
    phase: "ready",
    activeMode: "cloud",
    configuredMode: "cloud",
    cloudOrigin: origin,
    cloud: makeCloudStatus({
      origin,
      user: { userId: "user-1", username: "alice" },
      hasSavedSession: true,
    }),
    ...overrides,
  })
}

export function startupFailedState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  return makeBootstrapState({
    phase: "startup_failed",
    activeMode: null,
    configuredMode: "local",
    startupError: makeStartupDiagnostic(),
    ...overrides,
  })
}

export function cloudAuthRequiredState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  const origin = overrides.cloudOrigin ?? "https://books.example.com"
  return makeBootstrapState({
    phase: "cloud_auth_required",
    activeMode: "cloud",
    configuredMode: "cloud",
    cloudOrigin: origin,
    cloud: makeCloudStatus({ origin }),
    ...overrides,
  })
}

export function cloudUnreachableState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  const origin = overrides.cloudOrigin ?? "https://books.example.com"
  return makeBootstrapState({
    phase: "cloud_unreachable",
    activeMode: "cloud",
    configuredMode: "cloud",
    cloudOrigin: origin,
    cloud: makeCloudStatus({
      origin,
      protocolVersion: null,
      diagnostic: makeStartupDiagnostic({
        key: "client.cloud.unreachable",
        message: "Connection refused",
        path: null,
        detail: null,
      }),
    }),
    ...overrides,
  })
}

export function cloudIncompatibleState(
  overrides: Partial<BootstrapState> = {}
): BootstrapState {
  const origin = overrides.cloudOrigin ?? "https://books.example.com"
  return makeBootstrapState({
    phase: "cloud_protocol_incompatible",
    activeMode: "cloud",
    configuredMode: "cloud",
    cloudOrigin: origin,
    cloud: makeCloudStatus({
      origin,
      protocolVersion: 99,
      diagnostic: makeStartupDiagnostic({
        key: "client.cloud.protocol_incompatible",
        message: "Server protocol v99 is not supported",
        path: null,
        detail: "Supported: v1",
      }),
    }),
    ...overrides,
  })
}
