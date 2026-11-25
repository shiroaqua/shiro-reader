import { useEffect, useRef, useState } from "react"
import { BookOpen } from "lucide-react"
import { libraryBridge } from "@shiro/ui-shared/api/library"
import { cn } from "cn"

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
  const [visible, setVisible] = useState(
    () => typeof IntersectionObserver === "undefined"
  )
  const [failedUrl, setFailedUrl] = useState<string>()

  useEffect(() => {
    if (visible || !containerRef.current) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true)
          observer.disconnect()
        }
      },
      { rootMargin: "200px" }
    )
    observer.observe(containerRef.current)
    return () => observer.disconnect()
  }, [visible])

  const coverUrl = visible ? libraryBridge.coverUrl(bookId) : undefined

  return (
    <div
      ref={containerRef}
      data-slot="book-cover"
      className={cn("relative overflow-hidden rounded-lg border bg-muted", className)}
    >
      {coverUrl && failedUrl !== coverUrl ? (
        <img
          src={coverUrl}
          alt=""
          loading="lazy"
          decoding="async"
          className={cn("size-full object-cover", imageClassName)}
          onError={() => setFailedUrl(coverUrl)}
        />
      ) : (
        <div className="flex size-full items-center justify-center bg-gradient-to-br from-muted to-muted-foreground/5 text-muted-foreground">
          <BookOpen aria-hidden="true" className={cn("size-8 opacity-40", iconClassName)} />
        </div>
      )}
    </div>
  )
}
