import { useCallback, useMemo } from "react"
import {
  useQueries,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query"
import { library } from "@shiro/ui-shared/api/library"
import { buildBookTagIndex, type BookTagIndex } from "@shiro/ui-shared/lib/book-search"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { booksQueryKey, bookTagsQueryKey, tagsQueryKey } from "@shiro/ui-shared/lib/query-keys"
import type { Book, BookTag, TagTreeNode } from "@shiro/ui-shared/lib/types"

const MAX_TAG_FETCH_CONCURRENCY = 6
let activeTagFetches = 0
const tagFetchQueue: (() => void)[] = []

function releaseTagFetchSlot() {
  const next = tagFetchQueue.shift()
  if (next) next()
  else activeTagFetches -= 1
}

/**
 * Bounds concurrent per-book tag requests so large libraries do not queue
 * dozens of fetches behind browser connection limits and hit request timeouts.
 */
function enqueueTagFetch<T>(
  task: () => Promise<T>,
  signal?: AbortSignal
): Promise<T> {
  if (signal?.aborted) {
    return Promise.reject(new DOMException("Aborted", "AbortError"))
  }
  return new Promise<T>((resolve, reject) => {
    const run = () => {
      signal?.removeEventListener("abort", onAbort)
      if (signal?.aborted) {
        reject(new DOMException("Aborted", "AbortError"))
        releaseTagFetchSlot()
        return
      }
      Promise.resolve()
        .then(task)
        .then(resolve, reject)
        .finally(releaseTagFetchSlot)
    }
    const onAbort = () => {
      const position = tagFetchQueue.indexOf(run)
      if (position !== -1) tagFetchQueue.splice(position, 1)
      reject(new DOMException("Aborted", "AbortError"))
    }
    if (activeTagFetches < MAX_TAG_FETCH_CONCURRENCY) {
      activeTagFetches += 1
      run()
    } else {
      signal?.addEventListener("abort", onAbort, { once: true })
      tagFetchQueue.push(run)
    }
  })
}

type BindingsState = {
  data: (BookTag[] | undefined)[]
  settled: number
  pending: number
  failed: number
}

// Module-level so React Query can memoize the combined result between renders.
function combineBindings(results: UseQueryResult<BookTag[]>[]): BindingsState {
  let settled = 0
  let pending = 0
  let failed = 0
  const data = results.map((result) => {
    if (result.isPending) pending += 1
    else settled += 1
    if (result.isError) failed += 1
    return result.data
  })
  return { data, settled, pending, failed }
}

/**
 * Loads every tag plus every book's bindings (one request per book, cached by
 * React Query) and indexes them for searching. Nothing is fetched until
 * `enabled` turns true; already cached data is still used while disabled.
 */
export function useBookTagIndex(enabled: boolean): {
  books: Book[]
  index: BookTagIndex | null
  loaded: number
  total: number
  isLoading: boolean
  isError: boolean
  isTagsError: boolean
  retry: () => void
} {
  const { locale } = useTranslation()
  const queryClient = useQueryClient()
  const booksQuery = useQuery<Book[]>({
    queryKey: booksQueryKey,
    queryFn: () => library.bookshelf.book.list(),
  })
  const tagsQuery = useQuery<TagTreeNode[]>({
    queryKey: tagsQueryKey,
    queryFn: () => library.tag.list(),
    enabled,
  })
  const bookList = useMemo(() => booksQuery.data ?? [], [booksQuery.data])
  const bindings = useQueries({
    queries: bookList.map((book) => ({
      queryKey: bookTagsQueryKey(book.id),
      queryFn: ({ signal }) =>
        enqueueTagFetch(() => library.bookshelf.book.getTags(book.id), signal),
      enabled,
      staleTime: 60_000,
    })),
    combine: combineBindings,
  })

  const index = useMemo(() => {
    if (!tagsQuery.data) return null
    const map = new Map<string, BookTag[]>()
    bookList.forEach((book, position) => {
      const list = bindings.data[position]
      if (list) map.set(book.id, list)
    })
    return buildBookTagIndex(tagsQuery.data, map, locale)
  }, [tagsQuery.data, bookList, bindings.data, locale])

  const retry = useCallback(() => {
    void queryClient.refetchQueries({
      predicate: (query) =>
        query.state.status === "error" &&
        (query.queryKey[0] === tagsQueryKey[0] ||
          (query.queryKey[0] === booksQueryKey[0] &&
            query.queryKey[2] === "tags")),
    })
  }, [queryClient])

  return {
    books: bookList,
    index,
    loaded: bindings.settled,
    total: bookList.length,
    isLoading:
      enabled &&
      (booksQuery.isPending || tagsQuery.isPending || bindings.pending > 0),
    isError: enabled && (tagsQuery.isError || bindings.failed > 0),
    isTagsError: enabled && tagsQuery.isError,
    retry,
  }
}
