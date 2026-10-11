import type { Bookshelf } from "@/lib/types"

export function bookshelfBooksQueryKey(bookshelfId: Bookshelf["id"]) {
  return ["bookshelves", bookshelfId, "books"] as const
}
