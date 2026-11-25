import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { setPreviewLibrary } from "@shiro/ui-shared/api/library"
import {
  applyUploadSnapshot,
  cancelAllUploads,
  cancelUploadTask,
  closeMobileUploadPanel,
  dismissUploadErrors,
  dismissUploadTask,
  openMobileUploadPanel,
  pickAndEnqueueBooks,
  resetBookImportState,
  retryUploadTask,
  setUploadConcurrency,
  setUploadPanelExpanded,
  toggleUploadOptions,
  useBookImportState,
  useUploadQueueSync,
} from "@shiro/ui-shared/features/books/book-import-store"
import { booksQueryKey } from "@shiro/ui-shared/lib/query-keys"
import {
  makeRustErrorEvent,
  makeUploadSnapshot,
  makeUploadTask,
} from "../../support/builders"
import { FakeLibrary } from "../../support/fake-library"
import { createTestQueryClient } from "../../support/render"

describe("book-import-store", () => {
  it("filters out stale revisions and preserves monotonic snapshots", () => {
    const { result } = renderHook(() => useBookImportState())
    expect(result.current.upload.revision).toBe(0)

    act(() => {
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 5,
          concurrency: 6,
          tasks: [makeUploadTask({ id: 1, fileName: "a.epub" })],
        })
      )
    })
    expect(result.current.upload.revision).toBe(5)
    expect(result.current.upload.concurrency).toBe(6)

    // Out-of-order older revision must be ignored.
    act(() => {
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 3,
          concurrency: 2,
          tasks: [],
        })
      )
    })
    expect(result.current.upload.revision).toBe(5)
    expect(result.current.upload.concurrency).toBe(6)

    // Newer revision applies cleanly.
    act(() => {
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 6,
          concurrency: 8,
          tasks: [],
          visible: false,
        })
      )
    })
    expect(result.current.upload.revision).toBe(6)
    expect(result.current.upload.concurrency).toBe(8)
    expect(result.current.upload.visible).toBe(false)
  })

  it("throttles bookshelf invalidation across bursts and refreshes reliably across batches via completedCounter", async () => {
    vi.useFakeTimers()
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries")
    const fakeLibrary = new FakeLibrary()
    setPreviewLibrary(fakeLibrary)

    renderHook(() => useUploadQueueSync(queryClient, true))
    await act(async () => {
      await Promise.resolve()
    })

    // Batch 1: 2 books finish while 1 remains -> throttled to 1 invalidation after 150ms.
    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 1,
          visible: true,
          completedCounter: 1,
          stats: { total: 3, remaining: 2, active: 1, succeeded: 1, failed: 0 },
          tasks: [makeUploadTask({ id: 1 })],
        })
      )
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 2,
          visible: true,
          completedCounter: 2,
          stats: { total: 3, remaining: 1, active: 1, succeeded: 2, failed: 0 },
          tasks: [makeUploadTask({ id: 2 })],
        })
      )
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(0)

    act(() => {
      vi.advanceTimersByTime(150)
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(1)
    expect(invalidateSpy).toHaveBeenLastCalledWith({ queryKey: booksQueryKey })

    // Last item of Batch 1 completes (remaining === 0) -> flushes IMMEDIATELY without waiting 150ms.
    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 3,
          visible: true,
          completedCounter: 3,
          stats: { total: 3, remaining: 0, active: 0, succeeded: 3, failed: 0 },
          tasks: [],
        })
      )
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(2)

    // Panel hides & resets batch stats (succeeded returns to 0), keeping completedCounter = 3.
    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 4,
          visible: false,
          completedCounter: 3,
          stats: { total: 0, remaining: 0, active: 0, succeeded: 0, failed: 0 },
          tasks: [],
        })
      )
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(2)

    // Batch 2: single book completes -> completedCounter advances 3 -> 4 and immediately refreshes!
    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 5,
          visible: true,
          completedCounter: 4,
          stats: { total: 1, remaining: 0, active: 0, succeeded: 1, failed: 0 },
          tasks: [],
        })
      )
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(3)
  })

  it("guards picker re-entrancy only while dialog is open and forwards task/queue actions", async () => {
    let resolvePick!: (snap: ReturnType<typeof makeUploadSnapshot>) => void
    const fakeLibrary = new FakeLibrary().setPickHandler(
      () =>
        new Promise((resolve) => {
          resolvePick = resolve
        })
    )
    const pickSpy = vi.spyOn(fakeLibrary, "uploadPickAndEnqueue")
    setPreviewLibrary(fakeLibrary)

    const { result } = renderHook(() => useBookImportState())

    let p1!: Promise<void>
    act(() => {
      p1 = pickAndEnqueueBooks()
    })
    expect(result.current.pickerOpening).toBe(true)

    // Second click while native picker is still open is ignored.
    await act(async () => {
      await pickAndEnqueueBooks()
    })
    expect(pickSpy).toHaveBeenCalledTimes(1)

    await act(async () => {
      resolvePick(
        makeUploadSnapshot({
          revision: 1,
          tasks: [
            makeUploadTask({
              id: 1,
              fileName: "a.epub",
              status: {
                state: "failed",
                error: makeRustErrorEvent({ message: "Bad file" }),
              },
            }),
            makeUploadTask({ id: 2, fileName: "b.epub" }),
          ],
        })
      )
      await p1
    })
    expect(result.current.pickerOpening).toBe(false)
    expect(result.current.upload.tasks).toHaveLength(2)

    // View toggles and queue commands work cleanly.
    act(() => {
      toggleUploadOptions()
    })
    expect(result.current.optionsOpen).toBe(true)

    act(() => {
      setUploadPanelExpanded(false)
    })
    expect(result.current.expanded).toBe(false)
    expect(result.current.optionsOpen).toBe(false)

    await act(async () => {
      await setUploadConcurrency(12)
      await retryUploadTask(1)
      await cancelUploadTask(2)
      await cancelAllUploads()
      await dismissUploadTask(1)
      await dismissUploadErrors()
    })
    expect(result.current.upload.concurrency).toBe(12)

    act(() => {
      resetBookImportState()
    })
    expect(result.current.upload.revision).toBe(0)
    expect(result.current.upload.visible).toBe(false)
  })

  it("resets upload state when useUploadQueueSync transitions from enabled to disabled", async () => {
    const queryClient = createTestQueryClient()
    const fakeLibrary = new FakeLibrary()
    setPreviewLibrary(fakeLibrary)

    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) => {
        useUploadQueueSync(queryClient, enabled)
        return useBookImportState()
      },
      { initialProps: { enabled: true } }
    )
    await act(async () => {
      await Promise.resolve()
    })

    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 3,
          visible: true,
          tasks: [makeUploadTask({ id: 7, fileName: "stale.epub" })],
        })
      )
    })
    expect(result.current.upload.visible).toBe(true)
    expect(result.current.upload.tasks).toHaveLength(1)

    rerender({ enabled: false })
    expect(result.current.upload.visible).toBe(false)
    expect(result.current.upload.tasks).toHaveLength(0)
    expect(result.current.upload.revision).toBe(0)
  })

  it("invalidates bookshelf when initial hydration already reflects a newly completed upload", async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries")
    const fakeLibrary = new FakeLibrary().setUploadSnapshot(
      makeUploadSnapshot({
        revision: 1,
        visible: false,
        completedCounter: 1,
        stats: { total: 0, remaining: 0, active: 0, succeeded: 0, failed: 0 },
        tasks: [],
      })
    )
    setPreviewLibrary(fakeLibrary)

    renderHook(() => useUploadQueueSync(queryClient, true))
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(invalidateSpy).toHaveBeenCalledTimes(1)
    expect(invalidateSpy).toHaveBeenLastCalledWith({ queryKey: booksQueryKey })
  })

  it("flushes pending throttled bookshelf invalidation on unmount", async () => {
    vi.useFakeTimers()
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries")
    const fakeLibrary = new FakeLibrary()
    setPreviewLibrary(fakeLibrary)

    const { unmount } = renderHook(() => useUploadQueueSync(queryClient, true))
    await act(async () => {
      await Promise.resolve()
    })

    act(() => {
      fakeLibrary.emitUploadSnapshot(
        makeUploadSnapshot({
          revision: 1,
          visible: true,
          completedCounter: 1,
          stats: { total: 2, remaining: 1, active: 1, succeeded: 1, failed: 0 },
          tasks: [makeUploadTask({ id: 2 })],
        })
      )
    })
    expect(invalidateSpy).toHaveBeenCalledTimes(0)

    unmount()
    expect(invalidateSpy).toHaveBeenCalledTimes(1)
    expect(invalidateSpy).toHaveBeenLastCalledWith({ queryKey: booksQueryKey })
  })

  it("keeps mobileOpen closed on snapshot updates and auto-closes when snapshot hides", async () => {
    const fakeLibrary = new FakeLibrary()
    setPreviewLibrary(fakeLibrary)

    const { result } = renderHook(() => useBookImportState())
    expect(result.current.mobileOpen).toBe(false)

    // Receiving visible upload snapshots never opens the mobile full-screen UI automatically.
    act(() => {
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 1,
          visible: true,
          tasks: [makeUploadTask({ id: 1, fileName: "mobile.epub" })],
        })
      )
    })
    expect(result.current.mobileOpen).toBe(false)

    // Opening via notification tap sets mobileOpen and notifies backend.
    act(() => {
      openMobileUploadPanel()
    })
    expect(result.current.mobileOpen).toBe(true)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true])

    // Explicit close resets mobileOpen + optionsOpen and notifies backend.
    act(() => {
      toggleUploadOptions()
      closeMobileUploadPanel()
    })
    expect(result.current.mobileOpen).toBe(false)
    expect(result.current.optionsOpen).toBe(false)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true, false])

    // If opened again and backend clears/hides the queue, mobileOpen closes automatically.
    act(() => {
      openMobileUploadPanel()
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 2,
          visible: false,
          tasks: [],
        })
      )
    })
    expect(result.current.mobileOpen).toBe(false)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true, false, true, false])
  })

  it("notifies backend mobile UI closed when useUploadQueueSync disables while mobileOpen is true", async () => {
    const queryClient = createTestQueryClient()
    const fakeLibrary = new FakeLibrary()
    setPreviewLibrary(fakeLibrary)

    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) => {
        useUploadQueueSync(queryClient, enabled)
        return useBookImportState()
      },
      { initialProps: { enabled: true } }
    )
    await act(async () => {
      await Promise.resolve()
    })

    act(() => {
      applyUploadSnapshot(
        makeUploadSnapshot({
          revision: 1,
          visible: true,
          tasks: [makeUploadTask({ id: 1, fileName: "active.epub" })],
        })
      )
      openMobileUploadPanel()
    })
    expect(result.current.mobileOpen).toBe(true)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true])

    rerender({ enabled: false })
    expect(result.current.mobileOpen).toBe(false)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true, false])
  })

  it("subscribes to upload-open-ui and consumes pending open-ui ONLY when enableMobileOpenUi is true", async () => {
    const queryClient = createTestQueryClient()
    const fakeLibrary = new FakeLibrary().setPendingOpenUi(true)
    const consumeSpy = vi.spyOn(fakeLibrary, "uploadConsumePendingOpenUi")
    const listenOpenSpy = vi.spyOn(fakeLibrary, "listenUploadOpenUi")
    setPreviewLibrary(fakeLibrary)

    const { result, unmount } = renderHook(() => {
      useUploadQueueSync(queryClient, true)
      return useBookImportState()
    })
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(listenOpenSpy).not.toHaveBeenCalled()
    expect(consumeSpy).not.toHaveBeenCalled()
    expect(result.current.mobileOpen).toBe(false)

    act(() => {
      fakeLibrary.emitUploadOpenUi()
    })
    expect(result.current.mobileOpen).toBe(false)
    unmount()

    const { result: mobileResult } = renderHook(() => {
      useUploadQueueSync(queryClient, true, { enableMobileOpenUi: true })
      return useBookImportState()
    })
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(listenOpenSpy).toHaveBeenCalledTimes(1)
    expect(consumeSpy).toHaveBeenCalledTimes(1)
    expect(mobileResult.current.mobileOpen).toBe(true)
    expect(fakeLibrary.mobileUiOpenHistory).toEqual([true])
  })
})
