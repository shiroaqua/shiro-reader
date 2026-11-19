import { useEffect, useRef, useState, useSyncExternalStore } from "react"
import { useQuery } from "@tanstack/react-query"
import { BookOpen } from "lucide-react"
import { library } from "@/api/library"
import { bookCoverQueryKey } from "@/lib/query-keys"
import { cn } from "cn"

const blobUrlCache = new Map<Blob, { url: string; refs: number }>()
const blobUrlListeners = new Set<() => void>()

function subscribeBlobUrls(listener: () => void) {
  blobUrlListeners.add(listener)
  return () => {
    blobUrlListeners.delete(listener)
  }
}

function notifyBlobUrls() {
  blobUrlListeners.forEach((listener) => listener())
}

function useBlobUrl(blob: Blob | undefined): string | undefined {
  useEffect(() => {
    if (!blob) return
    const existing = blobUrlCache.get(blob)
    if (existing) {
      existing.refs += 1
    } else {
      blobUrlCache.set(blob, { url: URL.createObjectURL(blob), refs: 1 })
      notifyBlobUrls()
    }
    return () => {
      const entry = blobUrlCache.get(blob)
      if (!entry) return
      entry.refs -= 1
      if (entry.refs <= 0) {
        URL.revokeObjectURL(entry.url)
        blobUrlCache.delete(blob)
        notifyBlobUrls()
      }
    }
  }, [blob])

  return useSyncExternalStore(
    subscribeBlobUrls,
    () => (blob ? blobUrlCache.get(blob)?.url : undefined),
    () => undefined
  )
}

function useNearViewport(ref: React.RefObject<HTMLDivElement | null>): boolean {
  const [visible, setVisible] = useState(
    () => typeof IntersectionObserver === "undefined"
  )

  useEffect(() => {
    if (visible) return
    const element = ref.current
    if (!element) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true)
          observer.disconnect()
        }
      },
      { rootMargin: "200px" }
    )
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref, visible])

  return visible
}

/** Book cover with a themed placeholder; size and shape come from `className`. */
export function BookCover({
  bookId,
  className,
  iconClassName,
  imageClassName,
}: {
  bookId: string
  className?: string
  iconClassName?: string
  imageClassName?: string
}) {
  const containerRef = useRef<HTMLDivElement>(null)
  const isNearViewport = useNearViewport(containerRef)
  const coverQuery = useQuery({
    queryKey: bookCoverQueryKey(bookId),
    queryFn: ({ signal }) => library.bookshelf.book.getCover(bookId, signal),
    enabled: isNearViewport,
    staleTime: 5 * 60_000,
    retry: false,
  })
  const coverUrl = useBlobUrl(coverQuery.data)
  const [failedUrl, setFailedUrl] = useState<string | null>(null)
  const showImage = Boolean(coverUrl && !coverQuery.isError && failedUrl !== coverUrl)

  return (
    <div
      ref={containerRef}
      data-slot="book-cover"
      className={cn(
        "relative overflow-hidden rounded-lg border bg-muted",
        className
      )}
    >
      {showImage ? (
        <img
          src={coverUrl}
          alt=""
          loading="lazy"
          className={cn("size-full object-cover", imageClassName)}
          onError={() => setFailedUrl(coverUrl ?? null)}
        />
      ) : (
        <div className="flex size-full items-center justify-center bg-gradient-to-br from-muted to-muted-foreground/5 text-muted-foreground">
          <BookOpen
            aria-hidden="true"
            className={cn("size-8 opacity-40", iconClassName)}
          />
        </div>
      )}
    </div>
  )
}
