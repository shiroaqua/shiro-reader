import { describe, expect, it } from "vitest"
import {
  CommandError,
  getBookCoverProtocolUrl,
  isCommandErrorPayload,
  listenRuntimeState,
  listenRustErrors,
  listenUploadState,
  setCoverUrlBase,
  tauriBridge,
} from "@/api/tauri-bridge"
import { getErrorNotifications } from "@/lib/error-notifications"
import {
  makeCommandErrorPayload,
  makeRustErrorEvent,
  makeUploadSnapshot,
  makeUploadTask,
  readyLocalState,
} from "../../support/builders"
import { tauriTestHost } from "../../support/mock-tauri"

describe("tauri-bridge", () => {
  describe("cover URL helpers", () => {
    it("normalizes trailing slashes, ignores blank bases, and encodes book IDs", () => {
      setCoverUrlBase("http://127.0.0.1:8080/covers")
      expect(getBookCoverProtocolUrl("book/1?a=1")).toBe(
        "http://127.0.0.1:8080/covers/book%2F1%3Fa%3D1"
      )

      setCoverUrlBase("   ")
      expect(getBookCoverProtocolUrl("abc")).toBe(
        "http://127.0.0.1:8080/covers/abc"
      )
    })
  })

  describe("error handling and CommandError", () => {
    it("recognizes valid CommandErrorPayload shapes and rejects incomplete values", () => {
      expect(isCommandErrorPayload(makeCommandErrorPayload())).toBe(true)
      expect(
        isCommandErrorPayload(makeCommandErrorPayload({ itemId: "book-1" }))
      ).toBe(true)
      expect(isCommandErrorPayload(null)).toBe(false)
      expect(isCommandErrorPayload({ key: "x" })).toBe(false)
      expect(
        isCommandErrorPayload({
          operationId: "op",
          errorId: "err",
          code: "4000",
          key: "k",
          itemId: null,
        })
      ).toBe(false)
    })

    it("wraps structured command errors into CommandError without emitting duplicate local notifications", async () => {
      const payload = makeCommandErrorPayload({ key: "library.book.not_found" })
      tauriTestHost.onInvoke("books_list", () => {
        throw payload
      })

      await expect(tauriBridge.booksList()).rejects.toThrow(CommandError)
      await expect(tauriBridge.booksList()).rejects.toMatchObject({ payload })
      expect(getErrorNotifications()).toHaveLength(0)
    })

    it("notifies unknown local error when invoke rejects with an unstructured error", async () => {
      tauriTestHost.onInvoke("books_list", () => {
        throw new Error("IPC pipe closed")
      })

      await expect(tauriBridge.booksList()).rejects.toThrow("IPC pipe closed")
      expect(getErrorNotifications()).toHaveLength(1)
      expect(getErrorNotifications()[0]!.translation).toEqual({
        source: "local",
        key: "local.common.error.unknown",
      })
    })
  })

  describe("command argument mapping and event listeners", () => {
    it("maps snake_case tag and binding fields to camelCase Tauri arguments", async () => {
      tauriTestHost.onInvoke("tag_define", () => ({
        id: "t1",
        name: "Author",
        parent_id: "root",
        color: "#000000",
        allows_value: true,
        value_regex: ".+",
      }))
      tauriTestHost.onInvoke("book_tag_bind", () => undefined)

      await tauriBridge.tagDefine({
        name: "Author",
        parent_id: "root",
        color: "#000000",
        allows_value: true,
        value_regex: ".+",
      })
      await tauriBridge.bookTagBind("b1", { tag_id: "t1", value: "Alice" })

      expect(tauriTestHost.invokeCalls("tag_define")[0]!.args).toEqual({
        name: "Author",
        parentId: "root",
        color: "#000000",
        allowsValue: true,
        valueRegex: ".+",
      })
      expect(tauriTestHost.invokeCalls("book_tag_bind")[0]!.args).toEqual({
        bookId: "b1",
        tagId: "t1",
        value: "Alice",
      })
    })

    it("invokes concurrent upload queue IPC commands with exact argument payloads", async () => {
      const snap = makeUploadSnapshot({
        revision: 2,
        concurrency: 6,
        tasks: [makeUploadTask({ id: 9, fileName: "novel.epub" })],
      })
      tauriTestHost.onInvoke("upload_get_state", () => snap)
      tauriTestHost.onInvoke("upload_pick_and_enqueue", () => snap)
      tauriTestHost.onInvoke("upload_enqueue_paths", () => snap)
      tauriTestHost.onInvoke("upload_retry_task", () => snap)
      tauriTestHost.onInvoke("upload_cancel_task", () => snap)
      tauriTestHost.onInvoke("upload_cancel_all", () => snap)
      tauriTestHost.onInvoke("upload_dismiss_task", () => snap)
      tauriTestHost.onInvoke("upload_dismiss_errors", () => snap)
      tauriTestHost.onInvoke("upload_set_concurrency", () => snap)

      expect(await tauriBridge.uploadGetState()).toEqual(snap)
      expect(await tauriBridge.uploadPickAndEnqueue()).toEqual(snap)
      expect(await tauriBridge.uploadEnqueuePaths(["/books/a.epub"])).toEqual(
        snap
      )
      expect(await tauriBridge.uploadRetryTask(9)).toEqual(snap)
      expect(await tauriBridge.uploadCancelTask(9)).toEqual(snap)
      expect(await tauriBridge.uploadCancelAll()).toEqual(snap)
      expect(await tauriBridge.uploadDismissTask(9)).toEqual(snap)
      expect(await tauriBridge.uploadDismissErrors()).toEqual(snap)
      expect(await tauriBridge.uploadSetConcurrency(6)).toEqual(snap)

      expect(tauriTestHost.invokeCalls("upload_enqueue_paths")[0]!.args).toEqual(
        { paths: ["/books/a.epub"] }
      )
      expect(tauriTestHost.invokeCalls("upload_retry_task")[0]!.args).toEqual({
        taskId: 9,
      })
      expect(
        tauriTestHost.invokeCalls("upload_set_concurrency")[0]!.args
      ).toEqual({ concurrency: 6 })
    })

    it("forwards Rust error, runtime state, and upload state events and honors unlisten", async () => {
      const receivedErrors: string[] = []
      const receivedPhases: string[] = []
      const receivedRevisions: number[] = []

      const unlistenErrors = await listenRustErrors((event) => {
        receivedErrors.push(event.errorId)
      })
      const unlistenState = await listenRuntimeState((state) => {
        receivedPhases.push(state.phase)
      })
      const unlistenUploads = await listenUploadState((snapshot) => {
        receivedRevisions.push(snapshot.revision)
      })

      tauriTestHost.emitRustError(makeRustErrorEvent({ errorId: "e-1" }))
      tauriTestHost.emitRuntimeState(readyLocalState({ phase: "ready" }))
      tauriTestHost.emitUploadState(makeUploadSnapshot({ revision: 5 }))

      unlistenErrors()
      unlistenState()
      unlistenUploads()

      tauriTestHost.emitRustError(makeRustErrorEvent({ errorId: "e-2" }))
      tauriTestHost.emitRuntimeState(
        readyLocalState({ phase: "startup_failed" })
      )
      tauriTestHost.emitUploadState(makeUploadSnapshot({ revision: 6 }))

      expect(receivedErrors).toEqual(["e-1"])
      expect(receivedPhases).toEqual(["ready"])
      expect(receivedRevisions).toEqual([5])
    })
  })
})
