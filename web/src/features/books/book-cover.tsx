import { useEffect, useRef, useState, useSyncExternalStore } from "react"
import { useQuery } from "@tanstack/react-query"
import { BookOpen } from "lucide-react"
import { library } from "@/api/library"
import { bookCoverQueryKey } from "@/lib/query-keys"
import { cn } from "cn"

const MAX_COVER_EDGE_PX = 640
const STEP_DOWN_RATIO = 0.5

const blobUrlCache = new Map<
  Blob,
  { url: string; refs: number; task?: Promise<void> }
>()
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

function createScaledCanvas(width: number, height: number): HTMLCanvasElement {
  const canvas = document.createElement("canvas")
  canvas.width = width
  canvas.height = height
  return canvas
}

async function downscaleCoverBlob(blob: Blob): Promise<Blob | null> {
  if (
    typeof document === "undefined" ||
    typeof createImageBitmap !== "function"
  ) {
    return null
  }

  const dpr =
    typeof window !== "undefined"
      ? Math.min(Math.max(window.devicePixelRatio || 1, 1), 3)
      : 2
  const maxEdge = Math.round(MAX_COVER_EDGE_PX * Math.max(dpr / 2, 1))

  let bitmap: ImageBitmap
  try {
    bitmap = await createImageBitmap(blob)
  } catch {
    return null
  }

  try {
    const srcWidth = bitmap.width
    const srcHeight = bitmap.height
    const longestEdge = Math.max(srcWidth, srcHeight)

    if (longestEdge <= maxEdge) {
      return null
    }

    const scale = maxEdge / longestEdge
    const targetWidth = Math.max(1, Math.round(srcWidth * scale))
    const targetHeight = Math.max(1, Math.round(srcHeight * scale))

    let currentWidth = srcWidth
    let currentHeight = srcHeight
    let currentSource: CanvasImageSource = bitmap

    while (
      currentWidth * STEP_DOWN_RATIO > targetWidth &&
      currentHeight * STEP_DOWN_RATIO > targetHeight
    ) {
      const nextWidth = Math.max(
        targetWidth,
        Math.round(currentWidth * STEP_DOWN_RATIO)
      )
      const nextHeight = Math.max(
        targetHeight,
        Math.round(currentHeight * STEP_DOWN_RATIO)
      )
      const stepCanvas = createScaledCanvas(nextWidth, nextHeight)
      const stepCtx = stepCanvas.getContext("2d")
      if (!stepCtx) return null
      stepCtx.imageSmoothingEnabled = true
      stepCtx.imageSmoothingQuality = "high"
      stepCtx.drawImage(
        currentSource,
        0,
        0,
        currentWidth,
        currentHeight,
        0,
        0,
        nextWidth,
        nextHeight
      )
      currentSource = stepCanvas
      currentWidth = nextWidth
      currentHeight = nextHeight
    }

    const finalCanvas = createScaledCanvas(targetWidth, targetHeight)
    const finalCtx = finalCanvas.getContext("2d")
    if (!finalCtx) return null
    finalCtx.imageSmoothingEnabled = true
    finalCtx.imageSmoothingQuality = "high"
    finalCtx.drawImage(
      currentSource,
      0,
      0,
      currentWidth,
      currentHeight,
      0,
      0,
      targetWidth,
      targetHeight
    )

    const outputType =
      blob.type === "image/jpeg" || blob.type === "image/webp"
        ? blob.type
        : "image/png"

    return await new Promise<Blob | null>((resolve) => {
      finalCanvas.toBlob(
        (result) => resolve(result),
        outputType,
        outputType === "image/png" ? undefined : 0.92
      )
    })
  } finally {
    bitmap.close()
  }
}

function useBlobUrl(blob: Blob | undefined): string | undefined {
  useEffect(() => {
    if (!blob) return
    const existing = blobUrlCache.get(blob)
    if (existing) {
      existing.refs += 1
    } else {
      const entry: { url: string; refs: number; task?: Promise<void> } = {
        url: URL.createObjectURL(blob),
        refs: 1,
      }
      blobUrlCache.set(blob, entry)
      notifyBlobUrls()

      entry.task = downscaleCoverBlob(blob)
        .then((scaled) => {
          if (!scaled) return
          const current = blobUrlCache.get(blob)
          if (!current || current.refs <= 0) return
          const prevUrl = current.url
          current.url = URL.createObjectURL(scaled)
          URL.revokeObjectURL(prevUrl)
          notifyBlobUrls()
        })
        .catch(() => {
          // Keep original blob URL on fallback
        })
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
          decoding="async"
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
