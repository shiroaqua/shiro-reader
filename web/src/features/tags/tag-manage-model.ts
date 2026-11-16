import { library } from "@/api/library"
import { collectSubtreeIds, findTag } from "@/lib/tag-tree"
import type { Tag, TagTreeNode, UndefineTagStrategy } from "@/lib/types"
export async function commitTagChanges(changes: TagChanges) {
  const progress: CommitProgress = {
    createdIds: new Map(),
    movedIds: [],
    renamedIds: [],
    recoloredIds: [],
    deletedIds: [],
  }
  try {
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
    for (const [id, strategy] of changes.deletes) {
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

export type PendingCreate = Omit<Tag, "id"> & { draftId: string }

export type TagChanges = {
  creates: PendingCreate[]
  moves: [string, string | null][]
  renames: [string, string][]
  recolors: [string, string][]
  deletes: [string, UndefineTagStrategy][]
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

  for (const create of pendingCreates) {
    const node: TagTreeNode = {
      id: create.draftId,
      name: create.name,
      parent_id: create.parent_id,
      color: create.color,
      allows_value: create.allows_value,
      value_regex: create.value_regex,
      children: [],
    }
    const parent = create.parent_id ? byId.get(create.parent_id) : undefined
    if (parent) parent.children.push(node)
    else tree.push(node)
    byId.set(node.id, node)
  }

  return tree
}

export function applyPendingMoves(
  tree: TagTreeNode[],
  moves: ReadonlyMap<string, string | null>
): TagTreeNode[] {
  const detach = (
    nodes: TagTreeNode[],
    id: string
  ): TagTreeNode | undefined => {
    const index = nodes.findIndex((node) => node.id === id)
    if (index !== -1) return nodes.splice(index, 1)[0]
    for (const node of nodes) {
      const found = detach(node.children, id)
      if (found) return found
    }
  }

  for (const [id, parentId] of moves) {
    const node = findTag(tree, id)
    const parent = parentId ? findTag(tree, parentId) : null
    if (!node || (parentId && !parent)) continue
    if (parentId && collectSubtreeIds(node).includes(parentId)) continue
    detach(tree, id)
    node.parent_id = parentId
    if (parent) parent.children.push(node)
    else tree.push(node)
  }
  return tree
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
  metadata: Map<string, { depth: number; ancestors: string[] }>,
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

  const creates = pendingCreates
    .filter(
      (create) =>
        !pendingDeletes.has(create.draftId) &&
        !cascadeDeletedIds.has(create.draftId)
    )
    .map((create) => ({
      ...create,
      parent_id: resolveParentId(create.parent_id),
    }))

  const renames = [...pendingRenames].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const recolors = [...pendingRecolors].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const moves = [...pendingMoves]
    .filter(([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id))
    .map(
      ([id, parentId]) =>
        [id, resolveParentId(parentId)] as [string, string | null]
    )
    .filter(([id, parentId]) => findTag(serverTree, id)?.parent_id !== parentId)
  const pendingCreateIds = new Set(
    pendingCreates.map((create) => create.draftId)
  )
  const deletes = [...pendingDeletes]
    .filter(
      ([id]) =>
        !pendingCreateIds.has(id) &&
        !(metadata.get(id)?.ancestors ?? []).some(
          (ancestorId) => pendingDeletes.get(ancestorId)?.strategy === "CASCADE"
        )
    )
    .map(
      ([id, deletion]) =>
        [id, deletion.strategy] as [string, UndefineTagStrategy]
    )
    .sort(
      ([leftId], [rightId]) =>
        (metadata.get(rightId)?.depth ?? 0) - (metadata.get(leftId)?.depth ?? 0)
    )

  return { creates, moves, renames, recolors, deletes }
}
