import { useId, useMemo, useRef, useState, type CSSProperties } from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  Calendar,
  ChevronLeft,
  CircleAlert,
  Hash,
  LoaderCircle,
  Pencil,
  Plus,
  RotateCcw,
  X,
} from "lucide-react"
import { library } from "@shiro/ui-shared/api/library"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { shouldCancelUnderlyingModalDismiss } from "@shiro/ui-shared/components/ui/dialog"
import { Input } from "@shiro/ui-shared/components/ui/input"
import { NativeSelect } from "@shiro/ui-shared/components/ui/native-select"
import { Skeleton } from "@shiro/ui-shared/components/ui/skeleton"
import { Textarea } from "@shiro/ui-shared/components/ui/textarea"
import { BookCover } from "@shiro/ui-shared/features/books/book-cover"
import { tagNameColor } from "@shiro/ui-shared/features/tags/tag-color-picker"
import { isMobilePlatform } from "@shiro/ui-shared/hooks/use-mobile"
import {
  formatBookAuthors,
  formatBookLanguage,
  isFullIsoDate,
  matchLanguageOption,
  MAX_BOOK_AUTHORS,
  normalizeAuthorInputs,
} from "@shiro/ui-shared/lib/book-metadata"
import { formatBookTagSearchQuery } from "@shiro/ui-shared/lib/book-search"
import {
  formatBookDateTime,
  formatBookPublishedDate,
  formatByteSize,
} from "@shiro/ui-shared/lib/format"
import { translateTagName, useTranslation } from "@shiro/ui-shared/lib/i18n"
import {
  bookMetadataQueryKey,
  booksQueryKey,
  bookTagsQueryKey,
  tagsQueryKey,
} from "@shiro/ui-shared/lib/query-keys"
import { LANGUAGE_OPTIONS } from "@shiro/ui-shared/lib/tag-value-patterns"
import type {
  Book,
  BookMetadata,
  Tag,
  TagTreeNode,
} from "@shiro/ui-shared/lib/types"
import { cn } from "cn"

type AuthorDraftRow = {
  key: number
  value: string
}

type MetadataFormBaseline = {
  title: string
  subtitle: string
  authors: string[]
  publisher: string
  published_date: string
  language: string
  identifier: string
  description: string
}

type MetadataFormDraft = {
  title: string
  subtitle: string
  authors: AuthorDraftRow[]
  publisher: string
  published_date: string
  language: string
  identifier: string
  description: string
}

type OptionalMetadataField =
  | "subtitle"
  | "publisher"
  | "published_date"
  | "language"
  | "identifier"
  | "description"

const OPTIONAL_METADATA_HANDLERS: Record<
  OptionalMetadataField,
  {
    change: (id: string, value: string) => Promise<void>
    clear: (id: string) => Promise<void>
  }
> = {
  subtitle: {
    change: library.bookshelf.book.changeSubtitle,
    clear: library.bookshelf.book.clearSubtitle,
  },
  publisher: {
    change: library.bookshelf.book.changePublisher,
    clear: library.bookshelf.book.clearPublisher,
  },
  published_date: {
    change: library.bookshelf.book.changePublishedDate,
    clear: library.bookshelf.book.clearPublishedDate,
  },
  language: {
    change: library.bookshelf.book.changeLanguage,
    clear: library.bookshelf.book.clearLanguage,
  },
  identifier: {
    change: library.bookshelf.book.changeIdentifier,
    clear: library.bookshelf.book.clearIdentifier,
  },
  description: {
    change: library.bookshelf.book.changeDescription,
    clear: library.bookshelf.book.clearDescription,
  },
}

const REMAINING_OPTIONAL_METADATA_FIELDS: readonly OptionalMetadataField[] = [
  "description",
  "language",
  "publisher",
  "identifier",
  "published_date",
]

function isBookUnavailableError(error: unknown): boolean {
  return (
    error instanceof CommandError &&
    (error.payload.key === "library.book.not_found" ||
      error.payload.key === "library.book.already_deleted")
  )
}

function stringArraysEqual(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index])
}

