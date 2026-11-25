import { library } from "@shiro/ui-shared/api/library"
import { collectSubtreeIds, findTag } from "@shiro/ui-shared/lib/tag-tree"
import type {
  TagDefinitionInput,
  TagTreeNode,
  UndefineTagStrategy,
} from "@shiro/ui-shared/lib/types"

export async function commitTagChanges(changes: TagChanges) {
  const progress: CommitProgress = {
    createdIds: new Map(),
    movedIds: [],
    renamedIds: [],
    recoloredIds: [],
    deletedIds: [],
  }
  try {
    for (const [id, strategy] of changes.earlyDeletes) {
      await library.tag.undefine(id, strategy)
      progress.deletedIds.push(id)
    }
    for (const create of changes.creates) {
      const { draftId, ...definition } = create
      const parentId = definition.parent_id
        ? (progress.createdIds.get(definition.parent_id) ??
          definition.parent_id)
        : null
      const savedTag = await library.tag.define({
        ...definition,
        parent_id: parentId,
      })
      progress.createdIds.set(draftId, savedTag.id)
    }
    for (const [id, parentId] of changes.moves) {
      await library.tag.move(
        id,
        parentId ? (progress.createdIds.get(parentId) ?? parentId) : null
      )
      progress.movedIds.push(id)
    }
    for (const [id, strategy] of changes.lateDeletes) {
      await library.tag.undefine(id, strategy)
      progress.deletedIds.push(id)
    }
    for (const [id, name] of changes.renames) {
      await library.tag.rename(id, name)
      progress.renamedIds.push(id)
    }
    for (const [id, color] of changes.recolors) {
      await library.tag.recolor(id, color)
      progress.recoloredIds.push(id)
    }
  } catch (error) {
    throw new TagManageCommitError(error, progress)
  }
}

export type PendingDelete = { strategy: UndefineTagStrategy }

export type PendingCreate = TagDefinitionInput & { draftId: string }

export type TagChanges = {
  earlyDeletes: [string, UndefineTagStrategy][]
  creates: PendingCreate[]
  moves: [string, string | null][]
  lateDeletes: [string, UndefineTagStrategy][]
  renames: [string, string][]
  recolors: [string, string][]
}

export function countTagChanges(changes: TagChanges): number {
  return (
    changes.earlyDeletes.length +
    changes.creates.length +
    changes.moves.length +
    changes.lateDeletes.length +
    changes.renames.length +
    changes.recolors.length
  )
}

export type CommitProgress = {
  createdIds: Map<string, string>
  movedIds: string[]
  renamedIds: string[]
  recoloredIds: string[]
  deletedIds: string[]
}

export class TagManageCommitError extends Error {
  readonly progress: CommitProgress

  constructor(originalError: unknown, progress: CommitProgress) {
    super("Failed to commit tag changes", { cause: originalError })
    this.progress = progress
  }
}

export function collectSelectedRoots(
  nodes: TagTreeNode[],
  selected: ReadonlySet<string>
): TagTreeNode[] {
  const roots: TagTreeNode[] = []
  const visit = (items: TagTreeNode[], hasSelectedAncestor: boolean) => {
    for (const node of items) {
      const isSelected = selected.has(node.id)
      if (isSelected && !hasSelectedAncestor) roots.push(node)
      visit(node.children, hasSelectedAncestor || isSelected)
    }
  }
  visit(nodes, false)
  return roots
}

export function collectTagMetadata(nodes: TagTreeNode[]) {
  const metadata = new Map<string, { depth: number; ancestors: string[] }>()
  const visit = (items: TagTreeNode[], ancestors: string[]) => {
    for (const node of items) {
      metadata.set(node.id, { depth: ancestors.length, ancestors })
      visit(node.children, [...ancestors, node.id])
    }
  }
  visit(nodes, [])
  return metadata
}

function cloneTagTree(nodes: readonly TagTreeNode[]): TagTreeNode[] {
  return nodes.map((node) => ({
    ...node,
    children: cloneTagTree(node.children),
  }))
}

/** Orders draft creates so any draft parent always precedes its draft children. */
export function sortPendingCreates(
  pendingCreates: readonly PendingCreate[]
): PendingCreate[] {
  const byId = new Map(pendingCreates.map((item) => [item.draftId, item]))
  const ordered: PendingCreate[] = []
  const visited = new Set<string>()
  const visiting = new Set<string>()

  const visit = (item: PendingCreate) => {
    if (visited.has(item.draftId) || visiting.has(item.draftId)) return
    visiting.add(item.draftId)
    if (item.parent_id) {
      const parent = byId.get(item.parent_id)
      if (parent) visit(parent)
    }
    visiting.delete(item.draftId)
    visited.add(item.draftId)
    ordered.push(item)
  }

  for (const item of pendingCreates) visit(item)
  return ordered
}

