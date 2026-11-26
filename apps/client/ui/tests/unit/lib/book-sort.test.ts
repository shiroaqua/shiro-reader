import { describe, expect, it, vi } from "vitest"
import {
  DEFAULT_BOOK_SORT,
  loadBookSort,
  parseBookSort,
  saveBookSort,
  sortBooks,
  type BookSort,
} from "@shiro/ui-shared/lib/book-sort"
import { makeBook } from "../../support/builders"

describe("book-sort", () => {
  describe("sortBooks", () => {
    it("sorts titles with numeric natural collation and newer-date tie-breaker without mutating input", () => {
      const book10 = makeBook({
        id: "1",
        title: "Book 10",
        updated_at: "2026-01-01T00:00:00.000Z",
      })
      const book2 = makeBook({
        id: "2",
        title: "Book 2",
        updated_at: "2026-01-02T00:00:00.000Z",
      })
      const sameTitleOlder = makeBook({
        id: "3",
        title: "Alpha",
        updated_at: "2026-01-01T00:00:00.000Z",
      })
      const sameTitleNewer = makeBook({
        id: "4",
        title: "alpha",
        updated_at: "2026-01-05T00:00:00.000Z",
      })
      const input = [book10, sameTitleOlder, book2, sameTitleNewer]

      const asc = sortBooks(input, { field: "title", order: "asc" }, "en")
      expect(asc.map((book) => book.id)).toEqual(["4", "3", "2", "1"])

      const desc = sortBooks(input, { field: "title", order: "desc" }, "en")
      expect(desc.map((book) => book.id)).toEqual(["1", "2", "4", "3"])

      // Input array order remains untouched
      expect(input.map((book) => book.id)).toEqual(["1", "3", "2", "4"])
    })

    it.each<["created_at" | "updated_at"]>([["created_at"], ["updated_at"]])(
      "sorts by %s with title tie-breaker and treats unparseable dates as epoch zero",
      (field) => {
        const otherField = field === "created_at" ? "updated_at" : "created_at"
        const invalidDate = makeBook({
          id: "invalid",
          title: "Zebra",
          [field]: "not-a-date",
          [otherField]: "2026-04-01T00:00:00.000Z",
        })
        const older = makeBook({
          id: "older",
          title: "Bravo",
          [field]: "2026-02-01T10:00:00.000Z",
          [otherField]: "2026-03-15T10:00:00.000Z",
        })
        const newerA = makeBook({
          id: "newer-a",
          title: "Alpha",
          [field]: "2026-03-01T10:00:00.000Z",
          [otherField]: "2026-01-01T10:00:00.000Z",
        })
        const newerB = makeBook({
          id: "newer-b",
          title: "Charlie",
          [field]: "2026-03-01T10:00:00.000Z",
          [otherField]: "2026-01-02T10:00:00.000Z",
        })

        const desc = sortBooks(
          [invalidDate, older, newerB, newerA],
          { field, order: "desc" },
          "en"
        )
        expect(desc.map((book) => book.id)).toEqual([
          "newer-a",
          "newer-b",
          "older",
          "invalid",
        ])

        const asc = sortBooks(
          [invalidDate, older, newerB, newerA],
          { field, order: "asc" },
          "en"
        )
        expect(asc.map((book) => book.id)).toEqual([
          "invalid",
          "older",
          "newer-a",
          "newer-b",
        ])
      }
    )
  })

  describe("parseBookSort", () => {
    it.each<[string | null, BookSort]>([
      ["title:asc", { field: "title", order: "asc" }],
      ["title:desc", { field: "title", order: "desc" }],
      ["created_at:asc", { field: "created_at", order: "asc" }],
      ["created_at:desc", { field: "created_at", order: "desc" }],
      ["updated_at:asc", { field: "updated_at", order: "asc" }],
      ["updated_at:desc", { field: "updated_at", order: "desc" }],
      ["date:asc", DEFAULT_BOOK_SORT],
      ["date:desc", DEFAULT_BOOK_SORT],
      [null, DEFAULT_BOOK_SORT],
      ["", DEFAULT_BOOK_SORT],
      ["author:asc", DEFAULT_BOOK_SORT],
      ["title:random", DEFAULT_BOOK_SORT],
      ["title", DEFAULT_BOOK_SORT],
    ])("parses %j as %j", (raw, expected) => {
      expect(parseBookSort(raw)).toEqual(expected)
    })
  })

  describe("loadBookSort and saveBookSort", () => {
    it("persists and loads sort preferences through localStorage", () => {
      expect(loadBookSort()).toEqual(DEFAULT_BOOK_SORT)

      saveBookSort({ field: "created_at", order: "asc" })
      expect(loadBookSort()).toEqual({ field: "created_at", order: "asc" })

      saveBookSort({ field: "title", order: "asc" })
      expect(loadBookSort()).toEqual({ field: "title", order: "asc" })
    })

    it("falls back safely when localStorage throws", () => {
      vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
        throw new Error("storage disabled")
      })
      vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
        throw new Error("storage disabled")
      })

      expect(() => saveBookSort({ field: "title", order: "asc" })).not.toThrow()
      expect(loadBookSort()).toEqual(DEFAULT_BOOK_SORT)
    })
  })
})
