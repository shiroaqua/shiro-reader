import { describe, expect, it } from "vitest"
import {
  CommandError,
  getBookCoverProtocolUrl,
  isCommandErrorPayload,
  listenRuntimeState,
  listenRustErrors,
  setCoverUrlBase,
  tauriBridge,
  type ImportProgressEvent,
} from "@/api/tauri-bridge"
import { getErrorNotifications } from "@/lib/error-notifications"
import {
  makeCommandErrorPayload,
  makeImportResult,
  makeRustErrorEvent,
  readyLocalState,
} from "../../support/builders"
import { FakeChannel, tauriTestHost } from "../../support/mock-tauri"

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
      expect(getErrorNotifications()[0].translation).toEqual({
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

      expect(tauriTestHost.invokeCalls("tag_define")[0].args).toEqual({
        name: "Author",
        parentId: "root",
        color: "#000000",
        allowsValue: true,
        valueRegex: ".+",
      })
      expect(tauriTestHost.invokeCalls("book_tag_bind")[0].args).toEqual({
        bookId: "b1",
        tagId: "t1",
        value: "Alice",
      })
    })

    it("streams import progress events over a Channel in pickAndImportBooks", async () => {
      const expectedResult = makeImportResult()
      tauriTestHost.onInvoke("pick_and_import_books", (args) => {
        const channel = args?.onEvent as FakeChannel<ImportProgressEvent>
        channel.onmessage({
          stage: "started",
          operationId: expectedResult.operationId,
          total: 1,
        })
        channel.onmessage({
          stage: "finished",
          operationId: expectedResult.operationId,
          cancelled: false,
          importedCount: 1,
          failedCount: 0,
          skippedCount: 0,
        })
        return expectedResult
      })

      const stages: string[] = []
      const result = await tauriBridge.pickAndImportBooks((event) => {
        stages.push(event.stage)
      })

      expect(result).toEqual(expectedResult)
      expect(stages).toEqual(["started", "finished"])
    })

    it("forwards Rust error and runtime state events and honors unlisten", async () => {
      const receivedErrors: string[] = []
      const receivedPhases: string[] = []

      const unlistenErrors = await listenRustErrors((event) => {
        receivedErrors.push(event.errorId)
      })
      const unlistenState = await listenRuntimeState((state) => {
        receivedPhases.push(state.phase)
      })

      tauriTestHost.emitRustError(makeRustErrorEvent({ errorId: "e-1" }))
      tauriTestHost.emitRuntimeState(readyLocalState({ phase: "ready" }))

      unlistenErrors()
      unlistenState()

      tauriTestHost.emitRustError(makeRustErrorEvent({ errorId: "e-2" }))
      tauriTestHost.emitRuntimeState(
        readyLocalState({ phase: "startup_failed" })
      )

      expect(receivedErrors).toEqual(["e-1"])
      expect(receivedPhases).toEqual(["ready"])
    })
  })
})
