import type { TagTreeNode } from "@/lib/types"

/**
 * Keeps the nodes whose name contains the query, plus their ancestors so every match stays in place.
 * A matching node keeps its whole subtree. An empty query returns the tree unchanged.
 */
export function filterTagTree(
  nodes: TagTreeNode[],
  query: string
): TagTreeNode[] {
  const keyword = query.trim().toLowerCase()
  if (keyword === "") return nodes
  return nodes.flatMap((node) => {
    if (node.name.toLowerCase().includes(keyword)) return [node]
    const children = filterTagTree(node.children, keyword)
    return children.length > 0 ? [{ ...node, children }] : []
  })
}
