import { notifyLocalError } from "@shiro/ui-shared/lib/error-notifications"
import type { Book } from "@shiro/ui-shared/lib/types"
import { normalizeBookFormat } from "../contracts/engine"

/** Return whether the given bookshelf item can currently be opened in the reader. */
export function isBookSupportedInReader(book: Pick<Book, "type">): boolean {
  return normalizeBookFormat(book.type) === "EPUB"
}

/** Validate reader format support before opening a tab or window, notifying on unsupported formats. */
export function assertBookSupportedInReader(book: Pick<Book, "type">): boolean {
  if (isBookSupportedInReader(book)) {
    return true
  }
  notifyLocalError("local.reader.error.unsupported_format")
  return false
}
