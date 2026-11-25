import type { ReactNode } from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { act, renderHook, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { setPreviewLibrary } from "@shiro/ui-shared/api/library"
import { useBookTagIndex } from "@shiro/ui-shared/features/books/use-book-tag-index"
import { makeBook, makeBookTag, makeTagNode } from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { createTestQueryClient } from "../support/render"

function createWrapper() {
  const queryClient = createTestQueryClient()
  return {
    queryClient,
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  }
}

describe("useBookTagIndex", () => {
  it("defers tag and binding fetches until enabled turns true", async () => {
    const book = makeBook({ id: "b1", title: "Dune" })
    const tag = makeTagNode({ id: "t1", name: "SciFi" })
    const fakeLibrary = new FakeLibrary()
      .seedBooks([book])
      .seedTags([tag])
      .seedBindings("b1", [makeBookTag("t1")])

    const tagsSpy = vi.spyOn(fakeLibrary, "tagsList")
    const bindingsSpy = vi.spyOn(fakeLibrary, "bookTagsList")
    setPreviewLibrary(fakeLibrary)

    const { wrapper } = createWrapper()
    const { result, rerender } = renderHook(
      ({ enabled }) => useBookTagIndex(enabled),
      { initialProps: { enabled: false }, wrapper }
    )

    await waitFor(() => {
      expect(result.current.books).toHaveLength(1)
    })
    expect(tagsSpy).not.toHaveBeenCalled()
    expect(bindingsSpy).not.toHaveBeenCalled()
    expect(result.current.index).toBeNull()

    rerender({ enabled: true })

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false)
      expect(result.current.index).not.toBeNull()
    })
    expect(tagsSpy).toHaveBeenCalledTimes(1)
    expect(bindingsSpy).toHaveBeenCalledTimes(1)
    expect(result.current.loaded).toBe(1)
    expect(result.current.index?.bookCountByTag.get("t1")).toBe(1)
  })

  it("caps concurrent per-book tag binding requests to 6", async () => {
    const books = Array.from({ length: 10 }, (_, idx) =>
      makeBook({ id: `book-${idx}` })
    )
    const fakeLibrary = new FakeLibrary()
      .seedBooks(books)
      .seedTags([makeTagNode({ id: "t1", name: "Tag" })])

    let active = 0
    let maxActive = 0
    const pendingResolvers: (() => void)[] = []

    vi.spyOn(fakeLibrary, "bookTagsList").mockImplementation(() => {
      active += 1
      maxActive = Math.max(maxActive, active)
      return new Promise((resolve) => {
        pendingResolvers.push(() => {
          active -= 1
          resolve([])
        })
      })
    })
    setPreviewLibrary(fakeLibrary)

    const { wrapper } = createWrapper()
    const { result } = renderHook(() => useBookTagIndex(true), { wrapper })

    await waitFor(() => {
      expect(pendingResolvers.length).toBe(6)
    })
    expect(maxActive).toBe(6)

    // Drain all 10 pending requests so the module-level concurrency counter returns to 0
    while (result.current.loaded < 10) {
      const nextBatch = pendingResolvers.splice(0, pendingResolvers.length)
      if (nextBatch.length === 0) {
        await new Promise((resolve) => setTimeout(resolve, 10))
        continue
      }
      act(() => {
        for (const resolve of nextBatch) resolve()
      })
      await waitFor(() => {
        expect(
          pendingResolvers.length > 0 || result.current.loaded === 10
        ).toBe(true)
      })
    }

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false)
    })
    expect(maxActive).toBe(6)
    expect(result.current.loaded).toBe(10)
  })

  it("reports tag/binding errors and retries failed queries cleanly", async () => {
    const book = makeBook({ id: "b1", title: "Retry Book" })
    const tag = makeTagNode({ id: "t1", name: "Fiction" })
    const fakeLibrary = new FakeLibrary()
      .seedBooks([book])
      .seedTags([tag])
      .seedBindings("b1", [makeBookTag("t1")])

    const tagsSpy = vi
      .spyOn(fakeLibrary, "tagsList")
      .mockRejectedValueOnce(new Error("Temporary tag error"))
    setPreviewLibrary(fakeLibrary)

    const { wrapper } = createWrapper()
    const { result } = renderHook(() => useBookTagIndex(true), { wrapper })

    await waitFor(() => {
      expect(result.current.isTagsError).toBe(true)
      expect(result.current.isError).toBe(true)
    })

    act(() => {
      result.current.retry()
    })

    await waitFor(() => {
      expect(result.current.isError).toBe(false)
      expect(result.current.isLoading).toBe(false)
      expect(result.current.index).not.toBeNull()
    })
    expect(tagsSpy).toHaveBeenCalledTimes(2)
  })
})
