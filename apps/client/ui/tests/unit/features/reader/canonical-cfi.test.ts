import { describe, expect, it } from "vitest"
import {
  CanonicalCfiResolver,
  canonicalCfiResolver,
  isCanonicalRangeCfi,
} from "@shiro/ui-shared/features/reader/engine/epub/canonical-cfi"
import {
  createXhtmlElement,
  findSyntheticTranslationForBlock,
  indexTranslatableBlocks,
  injectSyntheticTranslation,
  removeSyntheticTranslations,
  setDocumentTranslationMode,
  SHIRO_ANCHOR_FOR_ATTR,
  SHIRO_BLOCK_ID_ATTR,
  SHIRO_HAS_TRANS_ATTR,
  SHIRO_SYNTHETIC_ATTR,
  XHTML_NS,
} from "@shiro/ui-shared/features/reader/engine/epub/xhtml-dom"

const SECTION_BASE_CFI = "epubcfi(/6/4[chapter1]!)"

function createStrictXhtmlSectionDoc(): Document {
  const paragraphs = Array.from({ length: 10 }, (_, idx) => {
    const num = idx + 1
    return `<p id="p-${num}">Paragraph ${num} contains enough sample text for canonical CFI verification.</p>`
  }).join("\n")

  const xhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
  <head>
    <title>Chapter 1</title>
  </head>
  <body>
    <h1 id="title">Chapter Title</h1>
    ${paragraphs}
    <p id="empty-ws">   \n\t  </p>
    <ol id="ordered-list">
      <li id="li-1">First ordered list item for testing.</li>
      <li id="li-2">Second ordered list item for testing.</li>
    </ol>
    <dl id="def-list">
      <dt id="dt-1">Term Alpha</dt>
      <dd id="dd-1">Definition description for Term Alpha.</dd>
    </dl>
    <figure id="fig-1">
      <figcaption id="figcap-1">Caption for figure one.</figcaption>
    </figure>
    <pre id="code-pre"><code id="code-block">const invariant = true;</code></pre>
    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text>SVG text</text></svg>
    <math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math>
  </body>
</html>`

  const doc = new DOMParser().parseFromString(xhtml, "application/xhtml+xml")
  const parserError = doc.querySelector("parsererror")
  if (parserError) {
    throw new Error(`Invalid XHTML fixture: ${parserError.textContent ?? ""}`)
  }
  return doc
}

function makeTextRange(
  doc: Document,
  elementId: string,
  startOffset: number,
  endOffset: number
): Range {
  const el = doc.getElementById(elementId)
  if (!el || !el.firstChild) {
    throw new Error(`Missing text node in #${elementId}`)
  }
  const range = doc.createRange()
  range.setStart(el.firstChild, startOffset)
  range.setEnd(el.firstChild, endOffset)
  return range
}