function toBaselineValues(
  title: string,
  metadata: BookMetadata
): MetadataFormBaseline {
  const normalizedLanguage = metadata.language?.trim() ?? ""
  return {
    title: title.trim(),
    subtitle: metadata.subtitle?.trim() ?? "",
    authors: normalizeAuthorInputs(metadata.authors),
    publisher: metadata.publisher?.trim() ?? "",
    published_date: metadata.published_date?.trim() ?? "",
    language:
      normalizedLanguage === ""
        ? ""
        : (matchLanguageOption(normalizedLanguage)?.code ?? normalizedLanguage),
    identifier: metadata.identifier?.trim() ?? "",
    description: metadata.description?.trim() ?? "",
  }
}

function normalizeDraftValues(values: MetadataFormDraft): MetadataFormBaseline {
  return {
    title: values.title.trim(),
    subtitle: values.subtitle.trim(),
    authors: normalizeAuthorInputs(values.authors.map((row) => row.value)),
    publisher: values.publisher.trim(),
    published_date: values.published_date.trim(),
    language: values.language.trim(),
    identifier: values.identifier.trim(),
    description: values.description.trim(),
  }
}

function formsEqual(
  a: MetadataFormBaseline,
  b: MetadataFormBaseline
): boolean {
  return (
    a.title === b.title &&
    a.subtitle === b.subtitle &&
    stringArraysEqual(a.authors, b.authors) &&
    a.publisher === b.publisher &&
    a.published_date === b.published_date &&
    a.language === b.language &&
    a.identifier === b.identifier &&
    a.description === b.description
  )
}

function flattenTagTree(nodes: readonly TagTreeNode[], out: Map<string, Tag>) {
  for (const node of nodes) {
    const { children, ...tag } = node
    out.set(tag.id, tag)
    flattenTagTree(children, out)
  }
}

export function BookPropertiesDialog({
  book,
  open,
  onOpenChange,
  onBookDeleted,
  onSearchTag,
}: {
  book: Book
  open: boolean
  onOpenChange: (open: boolean) => void
  onBookDeleted?: (() => void) | undefined
  onSearchTag?: ((query: string) => void) | undefined
}) {
  const { t } = useTranslation()
  const [mode, setMode] = useState<"view" | "edit">("view")
  const [isSaving, setIsSaving] = useState(false)
  const [prevOpen, setPrevOpen] = useState(open)
  if (open !== prevOpen) {
    setPrevOpen(open)
    if (!open) {
      setMode("view")
      setIsSaving(false)
    }
  }

  const isMobile = isMobilePlatform()
  const labelTitle =
    mode === "edit"
      ? t("local.book.metadata.edit_title")
      : t("local.book.details.title")

  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(next, eventDetails) => {
        if (shouldCancelUnderlyingModalDismiss(next, eventDetails, true)) return
        if (!next) {
          if (isSaving) {
            eventDetails.cancel()
            return
          }
          if (eventDetails.reason === "escape-key" && mode === "edit") {
            eventDetails.cancel()
            setMode("view")
            return
          }
        }
        onOpenChange(next)
      }}
    >
      <DialogPrimitive.Portal>
        {!isMobile && (
          <DialogPrimitive.Backdrop className="fixed inset-0 z-50 bg-black/15 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0 supports-backdrop-filter:backdrop-blur-xs max-sm:hidden" />
        )}
        <DialogPrimitive.Popup
          data-slot="book-properties-dialog"
          aria-label={labelTitle}
          className={cn(
            "fixed z-50 flex flex-col bg-background text-sm text-foreground outline-none transition-[opacity,translate] duration-200 data-ending-style:opacity-0 data-starting-style:opacity-0",
            isMobile
              ? "inset-0"
              : "max-sm:inset-0 sm:top-1/2 sm:left-1/2 sm:max-h-[min(44rem,calc(100dvh-2rem))] sm:w-full sm:max-w-lg sm:-translate-x-1/2 sm:-translate-y-1/2 sm:overflow-hidden sm:rounded-2xl sm:bg-popover sm:text-popover-foreground sm:shadow-xl sm:ring-1 sm:ring-foreground/10"
          )}
        >
          {open && (
            <BookPropertiesBody
              key={book.id}
              book={book}
              mode={mode}
              isMobile={isMobile}
              onModeChange={setMode}
              onSavingChange={setIsSaving}
              onClose={() => onOpenChange(false)}
              onBookDeleted={onBookDeleted}
              onSearchTag={onSearchTag}
            />
          )}
        </DialogPrimitive.Popup>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}

