export const booksQueryKey = ["books"] as const
export const trashQueryKey = ["trash"] as const
export const tagsQueryKey = ["tags"] as const
export const bookTagsQueryKey = (bookId: string) => [...booksQueryKey, bookId, "tags"] as const
export const bookCoverQueryKey = (bookId: string) => ["book-cover", bookId] as const
