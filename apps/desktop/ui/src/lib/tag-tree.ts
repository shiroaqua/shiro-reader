import type { TagTreeNode } from "@/lib/types"

/** Ids of every tag that has children, used to expand a whole tree at once. */
export function collectBranchIds(nodes: TagTreeNode[]): string[] {
  return nodes.flatMap((node) =>
    node.children.length > 0
      ? [node.id, ...collectBranchIds(node.children)]
      : []
  )
}

export function findTag(
  nodes: TagTreeNode[],
  id: string
): TagTreeNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node
    const found = findTag(node.children, id)
    if (found) return found
  }
  return undefined
}

/** The node itself plus every descendant, used to select or clear an entire subtree. */
export function collectSubtreeIds(node: TagTreeNode): string[] {
  return [node.id, ...node.children.flatMap(collectSubtreeIds)]
}