function BookPropertiesBody({
  book,
  mode,
  isMobile,
  onModeChange,
  onSavingChange,
  onClose,
  onBookDeleted,
  onSearchTag,
}: {
  book: Book
  mode: "view" | "edit"
  isMobile: boolean
  onModeChange: (mode: "view" | "edit") => void
  onSavingChange: (saving: boolean) => void
  onClose: () => void
  onBookDeleted?: (() => void) | undefined
  onSearchTag?: ((query: string) => void) | undefined
}) {
  const { t, locale } = useTranslation()
  const queryClient = useQueryClient()
  const formIdPrefix = useId()

  const nextAuthorKeyRef = useRef(0)
  const pendingFocusAuthorKeyRef = useRef<number | null>(null)
  const authorInputRefs = useRef(new Map<number, HTMLInputElement>())

  const [draft, setDraft] = useState<MetadataFormDraft>(() => ({
    title: book.title,
    subtitle: "",
    authors: [],
    publisher: "",
    published_date: "",
    language: "",
    identifier: "",
    description: "",
  }))
  const [baseline, setBaseline] = useState<MetadataFormBaseline>(() => ({
    title: book.title,
    subtitle: "",
    authors: [],
    publisher: "",
    published_date: "",
    language: "",
    identifier: "",
    description: "",
  }))
  const baselineRef = useRef<MetadataFormBaseline>(baseline)

  const handleUnavailableBook = () => {
    queryClient.setQueryData<Book[]>(
      booksQueryKey,
      (books) => books?.filter((item) => item.id !== book.id)
    )
    onBookDeleted?.()
    onClose()
  }

  const metadataQuery = useQuery({
    queryKey: bookMetadataQueryKey(book.id),
    queryFn: async () => {
      try {
        return await library.bookshelf.book.getMetadata(book.id)
      } catch (error) {
        if (isBookUnavailableError(error)) {
          handleUnavailableBook()
        }
        throw error
      }
    },
    staleTime: 0,
  })

  const tagsQuery = useQuery({
    queryKey: tagsQueryKey,
    queryFn: () => library.tag.list(),
  })

  const bookTagsQuery = useQuery({
    queryKey: bookTagsQueryKey(book.id),
    queryFn: () => library.bookshelf.book.getTags(book.id),
    staleTime: 0,
  })

  const resolvedTags = useMemo(() => {
    if (!tagsQuery.data || !bookTagsQuery.data) return []
    const tagById = new Map<string, Tag>()
    flattenTagTree(tagsQuery.data, tagById)
    return bookTagsQuery.data.flatMap((binding) => {
      const tag = tagById.get(binding.tag_id)
      if (!tag) return []
      const displayName = translateTagName(tag.name, locale)
      const value =
        binding.value !== null && binding.value !== "" ? binding.value : null
      return [
        {
          tag,
          displayName,
          value,
          query: formatBookTagSearchQuery(tag, value, locale),
        },
      ]
    })
  }, [tagsQuery.data, bookTagsQuery.data, locale])

  const saveMutation = useMutation({
    mutationFn: async (submitted: MetadataFormDraft) => {
      const nextDraft = normalizeDraftValues(submitted)
      const currentBaseline: MetadataFormBaseline = {
        ...baselineRef.current,
        authors: [...baselineRef.current.authors],
      }

      const commitBaseline = () => {
        const snapshot: MetadataFormBaseline = {
          ...currentBaseline,
          authors: [...currentBaseline.authors],
        }
        baselineRef.current = snapshot
        setBaseline(snapshot)
      }

      const syncOptionalField = async (field: OptionalMetadataField) => {
        if (nextDraft[field] === currentBaseline[field]) return
        const handler = OPTIONAL_METADATA_HANDLERS[field]
        if (nextDraft[field] === "") {
          await handler.clear(book.id)
        } else {
          await handler.change(book.id, nextDraft[field])
        }
        currentBaseline[field] = nextDraft[field]
        commitBaseline()
      }

      if (nextDraft.title !== currentBaseline.title) {
        await library.bookshelf.book.rename(book.id, nextDraft.title)
        currentBaseline.title = nextDraft.title
        commitBaseline()
        queryClient.setQueryData<Book[]>(
          booksQueryKey,
          (books) =>
            books?.map((item) =>
              item.id === book.id ? { ...item, title: nextDraft.title } : item
            )
        )
      }

      await syncOptionalField("subtitle")

      if (!stringArraysEqual(nextDraft.authors, currentBaseline.authors)) {
        if (nextDraft.authors.length === 0) {
          await library.bookshelf.book.clearAuthors(book.id)
        } else {
          await library.bookshelf.book.changeAuthors(book.id, nextDraft.authors)
        }
        currentBaseline.authors = [...nextDraft.authors]
        commitBaseline()
      }

      for (const field of REMAINING_OPTIONAL_METADATA_FIELDS) {
        await syncOptionalField(field)
      }
    },
    onMutate: () => {
      onSavingChange(true)
    },
    onSuccess: () => {
      onModeChange("view")
    },
    onError: (error) => {
      if (isBookUnavailableError(error)) {
        handleUnavailableBook()
      }
    },
    onSettled: async () => {
      onSavingChange(false)
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: bookMetadataQueryKey(book.id),
          refetchType: "all",
        }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ])
    },
  })

  const metadata = metadataQuery.data
  const isSaving = saveMutation.isPending
  const normalizedDraft = normalizeDraftValues(draft)
  const isDirty = !formsEqual(normalizedDraft, baseline)
  const isTitleMissing = normalizedDraft.title === ""
  const formattedAuthors = metadata
    ? formatBookAuthors(metadata.authors, locale)
    : null
  const unmatchedServerLanguage =
    metadata?.language && !matchLanguageOption(metadata.language)
      ? metadata.language.trim()
      : null

  const enterEditMode = () => {
    if (!metadata || isSaving) return
    const initial = toBaselineValues(book.title, metadata)
    const authorRows: AuthorDraftRow[] = initial.authors.map((value) => {
      const key = nextAuthorKeyRef.current
      nextAuthorKeyRef.current += 1
      return { key, value }
    })
    baselineRef.current = { ...initial, authors: [...initial.authors] }
    setBaseline({ ...initial, authors: [...initial.authors] })
    setDraft({
      title: book.title,
      subtitle: metadata.subtitle ?? "",
      authors: authorRows,
      publisher: metadata.publisher ?? "",
      published_date: metadata.published_date ?? "",
      language: initial.language,
      identifier: metadata.identifier ?? "",
      description: metadata.description ?? "",
    })
    onModeChange("edit")
  }

  const exitEditMode = () => {
    if (isSaving) return
    onModeChange("view")
  }

  const headerTitle =
    mode === "edit"
      ? t("local.book.metadata.edit_title")
      : t("local.book.details.title")

  return (
    <>
      <header
        className={cn(
          "flex shrink-0 items-center justify-between gap-2 border-b px-4 py-3",
          isMobile
            ? "pt-[max(0.75rem,var(--shiro-safe-top,env(safe-area-inset-top,0px)))]"
            : "max-sm:pt-[max(0.75rem,var(--shiro-safe-top,env(safe-area-inset-top,0px)))] sm:px-5 sm:py-3.5"
        )}
      >
        <div className="flex min-w-0 items-center gap-1.5">
          {mode === "edit" ? (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("local.common.back")}
              disabled={isSaving}
              onClick={exitEditMode}
            >
              <ChevronLeft className="size-5" aria-hidden="true" />
            </Button>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("local.common.back")}
              className={cn(!isMobile && "sm:hidden")}
              onClick={onClose}
            >
              <ChevronLeft className="size-5" aria-hidden="true" />
            </Button>
          )}
          <DialogPrimitive.Title className="truncate font-heading text-base font-semibold">
            {headerTitle}
          </DialogPrimitive.Title>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {mode === "view" ? (
            <>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={t("local.book.metadata.edit")}
                disabled={!metadata || metadataQuery.isPending}
                onClick={enterEditMode}
              >
                <Pencil aria-hidden="true" />
                <span>{t("local.book.metadata.edit")}</span>
              </Button>
              {!isMobile && (
                <DialogPrimitive.Close
                  render={
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t("local.common.close")}
                      className="max-sm:hidden"
                    />
                  }
                >
                  <X aria-hidden="true" />
                </DialogPrimitive.Close>
              )}
            </>
          ) : (
            <Button
              type="button"
              size="sm"
              disabled={isSaving || !isDirty || isTitleMissing}
              onClick={() => saveMutation.mutate(draft)}
            >
              {isSaving && (
                <LoaderCircle className="animate-spin" aria-hidden="true" />
              )}
              {isSaving
                ? t("local.book.metadata.saving")
                : t("local.book.metadata.save")}
            </Button>
          )}
        </div>
      </header>

      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-4 sm:px-5",
          isMobile
            ? "pb-[max(1.25rem,var(--shiro-safe-bottom,env(safe-area-inset-bottom,0px)))]"
            : "max-sm:pb-[max(1.25rem,var(--shiro-safe-bottom,env(safe-area-inset-bottom,0px)))] sm:pb-5"
        )}
      >
        {mode === "view" ? (
          <div className="flex flex-col gap-5">
            <div className="flex items-start gap-4">
              <BookCover
                bookId={book.id}
                className="aspect-[5/7] w-24 shrink-0 rounded-xl shadow-sm sm:w-28"
              />
              <div className="flex min-w-0 flex-1 flex-col gap-1 pt-1">
                <p className="text-base leading-snug font-semibold wrap-break-word text-foreground">
                  {book.title}
                </p>
                {metadata?.subtitle && (
                  <p className="text-sm leading-snug wrap-break-word text-foreground/85">
                    {metadata.subtitle}
                  </p>
                )}
                {formattedAuthors && (
                  <p className="mt-0.5 text-sm leading-snug wrap-break-word text-muted-foreground">
                    {formattedAuthors}
                  </p>
                )}
              </div>
            </div>

            {metadataQuery.isPending ? (
              <div
                aria-busy="true"
                aria-label={t("local.common.loading")}
                className="flex flex-col gap-3"
              >
                {Array.from({ length: 6 }).map((_, index) => (
                  <div key={index} className="flex items-center gap-4">
                    <Skeleton className="h-4 w-20 shrink-0" />
                    <Skeleton className="h-4 flex-1" />
                  </div>
                ))}
              </div>
            ) : metadataQuery.isError || !metadata ? (
              <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed px-4 py-8 text-center">
                <CircleAlert
                  aria-hidden="true"
                  className="size-6 text-muted-foreground"
                />
                <p className="text-sm text-muted-foreground">
                  {t("local.book.metadata.load_failed")}
                </p>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void metadataQuery.refetch()}
                >
                  <RotateCcw aria-hidden="true" />
                  {t("local.common.retry")}
                </Button>
              </div>
            ) : (
              <dl className="grid grid-cols-[max-content_minmax(0,1fr)] items-baseline gap-x-4 gap-y-3 text-sm">
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.description")}
                  value={metadata.description}
                  fallback={t("local.book.metadata.unknown")}
                  multiline
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.language")}
                  value={formatBookLanguage(metadata.language)}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.publisher")}
                  value={metadata.publisher}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.identifier")}
                  value={metadata.identifier}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.format")}
                  value={metadata.type.toUpperCase()}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.file_size")}
                  value={formatByteSize(metadata.size)}
                  fallback={t("local.book.metadata.unknown")}
                />
                {metadata.page_count !== null && (
                  <MetadataReadOnlyRow
                    label={t("local.book.metadata.page_count")}
                    value={String(metadata.page_count)}
                    fallback={t("local.book.metadata.unknown")}
                  />
                )}
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.published_date")}
                  value={formatBookPublishedDate(metadata.published_date, locale)}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.created_at")}
                  value={formatBookDateTime(metadata.created_at, locale)}
                  fallback={t("local.book.metadata.unknown")}
                />
                <MetadataReadOnlyRow
                  label={t("local.book.metadata.updated_at")}
                  value={formatBookDateTime(metadata.updated_at, locale)}
                  fallback={t("local.book.metadata.unknown")}
                />
              </dl>
            )}

            {resolvedTags.length > 0 && (
              <section
                aria-label={t("local.book.tags")}
                className="flex flex-col gap-2.5 border-t pt-4"
              >
                <h3 className="text-sm font-semibold text-foreground">
                  {t("local.book.tags")}
                </h3>
                <ul className="flex flex-wrap gap-2">
                  {resolvedTags.map(({ tag, displayName, value, query }) => {
                    const accent =
                      tagNameColor(tag.color) ?? "var(--foreground)"
                    const label =
                      value !== null ? `${displayName}: ${value}` : displayName
                    return (
                      <li key={tag.id}>
                        <button
                          type="button"
                          title={label}
                          style={{ "--tag-accent": accent } as CSSProperties}
                          onClick={() => {
                            onClose()
                            onSearchTag?.(query)
                          }}
                          className="inline-flex h-7 max-w-full cursor-pointer items-center gap-1 rounded-full bg-[color-mix(in_oklab,var(--tag-accent)_12%,transparent)] px-2.5 text-xs text-(--tag-accent) ring-1 ring-[color-mix(in_oklab,var(--tag-accent)_25%,transparent)] transition-opacity ring-inset hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                          <Hash
                            aria-hidden="true"
                            className="size-3 shrink-0 opacity-70"
                          />
                          <span className="truncate font-medium">
                            {displayName}
                          </span>
                          {value !== null && (
                            <span className="truncate text-foreground/80">
                              {value}
                            </span>
                          )}
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </section>
            )}
          </div>
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              if (!isSaving && isDirty && !isTitleMissing) {
                saveMutation.mutate(draft)
              }
            }}
          >
            <div className="flex items-start gap-4">
              <BookCover
                bookId={book.id}
                className="aspect-[5/7] w-24 shrink-0 rounded-xl shadow-sm sm:w-28"
              />
              <div className="flex min-w-0 flex-1 flex-col gap-3">
                <div className="flex flex-col gap-1.5">
                  <label
                    htmlFor={`${formIdPrefix}-title`}
                    className="text-sm font-semibold text-foreground"
                  >
                    {t("local.book.title")}{" "}
                    <span className="text-destructive" aria-hidden="true">
                      *
                    </span>
                  </label>
                  <Input
                    id={`${formIdPrefix}-title`}
                    value={draft.title}
                    placeholder={t("local.book.metadata.title_placeholder")}
                    aria-invalid={isTitleMissing}
                    disabled={isSaving}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        title: event.target.value,
                      }))
                    }
                  />
                </div>

                <div className="flex flex-col gap-1.5">
                  <label
                    htmlFor={`${formIdPrefix}-subtitle`}
                    className="text-sm font-semibold text-foreground"
                  >
                    {t("local.book.metadata.subtitle")}
                  </label>
                  <Input
                    id={`${formIdPrefix}-subtitle`}
                    maxLength={256}
                    value={draft.subtitle}
                    placeholder={t("local.book.metadata.subtitle_placeholder")}
                    disabled={isSaving}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        subtitle: event.target.value,
                      }))
                    }
                  />
                </div>
              </div>
            </div>

            <div
              role="group"
              aria-labelledby={`${formIdPrefix}-authors-label`}
              className="flex flex-col gap-1.5"
            >
              <span
                id={`${formIdPrefix}-authors-label`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.authors")}
              </span>
              {draft.authors.length > 0 && (
                <div className="flex flex-col gap-2">
                  {draft.authors.map((row, index) => (
                    <div key={row.key} className="flex items-center gap-2">
                      <Input
                        ref={(element) => {
                          if (element) {
                            authorInputRefs.current.set(row.key, element)
                            if (pendingFocusAuthorKeyRef.current === row.key) {
                              pendingFocusAuthorKeyRef.current = null
                              element.focus()
                            }
                          } else {
                            authorInputRefs.current.delete(row.key)
                          }
                        }}
                        maxLength={256}
                        value={row.value}
                        aria-label={t(
                          "local.book.metadata.author_item_label"
                        ).replace("{index}", String(index + 1))}
                        placeholder={t(
                          "local.book.metadata.author_placeholder"
                        )}
                        disabled={isSaving}
                        onChange={(event) =>
                          setDraft((current) => ({
                            ...current,
                            authors: current.authors.map((item) =>
                              item.key === row.key
                                ? { ...item, value: event.target.value }
                                : item
                            ),
                          }))
                        }
                      />
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        className="shrink-0 pointer-coarse:size-10"
                        aria-label={`${t("local.book.metadata.remove_author")} ${index + 1}`}
                        disabled={isSaving}
                        onClick={() =>
                          setDraft((current) => ({
                            ...current,
                            authors: current.authors.filter(
                              (item) => item.key !== row.key
                            ),
                          }))
                        }
                      >
                        <X aria-hidden="true" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="self-start"
                disabled={isSaving || draft.authors.length >= MAX_BOOK_AUTHORS}
                onClick={() => {
                  const newKey = nextAuthorKeyRef.current
                  nextAuthorKeyRef.current += 1
                  pendingFocusAuthorKeyRef.current = newKey
                  setDraft((current) => ({
                    ...current,
                    authors: [...current.authors, { key: newKey, value: "" }],
                  }))
                }}
              >
                <Plus aria-hidden="true" />
                {t("local.book.metadata.add_author")}
              </Button>
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                htmlFor={`${formIdPrefix}-description`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.description")}
              </label>
              <Textarea
                id={`${formIdPrefix}-description`}
                rows={4}
                maxLength={16_384}
                value={draft.description}
                placeholder={t("local.book.metadata.description_placeholder")}
                disabled={isSaving}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    description: event.target.value,
                  }))
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                htmlFor={`${formIdPrefix}-language`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.language")}
              </label>
              <NativeSelect
                id={`${formIdPrefix}-language`}
                value={draft.language}
                disabled={isSaving}
                className="pointer-coarse:h-10"
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    language: event.target.value,
                  }))
                }
              >
                <option value="">
                  {t("local.book.metadata.language_unset")}
                </option>
                {unmatchedServerLanguage && (
                  <option value={unmatchedServerLanguage} disabled>
                    {unmatchedServerLanguage}
                  </option>
                )}
                {LANGUAGE_OPTIONS.map(({ code, label }) => (
                  <option key={code} value={code}>
                    {label}
                  </option>
                ))}
              </NativeSelect>
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                htmlFor={`${formIdPrefix}-publisher`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.publisher")}
              </label>
              <Input
                id={`${formIdPrefix}-publisher`}
                maxLength={256}
                value={draft.publisher}
                placeholder={t("local.book.metadata.publisher_placeholder")}
                disabled={isSaving}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    publisher: event.target.value,
                  }))
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                htmlFor={`${formIdPrefix}-identifier`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.identifier")}
              </label>
              <Input
                id={`${formIdPrefix}-identifier`}
                maxLength={128}
                value={draft.identifier}
                placeholder={t("local.book.metadata.identifier_placeholder")}
                disabled={isSaving}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    identifier: event.target.value,
                  }))
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                htmlFor={`${formIdPrefix}-published-date`}
                className="text-sm font-semibold text-foreground"
              >
                {t("local.book.metadata.published_date")}
              </label>
              <div className="flex items-center gap-2">
                <Input
                  id={`${formIdPrefix}-published-date`}
                  value={draft.published_date}
                  placeholder={t(
                    "local.book.metadata.published_date_placeholder"
                  )}
                  disabled={isSaving}
                  className="flex-1"
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      published_date: event.target.value,
                    }))
                  }
                />
                <div
                  className={cn(
                    "relative flex size-8 pointer-coarse:size-10 shrink-0 items-center justify-center rounded-lg border border-input bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground has-focus-visible:border-ring has-focus-visible:ring-3 has-focus-visible:ring-ring/50",
                    isSaving && "pointer-events-none opacity-50"
                  )}
                >
                  <Calendar className="size-4" aria-hidden="true" />
                  <input
                    type="date"
                    aria-label={t("local.book.metadata.pick_date")}
                    title={t("local.book.metadata.pick_date")}
                    value={
                      isFullIsoDate(draft.published_date.trim())
                        ? draft.published_date.trim()
                        : ""
                    }
                    disabled={isSaving}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        published_date: event.target.value,
                      }))
                    }
                    className="absolute inset-0 size-full cursor-pointer opacity-0 [&::-webkit-calendar-picker-indicator]:absolute [&::-webkit-calendar-picker-indicator]:inset-0 [&::-webkit-calendar-picker-indicator]:h-full [&::-webkit-calendar-picker-indicator]:w-full [&::-webkit-calendar-picker-indicator]:cursor-pointer"
                  />
                </div>
              </div>
            </div>
          </form>
        )}
      </div>
    </>
  )
}

function MetadataReadOnlyRow({
  label,
  value,
  fallback,
  multiline = false,
}: {
  label: string
  value: string | null | undefined
  fallback: string
  multiline?: boolean
}) {
  const hasValue = value !== null && value !== undefined && value !== ""
  return (
    <>
      <dt className="font-semibold text-foreground">{label}</dt>
      <dd
        className={cn(
          "min-w-0 wrap-break-word",
          multiline && "whitespace-pre-wrap",
          hasValue ? "text-foreground" : "text-muted-foreground"
        )}
      >
        {hasValue ? value : fallback}
      </dd>
    </>
  )
}
