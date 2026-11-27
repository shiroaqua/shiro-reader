export const booksQueryKey = ["books"] as const
export const trashQueryKey = ["trash"] as const
export const tagsQueryKey = ["tags"] as const
export const bookMetadataQueryKey = (bookId: string) =>
  [...booksQueryKey, bookId, "metadata"] as const
export const bookTagsQueryKey = (bookId: string) => [...booksQueryKey, bookId, "tags"] as const