export function addPendingCreates(
  serverTree: TagTreeNode[],
  pendingCreates: PendingCreate[]
): TagTreeNode[] {
  const byId = new Map<string, TagTreeNode>()
  const cloneTree = (nodes: TagTreeNode[]): TagTreeNode[] =>
    nodes.map((node) => {
      const clone: TagTreeNode = { ...node, children: [] }
      byId.set(clone.id, clone)
      clone.children = cloneTree(node.children)
      return clone
    })
  const tree = cloneTree(serverTree)

  for (const create of sortPendingCreates(pendingCreates)) {
    const node: TagTreeNode = {
      id: create.draftId,
      name: create.name,
      parent_id: create.parent_id,
      color: create.color,
      allows_value: create.allows_value,
      value_regex: create.value_regex,
      created_at: "",
      updated_at: "",
      children: [],
    }
    const parent = create.parent_id ? byId.get(create.parent_id) : undefined
    if (parent) parent.children.push(node)
    else tree.push(node)
    byId.set(node.id, node)
  }

  return tree
}

function detachNode(nodes: TagTreeNode[], id: string): TagTreeNode | undefined {
  const index = nodes.findIndex((node) => node.id === id)
  if (index !== -1) return nodes.splice(index, 1)[0]
  for (const node of nodes) {
    const found = detachNode(node.children, id)
    if (found) return found
  }
  return undefined
}

/**
 * Applies moves in dependency order so moving an ancestor under a former
 * descendant waits until that descendant has moved out.
 */
function orderAndApplyMoves(
  baseTree: TagTreeNode[],
  moves: readonly [string, string | null][],
  mutateBase: boolean
): { orderedMoves: [string, string | null][]; tree: TagTreeNode[] } {
  const workingTree = mutateBase ? baseTree : cloneTagTree(baseTree)
  const orderedMoves: [string, string | null][] = []
  let remaining = [...moves]

  while (remaining.length > 0) {
    const deferred: [string, string | null][] = []
    let progressed = false

    for (const move of remaining) {
      const [id, parentId] = move
      const node = findTag(workingTree, id)
      const parent = parentId ? findTag(workingTree, parentId) : null
      if (!node || (parentId && !parent)) continue
      if (parentId && collectSubtreeIds(node).includes(parentId)) {
        deferred.push(move)
        continue
      }
      detachNode(workingTree, id)
      node.parent_id = parentId
      if (parent) parent.children.push(node)
      else workingTree.push(node)
      orderedMoves.push(move)
      progressed = true
    }

    if (!progressed) break
    remaining = deferred
  }

  return { orderedMoves, tree: workingTree }
}

export function applyPendingMoves(
  tree: TagTreeNode[],
  moves: ReadonlyMap<string, string | null>
): TagTreeNode[] {
  return orderAndApplyMoves(tree, [...moves], true).tree
}

export function collectCascadeDeletedIds(
  tree: TagTreeNode[],
  pendingDeletes: ReadonlyMap<string, PendingDelete>
): Set<string> {
  const deletedIds = new Set<string>()
  for (const [id, deletion] of pendingDeletes) {
    if (deletion.strategy !== "CASCADE") continue
    const tag = findTag(tree, id)
    if (tag) {
      for (const descendantId of collectSubtreeIds(tag)) {
        deletedIds.add(descendantId)
      }
    }
  }
  return deletedIds
}

export function resolveParentAfterPartialCommit(
  initialParentId: string | null,
  completedDeleteIds: ReadonlySet<string>,
  pendingDeletes: ReadonlyMap<string, PendingDelete>,
  pendingCreates: PendingCreate[],
  tree: TagTreeNode[]
): string | null {
  let parentId = initialParentId
  const visited = new Set<string>()
  while (parentId && completedDeleteIds.has(parentId)) {
    if (visited.has(parentId)) return null
    visited.add(parentId)
    if (pendingDeletes.get(parentId)?.strategy !== "PROMOTE") return null
    const pendingParent = pendingCreates.find(
      (create) => create.draftId === parentId
    )
    parentId = pendingParent
      ? pendingParent.parent_id
      : (findTag(tree, parentId)?.parent_id ?? null)
  }
  return parentId
}

