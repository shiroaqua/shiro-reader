import { describe, expect, it } from "vitest"
import { filterTagTree } from "@/lib/tag-filter"
import { collectBranchIds, collectSubtreeIds, findTag } from "@/lib/tag-tree"
import { makeTagNode } from "../../support/builders"

function createSampleTree() {
  const leafFantasy = makeTagNode({ id: "fantasy", name: "Fantasy" })
  const leafCyberpunk = makeTagNode({ id: "cyberpunk", name: "Cyberpunk" })
  const fiction = makeTagNode({
    id: "fiction",
    name: "Fiction",
    children: [leafFantasy, leafCyberpunk],
  })
  const author = makeTagNode({ id: "author", name: "tag.author" })
  const standalone = makeTagNode({ id: "standalone", name: "Standalone" })

  return {
    tree: [fiction, author, standalone],
    fiction,
    leafFantasy,
    leafCyberpunk,
    author,
    standalone,
  }
}

describe("tag-tree and tag-filter", () => {
  describe("tag-tree traversal helpers", () => {
    it("collects only branch ids, finds nested nodes, and collects full subtrees", () => {
      const { tree, fiction, leafCyberpunk } = createSampleTree()
      const deepChild = makeTagNode({ id: "sub", name: "Sub" })
      leafCyberpunk.children.push(deepChild)

      expect(collectBranchIds(tree)).toEqual(["fiction", "cyberpunk"])
      expect(findTag(tree, "sub")).toBe(deepChild)
      expect(findTag(tree, "missing")).toBeUndefined()
      expect(collectSubtreeIds(fiction)).toEqual([
        "fiction",
        "fantasy",
        "cyberpunk",
        "sub",
      ])
    })
  })

  describe("filterTagTree", () => {
    it("returns the original tree reference when query is empty or whitespace", () => {
      const { tree } = createSampleTree()

      expect(filterTagTree(tree, "")).toBe(tree)
      expect(filterTagTree(tree, "   ")).toBe(tree)
    })

    it("keeps ancestor path for nested matches while pruning non-matching siblings", () => {
      const { tree } = createSampleTree()

      const filtered = filterTagTree(tree, "cyber", "en")

      expect(filtered).toHaveLength(1)
      expect(filtered[0].id).toBe("fiction")
      expect(filtered[0].children.map((node) => node.id)).toEqual(["cyberpunk"])
    })

    it("retains the entire subtree when an ancestor itself matches", () => {
      const { tree } = createSampleTree()

      const filtered = filterTagTree(tree, "fict", "en")

      expect(filtered).toHaveLength(1)
      expect(filtered[0].children.map((node) => node.id)).toEqual([
        "fantasy",
        "cyberpunk",
      ])
    })

    it("matches built-in dotted tag keys by their localized display name", () => {
      const { tree } = createSampleTree()

      expect(filterTagTree(tree, "作者", "zh").map((node) => node.id)).toEqual([
        "author",
      ])
      expect(filterTagTree(tree, "auth", "en").map((node) => node.id)).toEqual([
        "author",
      ])
    })
  })
})
