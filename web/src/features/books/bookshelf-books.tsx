import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import {
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  LibraryBig,
} from "lucide-react"
import { library } from "@/api/library"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { BookCard } from "@/features/books/book-card"
import { UploadBookButton } from "@/features/books/upload-book-button"
import { booksQueryKey } from "@/lib/query-keys"
import {
  loadBookSort,
  saveBookSort,
  sortBooks,
  type BookSort,
  type BookSortField,
  type BookSortOrder,
} from "@/lib/book-sort"
import { useTranslation } from "@/lib/i18n"
import type { Book } from "@/lib/types"

export function BookshelfBooks() {
  const { t, locale } = useTranslation()
  const {
    data: books,
    isPending,
    isError,
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
    <section
      aria-labelledby="bookshelf-books-title"
      className="flex flex-1 flex-col gap-5 p-6"
    >
      <div className="flex items-center justify-end gap-2">
        <SortBooksMenu sort={sort} onChange={changeSort} />
        <UploadBookButton />
      </div>
      {isPending ? (
        <div
          aria-busy="true"
          aria-label={t("book.loading")}
          className={GRID_CLASS_NAME}
        >
          {Array.from({ length: 8 }, (_, index) => (
            <div key={index} className="flex flex-col gap-2.5">
              <Skeleton className="aspect-[3/4] w-full rounded-xl" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-3 w-1/2" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <p role="status" className="text-sm text-muted-foreground">
          {t("book.load_failed")}
        </p>
      ) : books.length === 0 ? (
        <div
          role="status"
          className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"
        >
          <div className="flex size-20 items-center justify-center rounded-2xl bg-muted/80">
            <LibraryBig aria-hidden="true" className="size-10" />
          </div>
          <p className="text-sm font-medium text-foreground">
            {t("book.empty")}
          </p>
        </div>
      ) : (
        <ul aria-label={t("book.list")} className={GRID_CLASS_NAME}>
          {sortedBooks.map((book) => (
            <BookCard key={book.id} book={book} />
          ))}
        </ul>
      )}
    </section>
  )
}

function SortBooksMenu({
  sort,
  onChange,
}: {
  sort: BookSort
  onChange: (sort: BookSort) => void
}) {
  const { t } = useTranslation()
  const Icon = sort.order === "asc" ? ArrowUpNarrowWide : ArrowDownWideNarrow

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button type="button" variant="outline" size="sm" />}
      >
        <Icon aria-hidden="true" />
        {t(sort.field === "title" ? "book.sort.title" : "book.sort.date")}
      </DropdownMenuTrigger>
      <DropdownMenuContent aria-label={t("book.sort")}>
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("book.sort.by")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.field}
            onValueChange={(field: BookSortField) =>
              onChange({ ...sort, field })
            }
          >
            <DropdownMenuRadioItem value="title">
              {t("book.sort.title")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="date">
              {t("book.sort.date")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("book.sort.order")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.order}
            onValueChange={(order: BookSortOrder) =>
              onChange({ ...sort, order })
            }
          >
            <DropdownMenuRadioItem value="asc">
              {t("book.sort.asc")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="desc">
              {t("book.sort.desc")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

const GRID_CLASS_NAME =
  "grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-x-5 gap-y-7"
