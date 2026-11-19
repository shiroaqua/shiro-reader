import type { Book } from "@/lib/types"

export type BookSortField = "title" | "date"
export type BookSortOrder = "asc" | "desc"
export type BookSort = { field: BookSortField; order: BookSortOrder }

export const DEFAULT_BOOK_SORT: BookSort = { field: "date", order: "desc" }

const STORAGE_KEY = "shiro-reader.book-sort"

const compareDate = (a: Book, b: Book) =>
  (Date.parse(a.updated_at) || 0) - (Date.parse(b.updated_at) || 0)

// Returns a sorted copy; "date" uses the update time shown on each card.
export function sortBooks(books: readonly Book[], { field, order }: BookSort, locale?: string) {
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: "base" })
  const direction = order === "asc" ? 1 : -1
  const compare =
    field === "title"
      ? (a: Book, b: Book) => direction * collator.compare(a.title, b.title) || compareDate(b, a)
      : (a: Book, b: Book) => direction * compareDate(a, b) || collator.compare(a.title, b.title)
  return [...books].sort(compare)
}

export function parseBookSort(value: string | null): BookSort {
  if (value) {
    const [field, order] = value.split(":")
    if ((field === "title" || field === "date") && (order === "asc" || order === "desc"))
      return { field, order }
  }
  return DEFAULT_BOOK_SORT
}

export function loadBookSort(): BookSort {
  try {
    return parseBookSort(localStorage.getItem(STORAGE_KEY))
  } catch {
    return DEFAULT_BOOK_SORT
  }
}

export function saveBookSort({ field, order }: BookSort) {
  try {
    localStorage.setItem(STORAGE_KEY, `${field}:${order}`)
  } catch {
    // Storage may be unavailable (e.g. private mode); sorting still works for the session.
  }
}
