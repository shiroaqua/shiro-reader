import { afterEach, describe, expect, it, vi } from "vitest"
import type { FoliateRendererElement } from "foliate-js/view.js"
import type { SectionAugmenter } from "@shiro/ui-shared/features/reader/contracts/augmentation"
import { AugmentationPipeline } from "@shiro/ui-shared/features/reader/engine/epub/augmentation-pipeline"
import { CanonicalCfiResolver } from "@shiro/ui-shared/features/reader/engine/epub/canonical-cfi"
import { RelayoutCoordinator } from "@shiro/ui-shared/features/reader/engine/epub/relayout-coordinator"
import {
  findSyntheticTranslationForBlock,
  indexTranslatableBlocks,
  injectSyntheticTranslation,
  setDocumentTranslationMode,
  SHIRO_BLOCK_ID_ATTR,
} from "@shiro/ui-shared/features/reader/engine/epub/xhtml-dom"

function createLongXhtmlDoc(paragraphCount: number): Document {
  const paragraphs = Array.from({ length: paragraphCount }, (_, idx) => {
    const num = idx + 1
    return `<p id="p-${num}">Paragraph ${num} body text.</p>`
  }).join("\n")

  const xhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><title>Long Chapter</title></head>
  <body>${paragraphs}</body>
</html>`
  return new DOMParser().parseFromString(xhtml, "application/xhtml+xml")
}

function makeSpanRange(
  doc: Document,
  fromId: string,
  toId: string
): Range {
  const fromEl = doc.getElementById(fromId)!
  const toEl = doc.getElementById(toId)!
  const range = doc.createRange()
  range.setStart(fromEl.firstChild!, 0)
  range.setEnd(toEl.firstChild!, toEl.firstChild!.nodeValue?.length ?? 0)
  return range
}

describe("AugmentationPipeline and RelayoutCoordinator", () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it("extracts visible + lookahead blocks and aborts prior AbortSignal immediately on page turn", () => {
    const doc = createLongXhtmlDoc(25)
    const requestRelayout = vi.fn()
    const pipeline = new AugmentationPipeline({
      lookaheadCount: 10,
      requestRelayout,
    })

    const signals: AbortSignal[] = []
    const seenVisibleIds: string[][] = []
    const seenLookaheadIds: string[][] = []
    const onMount = vi.fn()
    const onViewStateChange = vi.fn()
    const onUnmount = vi.fn()

    const augmenter: SectionAugmenter = {
      id: "test-translator",
      onSectionMount: onMount,
      onVisibleWindowChange(ctx) {
        signals.push(ctx.signal)
        seenVisibleIds.push(ctx.visibleBlocks.map((b) => b.blockId))
        seenLookaheadIds.push(ctx.lookaheadBlocks.map((b) => b.blockId))
      },
      onViewStateChange,
      onSectionUnmount: onUnmount,
    }

    pipeline.registerAugmenter(augmenter)
    const blocks = pipeline.mountSection(0, doc)
    expect(blocks).toHaveLength(25)
    expect(onMount).toHaveBeenCalledTimes(1)

    // Page 1 shows p-1 .. p-3 -> lookahead should be p-4 .. p-13 (10 blocks)
    const page1Range = makeSpanRange(doc, "p-1", "p-3")
    pipeline.handleRelocate(0, doc, page1Range)
    expect(signals).toHaveLength(1)
    expect(signals[0]?.aborted).toBe(false)
    expect(seenVisibleIds[0]).toEqual(["b-0-0", "b-0-1", "b-0-2"])
    expect(seenLookaheadIds[0]).toHaveLength(10)
    expect(seenLookaheadIds[0]?.[0]).toBe("b-0-3")
    expect(seenLookaheadIds[0]?.[9]).toBe("b-0-12")

    // Duplicate relocate on the exact same visible range does not abort
    pipeline.handleRelocate(0, doc, page1Range)
    expect(signals).toHaveLength(1)
    expect(signals[0]?.aborted).toBe(false)

    // Turn page to p-4 .. p-6 -> previous signal must abort immediately
    const page2Range = makeSpanRange(doc, "p-4", "p-6")
    pipeline.handleRelocate(0, doc, page2Range)
    expect(signals).toHaveLength(2)
    expect(signals[0]?.aborted).toBe(true)
    expect(signals[1]?.aborted).toBe(false)
    expect(seenVisibleIds[1]).toEqual(["b-0-3", "b-0-4", "b-0-5"])

    // Reverse projection from synthetic translation nodes in translated_only mode
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `译文-${block.blockId}`)
    }
    pipeline.updateViewState({
      translationMode: "translated_only",
      targetLanguage: "zh-CN",
    })
    expect(doc.documentElement.getAttribute("data-shiro-trans-mode")).toBe(
      "translated_only"
    )
    expect(onViewStateChange).toHaveBeenCalledTimes(1)

    const p10 = doc.getElementById("p-10")!
    const syn10 = findSyntheticTranslationForBlock(
      doc,
      p10.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!
    const synOnlyRange = doc.createRange()
    synOnlyRange.setStart(syn10.firstChild!, 0)
    synOnlyRange.setEnd(syn10.firstChild!, 4)
    pipeline.handleRelocate(0, doc, synOnlyRange)
    expect(signals).toHaveLength(3)
    expect(signals[1]?.aborted).toBe(true)
    expect(seenVisibleIds[2]).toEqual(["b-0-9"])

    pipeline.unmountSection(0)
    expect(signals[2]?.aborted).toBe(true)
    expect(onUnmount).toHaveBeenCalledWith(0)
  })

  it("coalesces relayout calls via 180ms throttle + frame scheduling and restores visual anchor", async () => {
    vi.useFakeTimers()

    const doc = createLongXhtmlDoc(5)
    const blocks = indexTranslatableBlocks(doc, 1)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `翻译-${block.blockId}`)
    }
    setDocumentTranslationMode(doc, "translated_only")

    const resolver = new CanonicalCfiResolver()
    const p3Range = makeSpanRange(doc, "p-3", "p-3")
    const anchorCfi = resolver.rangeToCanonicalCfi(
      doc,
      "epubcfi(/6/6[ch2]!)",
      p3Range
    )!

    const goToTargets: Array<{
      index: number
      anchor?: ((d: Document) => Range | Element | number | null) | undefined
    }> = []
    const renderMock = vi.fn()
    const goToMock = vi.fn(
      async (target: {
        index: number
        anchor?: ((d: Document) => Range | Element | number | null) | undefined
      }) => {
        goToTargets.push(target)
      }
    )

    const renderer = {
      render: renderMock,
      goTo: goToMock,
    } as unknown as FoliateRendererElement

    const coordinator = new RelayoutCoordinator({
      getRenderer: () => renderer,
      captureAnchor: () => ({ sectionIndex: 1, anchorCfi }),
      resolver,
      throttleMs: 180,
      scheduleFrame: (cb) => globalThis.setTimeout(cb, 16) as unknown as number,
      cancelFrame: (id) =>
        globalThis.clearTimeout(id as unknown as ReturnType<typeof setTimeout>),
    })

    // Rapid burst of token streaming updates coalesces into a single render+goTo
    coordinator.scheduleRelayout()
    coordinator.scheduleRelayout()
    coordinator.scheduleRelayout()
    expect(renderMock).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(179)
    expect(renderMock).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(17)
    expect(renderMock).toHaveBeenCalledTimes(1)
    expect(goToMock).toHaveBeenCalledTimes(1)

    const resolvedAnchor = goToTargets[0]?.anchor?.(doc)
    const expectedSyn3 = findSyntheticTranslationForBlock(
      doc,
      doc.getElementById("p-3")!.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )
    expect(resolvedAnchor).toBe(expectedSyn3)

    // dispose cancels pending scheduled relayout
    coordinator.scheduleRelayout()
    coordinator.dispose()
    await vi.advanceTimersByTimeAsync(300)
    expect(renderMock).toHaveBeenCalledTimes(1)
  })

  it("no-ops safely when renderer has no public render method (FixedLayout)", () => {
    vi.useFakeTimers()

    const fxlRenderer = {
      goTo: vi.fn(),
    } as unknown as FoliateRendererElement

    const coordinator = new RelayoutCoordinator({
      getRenderer: () => fxlRenderer,
      captureAnchor: () => ({
        sectionIndex: 0,
        anchorCfi: "epubcfi(/6/2!/4/2/1:0)",
      }),
      scheduleFrame: (cb) => {
        cb()
        return 1
      },
      cancelFrame: () => {},
    })

    coordinator.scheduleRelayout()
    vi.advanceTimersByTime(180)
    expect(fxlRenderer.goTo).not.toHaveBeenCalled()
    coordinator.dispose()
  })

  it("prevents paginator (0,0) display:none stretched visibleRange from flooding all trailing blocks", () => {
    const doc = createLongXhtmlDoc(25)
    const pipeline = new AugmentationPipeline({
      lookaheadCount: 5,
      requestRelayout: vi.fn(),
    })

    const seenVisible: string[][] = []
    pipeline.registerAugmenter({
      id: "guard-translator",
      onSectionMount: () => {},
      onVisibleWindowChange: (ctx) => {
        seenVisible.push(ctx.visibleBlocks.map((b) => b.blockId))
      },
      onViewStateChange: () => {},
      onSectionUnmount: () => {},
    })

    const blocks = pipeline.mountSection(0, doc)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `译文-${block.blockId}`)
    }
    pipeline.updateViewState({
      translationMode: "translated_only",
      targetLanguage: "zh-CN",
    })

    // Simulate paginator.js Page 1 bug: range starts at visible synthetic p-1 and stretches
    // to the last display:none host p-25 whose getBoundingClientRect() is (0,0,0,0).
    const syn1 = findSyntheticTranslationForBlock(doc, "b-0-0")!
    const p25 = doc.getElementById("p-25")!
    const stretchedRange = doc.createRange()
    stretchedRange.setStart(syn1.firstChild!, 0)
    stretchedRange.setEnd(p25, 0)

    pipeline.handleRelocate(0, doc, stretchedRange)
    expect(seenVisible).toHaveLength(1)
    expect(seenVisible[0]).toEqual(["b-0-0"])

    // When visibleBlocks is empty and rangeEndHidden is true, lookahead starts at collapsed startContainer
    // instead of past the stretched hidden range end (p-25).
    const seenEmptyLookahead: string[][] = []
    pipeline.registerAugmenter({
      id: "empty-visible-check",
      onSectionMount: () => {},
      onVisibleWindowChange: (ctx) => {
        seenEmptyLookahead.push(ctx.lookaheadBlocks.map((b) => b.blockId))
      },
      onViewStateChange: () => {},
      onSectionUnmount: () => {},
    })
    const p1 = doc.getElementById("p-1")!
    p1.setAttribute("style", "display: none")
    const hiddenStartToEnd = doc.createRange()
    hiddenStartToEnd.setStart(p1, 0)
    hiddenStartToEnd.setEnd(p25, 0)
    pipeline.handleRelocate(0, doc, hiddenStartToEnd)
    expect(seenEmptyLookahead.at(-1)).toEqual([
      "b-0-1",
      "b-0-2",
      "b-0-3",
      "b-0-4",
      "b-0-5",
    ])
  })

  it("defers RelayoutCoordinator render and goTo while an active text selection exists, then flushes once cleared", () => {
    vi.useFakeTimers()
    const render = vi.fn()
    const goTo = vi.fn().mockResolvedValue(undefined)
    const mockRenderer = {
      render,
      goTo,
    } as unknown as FoliateRendererElement

    let selectionActive = true
    const coordinator = new RelayoutCoordinator({
      getRenderer: () => mockRenderer,
      hasActiveSelection: () => selectionActive,
      captureAnchor: () => ({
        sectionIndex: 0,
        anchorCfi: "epubcfi(/6/2!/4/2/1:0)",
      }),
      throttleMs: 180,
      scheduleFrame: (cb) => {
        cb()
        return 1
      },
      cancelFrame: () => {},
    })

    coordinator.scheduleRelayout()
    vi.advanceTimersByTime(180)
    // Must not disturb user's active selection during background augmentation.
    expect(render).not.toHaveBeenCalled()
    expect(goTo).not.toHaveBeenCalled()

    // Once the selection collapses, notifying the coordinator replays the deferred relayout.
    selectionActive = false
    coordinator.notifySelectionCleared()
    vi.advanceTimersByTime(180)
    expect(render).toHaveBeenCalledTimes(1)
    expect(goTo).toHaveBeenCalledTimes(1)

    coordinator.dispose()
    vi.useRealTimers()
  })
})
