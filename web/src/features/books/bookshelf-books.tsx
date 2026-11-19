import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { CircleAlert, LibraryBig, RotateCcw } from "lucide-react"
import { library } from "@/api/library"
import { EmptyState } from "@/components/empty-state"
import { PageSection, PageToolbar } from "@/components/page-layout"
import { SortMenu } from "@/components/sort-menu"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { BookCard } from "@/features/books/book-card"
import { UploadBookButton } from "@/features/books/upload-book-button"
import { booksQueryKey } from "@/lib/query-keys"
import {
  loadBookSort,
  saveBookSort,
  sortBooks,
  type BookSort,
} from "@/lib/book-sort"
import { useTranslation } from "@/lib/i18n"
import type { Book } from "@/lib/types"

export function BookshelfBooks() {
  const { t, locale } = useTranslation()
  const {
    data: books,
    isPending,
    isError,
    isFetching,
    refetch,
  } = useQuery<Book[]>({
    queryKey: booksQueryKey,
    queryFn: () => library.bookshelf.book.list(),
  })
  const [sort, setSort] = useState<BookSort>(loadBookSort)
  const sortedBooks = useMemo(
    () => (books ? sortBooks(books, sort, locale) : []),
    [books, sort, locale]
  )

  const changeSort = (next: BookSort) => {
    setSort(next)
    saveBookSort(next)
  }

  return (
    <PageSection aria-label={t("local.dashboard.bookshelf")}>
      <PageToolbar
        start={
          books &&
          books.length > 0 && (
            <span className="tabular-nums">
              {t("local.book.count").replace("{count}", String(books.length))}
            </span>
          )
        }
        end={
          <>
            <SortMenu
              fields={[
                { value: "title", label: t("local.book.sort.title") },
                { value: "date", label: t("local.book.sort.date") },
              ]}
              sort={sort}
              onChange={changeSort}
            />
            <UploadBookButton />
          </>
        }
      />
      {isPending ? (
        <div
          aria-busy="true"
          aria-label={t("local.book.loading")}
          className={GRID_CLASS_NAME}
        >
          {Array.from({ length: 8 }, (_, index) => (
            <div key={index} className="flex flex-col items-center gap-2">
              <Skeleton className="aspect-[3/4] w-full rounded-xl" />
              <Skeleton className="mt-1 h-4 w-3/4" />
              <Skeleton className="h-3 w-1/2" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <EmptyState
          tone="destructive"
          icon={<CircleAlert aria-hidden="true" />}
          title={t("local.book.load_failed")}
          action={
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isFetching}
              onClick={() => void refetch()}
            >
              <RotateCcw aria-hidden="true" />
              {t("local.common.retry")}
            </Button>
          }
        />
      ) : books.length === 0 ? (
        <EmptyState
          icon={<LibraryBig aria-hidden="true" />}
          title={t("local.book.empty")}
          description={t("local.book.empty_hint")}
        />
      ) : (
        <ul aria-label={t("local.book.list")} className={GRID_CLASS_NAME}>
          {sortedBooks.map((book) => (
            <BookCard key={book.id} book={book} />
          ))}
        </ul>
      )}
    </PageSection>
  )
}

const GRID_CLASS_NAME =
  "grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-5 gap-y-8 sm:grid-cols-[repeat(auto-fill,minmax(9.5rem,1fr))]"