describe("xhtml-dom and CanonicalCfiResolver (application/xhtml+xml)", () => {
  it("creates XHTML-namespaced nodes without innerHTML, indexes idempotently, and preserves tag semantics", () => {
    const doc = createStrictXhtmlSectionDoc()

    const customNode = createXhtmlElement(
      doc,
      "div",
      { [SHIRO_SYNTHETIC_ATTR]: "note", "data-custom": "1" },
      "<span>literal-not-parsed</span>"
    )
    expect(customNode.namespaceURI).toBe(XHTML_NS)
    expect(customNode.children).toHaveLength(0)
    expect(customNode.textContent).toBe("<span>literal-not-parsed</span>")

    const blocks = indexTranslatableBlocks(doc, 2)
    // h1 + 10 p + 2 li + dt + dd + figcaption = 16 (whitespace p, pre/code, svg, math excluded)
    expect(blocks).toHaveLength(16)
    expect(blocks.some((b) => b.element.id === "empty-ws")).toBe(false)
    expect(blocks.some((b) => b.element.id === "code-block")).toBe(false)
    expect(blocks.some((b) => b.element.id === "code-pre")).toBe(false)

    // Inject synthetic translations for p, li, dd, figcaption
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `译文：${block.sourceText}`)
    }

    // Re-indexing after synthetic injection must be completely idempotent
    const blocksAfter = indexTranslatableBlocks(doc, 2)
    expect(blocksAfter.map((b) => b.blockId)).toEqual(
      blocks.map((b) => b.blockId)
    )
    expect(blocksAfter.map((b) => b.segmentHash)).toEqual(
      blocks.map((b) => b.segmentHash)
    )

    // Tag-semantic legality checks: no <div> directly under <ol> or <dl>
    const orderedList = doc.getElementById("ordered-list")!
    expect(orderedList.querySelector(":scope > div")).toBeNull()
    const li1 = doc.getElementById("li-1")!
    const li1Trans = li1.nextElementSibling!
    expect(li1Trans.localName).toBe("li")
    expect(li1Trans.getAttribute(SHIRO_SYNTHETIC_ATTR)).toBe("translation")
    expect(li1Trans.getAttribute(SHIRO_ANCHOR_FOR_ATTR)).toBe(
      li1.getAttribute(SHIRO_BLOCK_ID_ATTR)
    )
    expect(li1Trans.getAttribute("style")).toContain("display: block")
    expect(li1Trans.getAttribute("style")).toContain("list-style: none")

    const defList = doc.getElementById("def-list")!
    expect(defList.querySelector(":scope > div")).toBeNull()
    const dd1 = doc.getElementById("dd-1")!
    expect(dd1.nextElementSibling?.localName).toBe("dd")

    // Idempotent update in place
    const p1 = doc.getElementById("p-1")!
    const p1TransFirst = findSyntheticTranslationForBlock(
      doc,
      p1.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )
    const p1TransUpdated = injectSyntheticTranslation(doc, p1, "更新后的第一段译文")
    expect(p1TransUpdated).toBe(p1TransFirst)
    expect(p1TransUpdated.textContent).toBe("更新后的第一段译文")

    // Cleanup removes synthetic nodes and ready marker
    removeSyntheticTranslations(doc)
    expect(doc.querySelectorAll(`[${SHIRO_SYNTHETIC_ATTR}]`)).toHaveLength(0)
    expect(p1.hasAttribute(SHIRO_HAS_TRANS_ATTR)).toBe(false)
  })

  it("produces 100% zero-drift CFIs before and after synthetic DOM injection", () => {
    const doc = createStrictXhtmlSectionDoc()
    const resolver = new CanonicalCfiResolver()

    const r1 = makeTextRange(doc, "p-1", 5, 12)
    const r5 = makeTextRange(doc, "p-5", 5, 12)
    const r10 = makeTextRange(doc, "p-10", 5, 12)
    const rLi2 = makeTextRange(doc, "li-2", 0, 6)

    const cfi1Before = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r1)
    const cfi5Before = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r5)
    const cfi10Before = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r10)
    const cfiLi2Before = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, rLi2)

    expect(cfi1Before).toMatch(/^epubcfi\(\/6\/4\[chapter1\]!\/.+\)$/)
    expect(cfi5Before).toBeTruthy()
    expect(cfi10Before).toBeTruthy()
    expect(cfiLi2Before).toBeTruthy()

    // Index and inject synthetic translation nodes for every block
    const blocks = indexTranslatableBlocks(doc, 0)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `翻译-${block.blockId}`)
    }

    const cfi1After = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r1)
    const cfi5After = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r5)
    const cfi10After = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, r10)
    const cfiLi2After = resolver.rangeToCanonicalCfi(doc, SECTION_BASE_CFI, rLi2)

    expect(cfi1After).toBe(cfi1Before)
    expect(cfi5After).toBe(cfi5Before)
    expect(cfi10After).toBe(cfi10Before)
    expect(cfiLi2After).toBe(cfiLi2Before)
  })

  it("resolves OPF-indirected CFIs back to exact original text ranges and hands off visual anchors in translated_only mode", () => {
    const doc = createStrictXhtmlSectionDoc()
    const blocks = indexTranslatableBlocks(doc, 0)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `中文-${block.blockId}`)
    }

    const expectedRange = makeTextRange(doc, "p-5", 5, 12)
    const expectedText = expectedRange.toString()
    const fullCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      expectedRange
    )
    expect(fullCfi).not.toBeNull()

    // Reverse resolve with /6/4[chapter1]! OPF indirection on injected document
    const resolved = canonicalCfiResolver.canonicalCfiToRange(doc, fullCfi!)
    expect(resolved).not.toBeNull()
    expect(resolved?.toString()).toBe(expectedText)
    expect(resolved?.startContainer).toBe(doc.getElementById("p-5")?.firstChild)
    expect(resolved?.startOffset).toBe(5)
    expect(resolved?.endOffset).toBe(12)

    // Local CFI without OPF indirection also resolves cleanly
    const localOnlyCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      "",
      expectedRange
    )
    expect(localOnlyCfi).toMatch(/^epubcfi\(\/[^!]+\)$/)
    const resolvedLocal = canonicalCfiResolver.canonicalCfiToRange(
      doc,
      localOnlyCfi!
    )
    expect(resolvedLocal?.toString()).toBe(expectedText)

    // Invalid or OPF-only CFIs return null instead of throwing
    expect(canonicalCfiResolver.canonicalCfiToRange(doc, "")).toBeNull()
    expect(canonicalCfiResolver.canonicalCfiToRange(doc, "invalid")).toBeNull()
    expect(canonicalCfiResolver.canonicalCfiToRange(doc, "epubcfi(/6/4!)")).toBeNull()
    expect(canonicalCfiResolver.canonicalCfiToRange(doc, "epubcfi(/6/4)")).toBeNull()

    // Synthetic node with un-snappable host (only documentElement ancestor, no body/siblings) returns null
    const bareDoc = new DOMParser().parseFromString(
      `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><div data-shiro-synthetic="translation" data-shiro-anchor-for="missing-id">orphan</div></html>`,
      "application/xhtml+xml"
    )
    const orphanDiv = bareDoc.querySelector("div")!
    const orphanRange = bareDoc.createRange()
    orphanRange.setStart(orphanDiv.firstChild!, 0)
    orphanRange.setEnd(orphanDiv.firstChild!, 3)
    expect(
      canonicalCfiResolver.rangeToCanonicalCfi(bareDoc, SECTION_BASE_CFI, orphanRange)
    ).toBeNull()

    // Bilingual mode keeps the original Range as visual anchor
    setDocumentTranslationMode(doc, "bilingual")
    const bilingualAnchor = canonicalCfiResolver.resolveVisualAnchor(doc, fullCfi!)
    expect(bilingualAnchor).toBeInstanceOf(Range)

    // translated_only mode hands off anchor to the paired visible synthetic node
    setDocumentTranslationMode(doc, "translated_only")
    const p5 = doc.getElementById("p-5")!
    const p5Synthetic = findSyntheticTranslationForBlock(
      doc,
      p5.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )
    const transOnlyAnchor = canonicalCfiResolver.resolveVisualAnchor(doc, fullCfi!)
    expect(transOnlyAnchor).toBe(p5Synthetic)

    // Explicit display:none also triggers visual anchor hand-off regardless of mode attr
    setDocumentTranslationMode(doc, "bilingual")
    p5.setAttribute("style", "display: none")
    expect(canonicalCfiResolver.resolveVisualAnchor(doc, fullCfi!)).toBe(
      p5Synthetic
    )
  })

  it("handles synthetic selections and bidirectional mixed selections without /-1 corruption", () => {
    const doc = createStrictXhtmlSectionDoc()
    const blocks = indexTranslatableBlocks(doc, 0)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `翻译内容：${block.sourceText}`)
    }
    setDocumentTranslationMode(doc, "bilingual")

    const p4 = doc.getElementById("p-4")!
    const p5 = doc.getElementById("p-5")!
    const div4 = findSyntheticTranslationForBlock(
      doc,
      p4.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!
    const div5 = findSyntheticTranslationForBlock(
      doc,
      p5.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!

    // 1. Pure original selection inside p-4
    const origRange = makeTextRange(doc, "p-4", 2, 10)
    expect(canonicalCfiResolver.inspectSelectionOrigin(origRange)).toEqual({
      textOrigin: "original",
      anchorBlockOriginalText: null,
    })

    // 2. Pure translation selection inside paragraph 5 synthetic div
    const transRange = doc.createRange()
    transRange.setStart(div5.firstChild!, 2)
    transRange.setEnd(div5.firstChild!, 10)
    const transOrigin = canonicalCfiResolver.inspectSelectionOrigin(transRange)
    expect(transOrigin).toEqual({
      textOrigin: "translation",
      anchorBlockOriginalText:
        "Paragraph 5 contains enough sample text for canonical CFI verification.",
    })

    const transCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      transRange
    )
    expect(transCfi).not.toBeNull()
    expect(transCfi).not.toContain("/-1")
    const roundtripTrans = canonicalCfiResolver.canonicalCfiToRange(doc, transCfi!)
    expect(roundtripTrans?.startContainer).toBe(p5.firstChild)
    expect(roundtripTrans?.startOffset).toBe(0)

    // 3. Forward mixed selection: p-4 original -> div-5 translation
    const forwardMixed = doc.createRange()
    forwardMixed.setStart(p4.firstChild!, 4)
    forwardMixed.setEnd(div5.firstChild!, 6)
    expect(canonicalCfiResolver.inspectSelectionOrigin(forwardMixed)).toEqual({
      textOrigin: "mixed",
      anchorBlockOriginalText:
        "Paragraph 5 contains enough sample text for canonical CFI verification.",
    })
    const forwardMixedCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      forwardMixed
    )
    expect(forwardMixedCfi).not.toBeNull()
    expect(forwardMixedCfi).not.toContain("/-1")
    const roundtripForward = canonicalCfiResolver.canonicalCfiToRange(
      doc,
      forwardMixedCfi!
    )
    expect(roundtripForward?.startContainer).toBe(p4.firstChild)
    expect(roundtripForward?.startOffset).toBe(4)
    expect(roundtripForward?.endContainer).toBe(p5.firstChild)
    expect(roundtripForward?.endOffset).toBe(
      p5.firstChild?.nodeValue?.length ?? 0
    )

    // 4. Reverse mixed selection: div-4 translation -> p-5 original
    const reverseMixed = doc.createRange()
    reverseMixed.setStart(div4.firstChild!, 3)
    reverseMixed.setEnd(p5.firstChild!, 9)
    expect(canonicalCfiResolver.inspectSelectionOrigin(reverseMixed)).toEqual({
      textOrigin: "mixed",
      anchorBlockOriginalText:
        "Paragraph 4 contains enough sample text for canonical CFI verification.",
    })
    const reverseMixedCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      reverseMixed
    )
    expect(reverseMixedCfi).not.toBeNull()
    expect(reverseMixedCfi).not.toContain("/-1")
    const roundtripReverse = canonicalCfiResolver.canonicalCfiToRange(
      doc,
      reverseMixedCfi!
    )
    expect(roundtripReverse?.startContainer).toBe(p4.firstChild)
    expect(roundtripReverse?.startOffset).toBe(0)
    expect(roundtripReverse?.endContainer).toBe(p5.firstChild)
    expect(roundtripReverse?.endOffset).toBe(9)

    // 5. Cross-paragraph original-to-original in bilingual mode traverses div-4 -> mixed
    const crossOriginal = doc.createRange()
    crossOriginal.setStart(p4.firstChild!, 0)
    crossOriginal.setEnd(p5.firstChild!, 5)
    expect(canonicalCfiResolver.inspectSelectionOrigin(crossOriginal)).toEqual({
      textOrigin: "mixed",
      anchorBlockOriginalText:
        "Paragraph 4 contains enough sample text for canonical CFI verification.",
    })
  })

  it("clamps triple-click trailing endOffset=0 boundaries and preserves Element-container ranges", () => {
    const doc = createStrictXhtmlSectionDoc()
    const blocks = indexTranslatableBlocks(doc, 0)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `翻译内容：${block.sourceText}`)
    }
    setDocumentTranslationMode(doc, "bilingual")

    const p4 = doc.getElementById("p-4")!
    const p5 = doc.getElementById("p-5")!
    const div4 = findSyntheticTranslationForBlock(
      doc,
      p4.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!

    // Triple-clicking original p-4 yields endContainer inside next sibling synthetic div4 at offset 0.
    // Must classify as pure "original" and resolve CFI solely to p-4.
    const tripleClickOriginal = doc.createRange()
    tripleClickOriginal.setStart(p4.firstChild!, 0)
    tripleClickOriginal.setEnd(div4.firstChild!, 0)
    expect(canonicalCfiResolver.inspectSelectionOrigin(tripleClickOriginal)).toEqual({
      textOrigin: "original",
      anchorBlockOriginalText: null,
    })
    const tripleOriginalCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      tripleClickOriginal
    )
    expect(tripleOriginalCfi).not.toBeNull()
    const resolvedTripleOriginal = canonicalCfiResolver.canonicalCfiToRange(
      doc,
      tripleOriginalCfi!
    )
    expect(resolvedTripleOriginal?.toString()).toBe(p4.textContent)

    // Triple-clicking synthetic div4 yields endContainer at p-5 offset 0 -> must stay pure "translation".
    const tripleClickTranslation = doc.createRange()
    tripleClickTranslation.setStart(div4.firstChild!, 0)
    tripleClickTranslation.setEnd(p5.firstChild!, 0)
    expect(canonicalCfiResolver.inspectSelectionOrigin(tripleClickTranslation)).toEqual({
      textOrigin: "translation",
      anchorBlockOriginalText:
        "Paragraph 4 contains enough sample text for canonical CFI verification.",
    })

    // Element-container Range (e.g. range.selectNode(p5) -> container=body, offset=childIdx)
    // must resolve to p-5's exact text rather than collapsing onto <body>.
    const elementRange = doc.createRange()
    elementRange.selectNode(p5)
    const elementCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      elementRange
    )
    expect(elementCfi).not.toBeNull()
    const resolvedElement = canonicalCfiResolver.canonicalCfiToRange(doc, elementCfi!)
    expect(resolvedElement?.toString()).toBe(p5.textContent)
  })

  it("handles cross-Realm Element targets in resolveVisualTarget and ignores hidden siblings during triple-click clamping", () => {
    const doc = createStrictXhtmlSectionDoc()
    const blocks = indexTranslatableBlocks(doc, 0)
    for (const block of blocks) {
      injectSyntheticTranslation(doc, block.element, `译文：${block.sourceText}`)
    }
    const p4 = doc.getElementById("p-4")!
    const p5 = doc.getElementById("p-5")!
    const div4 = findSyntheticTranslationForBlock(
      doc,
      p4.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!
    const div5 = findSyntheticTranslationForBlock(
      doc,
      p5.getAttribute(SHIRO_BLOCK_ID_ATTR)!
    )!

    // Simulate a cross-Realm Element (fails `instanceof Element` in host Realm, has nodeType === 1).
    const crossRealmP5 = {
      nodeType: 1,
      parentNode: p5.parentNode,
      hasAttribute: (name: string) => p5.hasAttribute(name),
      getAttribute: (name: string) => p5.getAttribute(name),
    } as unknown as Element
    expect(crossRealmP5 instanceof Element).toBe(false)

    setDocumentTranslationMode(doc, "original")
    expect(canonicalCfiResolver.resolveVisualTarget(doc, crossRealmP5)).toBe(
      crossRealmP5
    )

    setDocumentTranslationMode(doc, "translated_only")
    expect(canonicalCfiResolver.resolveVisualTarget(doc, crossRealmP5)).toBe(
      div5
    )

    // Triple-click div4 in translated_only mode stops at (div5.firstChild, 0):
    // walking backwards must skip hidden host p-5 and land on div4 -> pure "translation".
    const transOnlyTriple = doc.createRange()
    transOnlyTriple.setStart(div4.firstChild!, 0)
    transOnlyTriple.setEnd(div5.firstChild!, 0)
    expect(canonicalCfiResolver.inspectSelectionOrigin(transOnlyTriple)).toEqual({
      textOrigin: "translation",
      anchorBlockOriginalText:
        "Paragraph 4 contains enough sample text for canonical CFI verification.",
    })
    const transOnlyCfi = canonicalCfiResolver.rangeToCanonicalCfi(
      doc,
      SECTION_BASE_CFI,
      transOnlyTriple
    )
    expect(transOnlyCfi).not.toBeNull()
    const resolvedTransPoint = canonicalCfiResolver.canonicalCfiToRange(
      doc,
      transOnlyCfi!
    )
    expect(resolvedTransPoint?.collapsed).toBe(true)
    expect(resolvedTransPoint?.startContainer).toBe(p4.firstChild)
    expect(resolvedTransPoint?.startOffset).toBe(0)
    expect(canonicalCfiResolver.resolveVisualAnchor(doc, transOnlyCfi!)).toBe(
      div4
    )

    // Triple-click p4 in original mode with cached hidden translations stops at (p5.firstChild, 0):
    // walking backwards must skip hidden synthetic div4 and stay pure "original".
    setDocumentTranslationMode(doc, "original")
    const originalTriple = doc.createRange()
    originalTriple.setStart(p4.firstChild!, 0)
    originalTriple.setEnd(p5.firstChild!, 0)
    expect(canonicalCfiResolver.inspectSelectionOrigin(originalTriple)).toEqual({
      textOrigin: "original",
      anchorBlockOriginalText: null,
    })
  })

  it("isCanonicalRangeCfi mirrors Rust CFI range validation including escaped bracket commas", () => {
    expect(isCanonicalRangeCfi("epubcfi(/6/4[ch1]!/4/2,/1:0,/3:15)")).toBe(true)
    expect(isCanonicalRangeCfi("epubcfi(/6/4!/4/2/1,:2,:9)")).toBe(true)
    expect(isCanonicalRangeCfi("epubcfi(/6/4!/4,,/2:5)")).toBe(true)
    expect(isCanonicalRangeCfi("epubcfi(/6/4[a,b^,c]!/4/2,/1:0,/1:5)")).toBe(true)

    expect(isCanonicalRangeCfi("epubcfi(/6/4[ch1]!)")).toBe(false)
    expect(isCanonicalRangeCfi("epubcfi(/6/4[ch1]!/4/2/1:12[a,b^,c])")).toBe(false)
    expect(isCanonicalRangeCfi("epubcfi(/6/4!/4/-1,/1:0,/1:2)")).toBe(false)
    expect(isCanonicalRangeCfi("epubcfi(/6/4!/4,bad,/2)")).toBe(false)
    expect(isCanonicalRangeCfi("epubcfi(/6/4!/4,,)")).toBe(false)
    expect(isCanonicalRangeCfi(" epubcfi(/6/4!/4/2,/1:0,/1:5)")).toBe(false)
    expect(isCanonicalRangeCfi("invalid")).toBe(false)
  })
})
