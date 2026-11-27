import { getBookCoverProtocolUrl, tauriBridge } from "./tauri-bridge"

export type LibraryBridge = Pick<
  typeof tauriBridge,
  | "booksList"
  | "bookRename"
  | "bookMetadataGet"
  | "bookSubtitleChange"
  | "bookSubtitleClear"
  | "bookAuthorsChange"
  | "bookAuthorsClear"
  | "bookPublisherChange"
  | "bookPublisherClear"
  | "bookPublishedDateChange"
  | "bookPublishedDateClear"
  | "bookLanguageChange"
  | "bookLanguageClear"
  | "bookDescriptionChange"
  | "bookDescriptionClear"
  | "bookIdentifierChange"
  | "bookIdentifierClear"
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
      getMetadata: (id: string) => libraryBridge.bookMetadataGet(id),
      changeSubtitle: (id: string, subtitle: string) =>
        libraryBridge.bookSubtitleChange(id, subtitle),
      clearSubtitle: (id: string) => libraryBridge.bookSubtitleClear(id),
      changeAuthors: (id: string, authors: string[]) =>
        libraryBridge.bookAuthorsChange(id, authors),
      clearAuthors: (id: string) => libraryBridge.bookAuthorsClear(id),
      changePublisher: (id: string, publisher: string) =>
        libraryBridge.bookPublisherChange(id, publisher),
      clearPublisher: (id: string) => libraryBridge.bookPublisherClear(id),
      changePublishedDate: (id: string, publishedDate: string) =>
        libraryBridge.bookPublishedDateChange(id, publishedDate),
      clearPublishedDate: (id: string) =>
        libraryBridge.bookPublishedDateClear(id),
      changeLanguage: (id: string, language: string) =>
        libraryBridge.bookLanguageChange(id, language),
      clearLanguage: (id: string) => libraryBridge.bookLanguageClear(id),
      changeDescription: (id: string, description: string) =>
        libraryBridge.bookDescriptionChange(id, description),
      clearDescription: (id: string) => libraryBridge.bookDescriptionClear(id),
      changeIdentifier: (id: string, identifier: string) =>
        libraryBridge.bookIdentifierChange(id, identifier),
      clearIdentifier: (id: string) => libraryBridge.bookIdentifierClear(id),
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
