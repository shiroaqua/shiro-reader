import { describe, expect, it, vi } from "vitest"
import { setPreviewLibrary } from "@shiro/ui-shared/api/library"
import {
  TagManageCommitError,
  addPendingCreates,
  applyPendingMoves,
  collectCascadeDeletedIds,
  collectSelectedRoots,
  commitTagChanges,
  countTagChanges,
  prepareChanges,
  resolveParentAfterPartialCommit,
  sortPendingCreates,
  type PendingCreate,
  type PendingDelete,
} from "@shiro/ui-shared/features/tags/tag-manage-model"
import { makeTag, makeTagNode } from "../../support/builders"
import { FakeLibrary } from "../../support/fake-library"

function draft(
  draftId: string,
  name: string,
  parentId: string | null = null
): PendingCreate {
  return {
    draftId,
    name,
    parent_id: parentId,
    color: "#75a9cc",
    allows_value: false,
    value_regex: null,
  }
}

describe("tag-manage-model", () => {
  describe("pending create and move ordering", () => {
    it("orders draft parents before draft children and terminates on cyclic draft links", () => {
      const child = draft("d-child", "Child", "d-parent")
      const parent = draft("d-parent", "Parent", null)

      expect(sortPendingCreates([child, parent]).map((c) => c.draftId)).toEqual([
        "d-parent",
        "d-child",
      ])

      const cycA = draft("cyc-a", "A", "cyc-b")
      const cycB = draft("cyc-b", "B", "cyc-a")
      expect(sortPendingCreates([cycA, cycB])).toHaveLength(2)
    })

    it("attaches pending creates to the cloned tree without mutating the server tree", () => {
      const root = makeTagNode({ id: "root", name: "Root" })
      const tree = [root]

      const nextTree = addPendingCreates(tree, [
        draft("d-grand", "Grandchild", "d-child"),
        draft("d-child", "Child", "root"),
      ])

      expect(root.children).toHaveLength(0)
      expect(nextTree[0]!.children[0]!.id).toBe("d-child")
      expect(nextTree[0]!.children[0]!.children[0]!.id).toBe("d-grand")
    })

    it("defers moving an ancestor under a former descendant until that descendant moves out", () => {
      const grand = makeTagNode({ id: "grand", name: "Grand", parent_id: "child" })
      const child = makeTagNode({
        id: "child",
        name: "Child",
        parent_id: "root",
        children: [grand],
      })
      const root = makeTagNode({
        id: "root",
        name: "Root",
        children: [child],
      })

      // Move `root` under `child` (blocked while `child` is still inside `root`),
      // and move `child` to top-level (null).
      const result = applyPendingMoves(
        [root],
        new Map<string, string | null>([
          ["root", "child"],
          ["child", null],
        ])
      )

      expect(result).toHaveLength(1)
      expect(result[0]!.id).toBe("child")
      expect(result[0]!.parent_id).toBeNull()
      expect(result[0]!.children.map((node) => node.id)).toEqual([
        "grand",
        "root",
      ])
    })
  })

  describe("selection, cascade, and partial-commit parent resolution", () => {
    it("collects topmost selected roots and cascade-deleted subtree ids", () => {
      const leaf = makeTagNode({ id: "leaf", name: "Leaf", parent_id: "branch" })
      const branch = makeTagNode({
        id: "branch",
        name: "Branch",
        parent_id: "root",
        children: [leaf],
      })
      const root = makeTagNode({
        id: "root",
        name: "Root",
        children: [branch],
      })

      const selectedRoots = collectSelectedRoots(
        [root],
        new Set(["root", "leaf"])
      )
      expect(selectedRoots.map((node) => node.id)).toEqual(["root"])

      const cascadeIds = collectCascadeDeletedIds(
        [root],
        new Map<string, PendingDelete>([["branch", { strategy: "CASCADE" }]])
      )
      expect([...cascadeIds]).toEqual(["branch", "leaf"])
    })

    it("walks PROMOTE-deleted ancestors and stops on CASCADE or missing parent", () => {
      const child = makeTagNode({
        id: "child",
        name: "Child",
        parent_id: "parent",
      })
      const parent = makeTagNode({
        id: "parent",
        name: "Parent",
        parent_id: "grand",
        children: [child],
      })
      const grand = makeTagNode({
        id: "grand",
        name: "Grand",
        parent_id: null,
        children: [parent],
      })

      const pendingDeletes = new Map<string, PendingDelete>([
        ["parent", { strategy: "PROMOTE" }],
        ["grand", { strategy: "CASCADE" }],
      ])

      // Only `parent` completed (PROMOTE) -> resolves to `grand`
      expect(
        resolveParentAfterPartialCommit(
          "parent",
          new Set(["parent"]),
          pendingDeletes,
          [],
          [grand]
        )
      ).toBe("grand")

      // Both `parent` and `grand` (CASCADE) completed -> resolves to null
      expect(
        resolveParentAfterPartialCommit(
          "parent",
          new Set(["parent", "grand"]),
          pendingDeletes,
          [],
          [grand]
        )
      ).toBeNull()
    })
  })

  describe("prepareChanges", () => {
    it("discards edits to deleted tags, promotes children across PROMOTE deletes, and defers deletes protecting moved descendants", () => {
      const keptChild = makeTagNode({
        id: "kept-child",
        name: "Kept",
        parent_id: "promoted-parent",
      })
      const promotedParent = makeTagNode({
        id: "promoted-parent",
        name: "OldParent",
        parent_id: null,
        children: [keptChild],
      })
      const goneParent = makeTagNode({ id: "gone", name: "Gone" })
      const serverTree = [promotedParent, goneParent]

      // Preview tree reflects `kept-child` moved to top level
      const previewTree = addPendingCreates(
        applyPendingMoves(
          [
            makeTagNode({
              ...promotedParent,
              children: [{ ...keptChild }],
            }),
            { ...goneParent },
          ],
          new Map([["kept-child", null]])
        ),
        [draft("d-new", "NewTag", "promoted-parent"), draft("d-del", "Temp")]
      )

      const changes = prepareChanges(
        serverTree,
        previewTree,
        [draft("d-new", "NewTag", "promoted-parent"), draft("d-del", "Temp")],
        new Map<string, string | null>([
          ["kept-child", null],
          ["gone", "promoted-parent"],
        ]),
        new Map([
          ["kept-child", "RenamedKept"],
          ["gone", "IgnoredRename"],
        ]),
        new Map([["promoted-parent", "#ff0000"]]),
        new Map<string, PendingDelete>([
          ["promoted-parent", { strategy: "PROMOTE" }],
          ["gone", { strategy: "CASCADE" }],
          ["d-del", { strategy: "CASCADE" }],
        ])
      )

      // `gone` runs early (no moved kept descendant), `promoted-parent` runs late (protects moved `kept-child`)
      expect(changes.earlyDeletes).toEqual([["gone", "CASCADE"]])
      expect(changes.lateDeletes).toEqual([["promoted-parent", "PROMOTE"]])
      // `d-new` resolves through PROMOTE-deleted `promoted-parent` to null; `d-del` dropped
      expect(changes.creates.map((c) => [c.draftId, c.parent_id])).toEqual([
        ["d-new", null],
      ])
      expect(changes.moves).toEqual([["kept-child", null]])
      expect(changes.renames).toEqual([["kept-child", "RenamedKept"]])
      expect(changes.recolors).toEqual([])
      expect(countTagChanges(changes)).toBe(5)
    })

    it("explicitly deletes a server tag that was dragged under a cascade-deleted parent", () => {
      const movedChild = makeTagNode({ id: "mover", name: "Mover" })
      const doomedParent = makeTagNode({ id: "doomed", name: "Doomed" })
      const serverTree = [movedChild, doomedParent]

      const previewTree = applyPendingMoves(
        [makeTagNode(movedChild), makeTagNode(doomedParent)],
        new Map([["mover", "doomed"]])
      )

      const changes = prepareChanges(
        serverTree,
        previewTree,
        [],
        new Map([["mover", "doomed"]]),
        new Map(),
        new Map(),
        new Map([["doomed", { strategy: "CASCADE" }]])
      )

      expect(changes.moves).toEqual([])
      expect(changes.earlyDeletes).toContainEqual(["mover", "CASCADE"])
      expect(changes.earlyDeletes).toContainEqual(["doomed", "CASCADE"])
    })
  })

  describe("commitTagChanges", () => {
    it("executes operations in phase order and remaps draft parent ids", async () => {
      const fakeLibrary = new FakeLibrary().seedTags([
        makeTagNode({ id: "old", name: "Old" }),
        makeTagNode({ id: "movable", name: "Movable" }),
      ])
      const callOrder: string[] = []
      const defineSpy = vi
        .spyOn(fakeLibrary, "tagDefine")
        .mockImplementation(async (def) => {
          callOrder.push(`define:${def.name}:${def.parent_id ?? "root"}`)
          return makeTag({ ...def, id: `saved-${def.name}` })
        })
      vi.spyOn(fakeLibrary, "tagUndefine").mockImplementation(async (id) => {
        callOrder.push(`delete:${id}`)
      })
      vi.spyOn(fakeLibrary, "tagMove").mockImplementation(async (id, p) => {
        callOrder.push(`move:${id}->${p}`)
      })
      vi.spyOn(fakeLibrary, "tagRename").mockImplementation(async (id, name) => {
        callOrder.push(`rename:${id}=${name}`)
      })
      vi.spyOn(fakeLibrary, "tagRecolor").mockImplementation(async (id, c) => {
        callOrder.push(`recolor:${id}=${c}`)
      })
      setPreviewLibrary(fakeLibrary)

      await commitTagChanges({
        earlyDeletes: [["old", "CASCADE"]],
        creates: [
          draft("draft-1", "ParentDraft"),
          draft("draft-2", "ChildDraft", "draft-1"),
        ],
        moves: [["movable", "draft-1"]],
        lateDeletes: [],
        renames: [["movable", "RenamedMovable"]],
        recolors: [["movable", "#a4bd69"]],
      })

      expect(defineSpy).toHaveBeenCalledTimes(2)
      expect(callOrder).toEqual([
        "delete:old",
        "define:ParentDraft:root",
        "define:ChildDraft:saved-ParentDraft",
        "move:movable->saved-ParentDraft",
        "rename:movable=RenamedMovable",
        "recolor:movable=#a4bd69",
      ])
    })

    it("throws TagManageCommitError carrying partial progress when a step fails", async () => {
      const fakeLibrary = new FakeLibrary().seedTags([
        makeTagNode({ id: "t1", name: "One" }),
      ])
      vi.spyOn(fakeLibrary, "tagRename").mockRejectedValueOnce(
        new Error("Rename conflict")
      )
      setPreviewLibrary(fakeLibrary)

      let caught: unknown
      try {
        await commitTagChanges({
          earlyDeletes: [],
          creates: [draft("draft-1", "Fresh")],
          moves: [],
          lateDeletes: [],
          renames: [["t1", "Duplicate"]],
          recolors: [["t1", "#d98282"]],
        })
      } catch (error) {
        caught = error
      }

      expect(caught).toBeInstanceOf(TagManageCommitError)
      const commitError = caught as TagManageCommitError
      expect(commitError.progress.createdIds.get("draft-1")).toBe("tag-1")
      expect(commitError.progress.renamedIds).toEqual([])
      expect(commitError.progress.recoloredIds).toEqual([])
    })
  })
})
