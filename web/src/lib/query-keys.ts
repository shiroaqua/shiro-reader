export const booksQueryKey = ["books"] as const
export const tagsQueryKey = ["tags"] as const
export const bookTagsQueryKey = (bookId: string) => [...booksQueryKey, bookId, "tags"] as const
