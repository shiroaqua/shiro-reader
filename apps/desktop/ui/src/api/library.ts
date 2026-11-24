import { getBookCoverProtocolUrl, tauriBridge } from "./tauri-bridge"

export type LibraryBridge = Pick<
  typeof tauriBridge,
  | "booksList"
  | "bookRename"
  | "booksDelete"
  | "trashList"
  | "trashRestore"
  | "trashPermanentlyDelete"
  | "tagsList"
  | "tagDefine"
  | "tagRename"
  | "tagRecolor"
  | "tagMove"
  | "tagUndefine"
  | "bookTagsList"
  | "bookTagBind"
  | "bookTagUnbind"
  | "listenUploadState"
  | "listenUploadOpenUi"
  | "uploadGetState"
  | "uploadPickAndEnqueue"
  | "uploadEnqueuePaths"
  | "uploadRetryTask"
  | "uploadCancelTask"
  | "uploadCancelAll"
  | "uploadDismissTask"
  | "uploadDismissErrors"
  | "uploadSetConcurrency"
  | "uploadConsumePendingOpenUi"
  | "uploadSetMobileUiOpen"
> & {
  coverUrl: (id: string) => string | undefined
}

export let libraryBridge: LibraryBridge = {
  ...tauriBridge,
  coverUrl: getBookCoverProtocolUrl,
}

export function setPreviewLibrary(bridge: LibraryBridge) {
  if (import.meta.env.DEV) libraryBridge = bridge
}

export function resetLibraryBridge() {
  libraryBridge = {
    ...tauriBridge,
    coverUrl: getBookCoverProtocolUrl,
  }
}

export const library = {
  bookshelf: {
    book: {
      list: () => libraryBridge.booksList(),
      rename: (id: string, title: string) => libraryBridge.bookRename(id, title),
      delete: (id: string) => libraryBridge.booksDelete([id]),
      deleteBatch: (ids: string[]) => libraryBridge.booksDelete(ids),
      getTags: (id: string) => libraryBridge.bookTagsList(id),
      bindTag: (...args: Parameters<LibraryBridge["bookTagBind"]>) =>
        libraryBridge.bookTagBind(...args),
      unbindTag: (id: string, tagId: string) => libraryBridge.bookTagUnbind(id, tagId),
    },
    trash: {
      list: () => libraryBridge.trashList(),
      restoreBatch: (ids: string[]) => libraryBridge.trashRestore(ids),
      permanentlyDeleteBatch: (ids: string[]) => libraryBridge.trashPermanentlyDelete(ids),
    },
  },
  tag: {
    list: () => libraryBridge.tagsList(),
    define: (...args: Parameters<LibraryBridge["tagDefine"]>) => libraryBridge.tagDefine(...args),
    rename: (id: string, name: string) => libraryBridge.tagRename(id, name),
    recolor: (id: string, color: string) => libraryBridge.tagRecolor(id, color),
    move: (id: string, parentId: string | null) => libraryBridge.tagMove(id, parentId),
    undefine: (...args: Parameters<LibraryBridge["tagUndefine"]>) =>
      libraryBridge.tagUndefine(...args),
  },
}
