import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { setPreviewLibrary } from "@/api/library"
import {
  cancelActiveBookImport,
  resetBookImportState,
  startBookImport,
  useBookImportState,
} from "@/features/books/book-import-store"
import { booksQueryKey } from "@/lib/query-keys"
import {
  makeBook,
  makeCommandErrorPayload,
  makeImportResult,
} from "../../support/builders"
import { FakeLibrary } from "../../support/fake-library"
import { createTestQueryClient } from "../../support/render"

describe("book-import-store", () => {
  it("tracks streaming progress, invalidates books on item success, and clears summary on clean finish", async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries")
    const importedBook = makeBook({ id: "b1", title: "Tea" })

    let emitEvent!: Parameters<FakeLibrary["pickAndImportBooks"]>[0]
    let resolveImport!: (result: ReturnType<typeof makeImportResult>) => void
    const fakeLibrary = new FakeLibrary().setImportHandler(
      (emit) =>
        new Promise((resolve) => {
          emitEvent = emit
          resolveImport = resolve
        })
    )
    setPreviewLibrary(fakeLibrary)

    const { result } = renderHook(() => useBookImportState())
    expect(result.current.pending).toBe(false)

    let runPromise!: Promise<void>
    act(() => {
      runPromise = startBookImport(queryClient)
    })
    expect(result.current.pending).toBe(true)
    expect(result.current.canCancel).toBe(false)

    act(() => {
      emitEvent({ stage: "started", operationId: "op-1", total: 1 })
      emitEvent({
        stage: "itemProgress",
        operationId: "op-1",
        index: 0,
        total: 1,
        fileName: "tea.epub",
        bytesRead: 50,
        totalBytes: 100,
      })
    })
    expect(result.current.canCancel).toBe(true)
    expect(result.current.progress?.fileName).toBe("tea.epub")

    act(() => {
      emitEvent({
        stage: "itemSucceeded",
        operationId: "op-1",
        index: 0,
        total: 1,
        book: importedBook,
      })
      emitEvent({
        stage: "finished",
        operationId: "op-1",
        cancelled: false,
        importedCount: 1,
        failedCount: 0,
        skippedCount: 0,
      })
    })
    expect(result.current.canCancel).toBe(false)
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: booksQueryKey })

    await act(async () => {
      resolveImport(
        makeImportResult({ operationId: "op-1", imported: [importedBook] })
      )
      await runPromise
    })

    expect(result.current).toEqual({
      pending: false,
      progress: null,
      summary: null,
      canCancel: false,
      cancelling: false,
    })
  })

  it("ignores concurrent starts, handles cancel recovery, and retains failure summary", async () => {
    const queryClient = createTestQueryClient()
    let emitEvent!: Parameters<FakeLibrary["pickAndImportBooks"]>[0]
    let resolveImport!: (result: ReturnType<typeof makeImportResult>) => void

    const fakeLibrary = new FakeLibrary()
      .setCancelResult(false)
      .setImportHandler(
        (emit) =>
          new Promise((resolve) => {
            emitEvent = emit
            resolveImport = resolve
          })
      )
    const importSpy = vi.spyOn(fakeLibrary, "pickAndImportBooks")
    setPreviewLibrary(fakeLibrary)

    const { result } = renderHook(() => useBookImportState())

    let runPromise!: Promise<void>
    act(() => {
      runPromise = startBookImport(queryClient)
    })
    await act(async () => {
      await startBookImport(queryClient)
    })
    expect(importSpy).toHaveBeenCalledTimes(1)

    act(() => {
      emitEvent({ stage: "started", operationId: "op-2", total: 1 })
    })

    // Backend returns false -> cancelling reverts to false so user can retry
    await act(async () => {
      await cancelActiveBookImport()
    })
    expect(result.current.cancelling).toBe(false)

    fakeLibrary.setCancelResult(true)
    await act(async () => {
      await cancelActiveBookImport()
    })
    expect(result.current.cancelling).toBe(true)

    const failureResult = makeImportResult({
      operationId: "op-2",
      failed: [
        {
          fileName: "broken.pdf",
          error: makeCommandErrorPayload({ key: "library.book.invalid_file" }),
        },
      ],
      summaryMessage: "1 failed",
    })
    await act(async () => {
      resolveImport(failureResult)
      await runPromise
    })

    expect(result.current.summary).toEqual(failureResult)
    expect(result.current.cancelling).toBe(false)
  })

  it("discards stale events from an older import after resetBookImportState", async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries")
    let emitEvent!: Parameters<FakeLibrary["pickAndImportBooks"]>[0]
    let resolveImport!: (result: ReturnType<typeof makeImportResult>) => void

    const fakeLibrary = new FakeLibrary().setImportHandler(
      (emit) =>
        new Promise((resolve) => {
          emitEvent = emit
          resolveImport = resolve
        })
    )
    setPreviewLibrary(fakeLibrary)

    const { result } = renderHook(() => useBookImportState())

    let runPromise!: Promise<void>
    act(() => {
      runPromise = startBookImport(queryClient)
    })

    act(() => {
      resetBookImportState()
    })
    expect(result.current.pending).toBe(false)

    await act(async () => {
      emitEvent({
        stage: "itemSucceeded",
        operationId: "stale-op",
        index: 0,
        total: 1,
        book: makeBook(),
      })
      resolveImport(
        makeImportResult({
          operationId: "stale-op",
          skipped: ["dup.epub"],
          summaryMessage: "1 skipped",
        })
      )
      await runPromise
    })

    expect(result.current.summary).toBeNull()
    expect(invalidateSpy).not.toHaveBeenCalled()
  })
})
