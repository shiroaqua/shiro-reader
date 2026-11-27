import type { EphemeralReaderViewState } from "./engine"

export interface TranslatableBlockHandle {
  readonly blockId: string
  readonly sectionIndex: number
  readonly element: Element
  readonly sourceText: string
  readonly segmentHash: string
}

export interface SectionAugmenter {
  readonly id: string
  onSectionMount(ctx: {
    sectionIndex: number
    doc: Document
    mode: EphemeralReaderViewState
  }): void
  onVisibleWindowChange(ctx: {
    sectionIndex: number
    doc: Document
    visibleBlocks: readonly TranslatableBlockHandle[]
    lookaheadBlocks: readonly TranslatableBlockHandle[]
    signal: AbortSignal
    requestRelayout: () => void
  }): void
  onViewStateChange(ctx: {
    sectionIndex: number
    doc: Document
    state: EphemeralReaderViewState
    requestRelayout: () => void
  }): void
  onSectionUnmount(sectionIndex: number): void
}
