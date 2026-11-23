import type { TagTreeNode } from "@/lib/types"
import { translateTagName, type Locale } from "@/lib/i18n"

/**
 * Keeps the nodes whose name contains the query, plus their ancestors so every match stays in place.
 * A matching node keeps its whole subtree. An empty query returns the tree unchanged.
 */
export function filterTagTree(
  nodes: TagTreeNode[],
  query: string,
  locale?: Locale
): TagTreeNode[] {
  const keyword = query.trim().toLowerCase()
  if (keyword === "") return nodes
  return nodes.flatMap((node) => {
    const translatedName = translateTagName(node.name, locale).toLowerCase()
    if (
      node.name.toLowerCase().includes(keyword) ||
      translatedName.includes(keyword)
    )
      return [node]
    const children = filterTagTree(node.children, keyword, locale)
    return children.length > 0 ? [{ ...node, children }] : []
  })
}
