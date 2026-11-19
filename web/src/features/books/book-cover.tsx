import { useEffect, useState } from "react"
import { BookOpen } from "lucide-react"
import { library } from "@/api/library"
import { cn } from "cn"

/** Loads a book cover as an object URL, revoking it when the book changes or unmounts. */
function useBookCover(bookId: string) {
  const [cover, setCover] = useState<{ bookId: string; url?: string; failed: boolean }>(
    { bookId, failed: false }
  )

  useEffect(() => {
    let active = true
    let objectUrl: string | undefined
    void library.bookshelf.book
      .getCover(bookId)
      .then((blob) => {
        if (!active) return
        objectUrl = URL.createObjectURL(blob)
        setCover({ bookId, url: objectUrl, failed: false })
      })
      .catch(() => {
        if (active) setCover({ bookId, failed: true })
      })
    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [bookId])

  // Ignore a result that still belongs to the previous book.
  return cover.bookId === bookId ? cover : { bookId, failed: false }
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
  const cover = useBookCover(bookId)
  const [imageFailed, setImageFailed] = useState(false)
  const showImage = cover.url && !cover.failed && !imageFailed

  return (
    <div
      data-slot="book-cover"
      className={cn(
        "relative overflow-hidden rounded-lg border bg-muted",
        className
      )}
    >
      {showImage ? (
        <img
          src={cover.url}
          alt=""
          loading="lazy"
          className={cn("size-full object-cover", imageClassName)}
          onError={() => setImageFailed(true)}
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