export function prepareChanges(
  serverTree: TagTreeNode[],
  tree: TagTreeNode[],
  pendingCreates: PendingCreate[],
  pendingMoves: ReadonlyMap<string, string | null>,
  pendingRenames: ReadonlyMap<string, string>,
  pendingRecolors: ReadonlyMap<string, string>,
  pendingDeletes: ReadonlyMap<string, PendingDelete>
): TagChanges {
  const cascadeDeletedIds = collectCascadeDeletedIds(tree, pendingDeletes)
  const pendingCreatesById = new Map(
    pendingCreates.map((create) => [create.draftId, create])
  )

  const resolveParentId = (initialParentId: string | null) => {
    let parentId = initialParentId
    const visited = new Set<string>()
    while (parentId && pendingDeletes.has(parentId)) {
      if (visited.has(parentId)) return null
      visited.add(parentId)
      if (pendingDeletes.get(parentId)?.strategy === "CASCADE") return null
      const pendingParent = pendingCreatesById.get(parentId)
      parentId = pendingParent
        ? pendingParent.parent_id
        : (findTag(tree, parentId)?.parent_id ?? null)
    }
    return parentId
  }

  const creates = sortPendingCreates(
    pendingCreates
      .filter(
        (create) =>
          !pendingDeletes.has(create.draftId) &&
          !cascadeDeletedIds.has(create.draftId)
      )
      .map((create) => ({
        ...create,
        parent_id: resolveParentId(create.parent_id),
      }))
  )

  const renames = [...pendingRenames].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const recolors = [...pendingRecolors].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const rawMoves = [...pendingMoves]
    .filter(([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id))
    .map(
      ([id, parentId]) =>
        [id, resolveParentId(parentId)] as [string, string | null]
    )
    .filter(([id, parentId]) => findTag(serverTree, id)?.parent_id !== parentId)

  const { orderedMoves: moves, tree: preDeleteTree } = orderAndApplyMoves(
    addPendingCreates(serverTree, creates),
    rawMoves,
    true
  )
  const preDeleteMetadata = collectTagMetadata(preDeleteTree)
  const pendingCreateIds = new Set(
    pendingCreates.map((create) => create.draftId)
  )

  const deleteMap = new Map<string, UndefineTagStrategy>()
  for (const [id, deletion] of pendingDeletes) {
    if (!pendingCreateIds.has(id)) {
      deleteMap.set(id, deletion.strategy)
    }
  }
  // A server tag dragged under a cascade-deleted parent is not moved on the server,
  // so include it in deletes explicitly if no cascade-deleted ancestor covers it there.
  for (const id of cascadeDeletedIds) {
    if (pendingCreateIds.has(id) || deleteMap.has(id)) continue
    const coveredOnServer = (
      preDeleteMetadata.get(id)?.ancestors ?? []
    ).some((ancestorId) => deleteMap.get(ancestorId) === "CASCADE")
    if (!coveredOnServer) {
      deleteMap.set(id, "CASCADE")
    }
  }

  const allDeletes = [...deleteMap]
    .filter(
      ([id]) =>
        !(preDeleteMetadata.get(id)?.ancestors ?? []).some(
          (ancestorId) => deleteMap.get(ancestorId) === "CASCADE"
        )
    )
    .sort(([leftId, leftStrategy], [rightId, rightStrategy]) => {
      const depthDiff =
        (preDeleteMetadata.get(rightId)?.depth ?? 0) -
        (preDeleteMetadata.get(leftId)?.depth ?? 0)
      if (depthDiff !== 0) return depthDiff
      // Run non-promoting deletes before PROMOTE deletes at the same depth to avoid sibling collisions.
      const leftRank = leftStrategy === "PROMOTE" ? 1 : 0
      const rightRank = rightStrategy === "PROMOTE" ? 1 : 0
      return leftRank - rightRank
    })

  // Defer deleting any server tag whose subtree contains a kept node being moved out.
  const movedNodeIds = new Set(moves.map(([id]) => id))
  const earlyDeletes: [string, UndefineTagStrategy][] = []
  const lateDeletes: [string, UndefineTagStrategy][] = []
  for (const entry of allDeletes) {
    const [id] = entry
    const serverNode = findTag(serverTree, id)
    const protectsMovedDescendant =
      serverNode !== undefined &&
      collectSubtreeIds(serverNode).some(
        (descendantId) => descendantId !== id && movedNodeIds.has(descendantId)
      )
    if (protectsMovedDescendant) lateDeletes.push(entry)
    else earlyDeletes.push(entry)
  }

  return { earlyDeletes, creates, moves, lateDeletes, renames, recolors }
}
