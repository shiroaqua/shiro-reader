import { describe, expect, it, vi } from "vitest"
import {
  attachBookSanitizer,
  projectClientPointToNormalizedViewport,
  sanitizeMarkupDocument,
  SectionLifecycleBridge,
} from "@shiro/ui-shared/features/reader/engine/epub/section-bridge"

function makeRect(
  left: number,
  top: number,
  width: number,
  height: number
): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  }
}

describe("section-bridge XSS sanitization & coordinate projection", () => {
  it("strips <script>, on* attributes, and javascript:/vbscript: URIs from XHTML and SVG", async () => {
    const xhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:x="http://www.w3.org/1999/xlink">
  <body>
    <script>window.parent.__TAURI_INTERNALS__ = null</script>
    <img src="cover.jpg" onerror="alert(1)" onload="alert(2)" />
    <a id="bad-link" href="  java&#x09;script:alert(1)">Bad</a>
    <a id="xlink-bad" x:href="javascript:alert(1)">XLink Bad</a>
    <img id="svg-data" src="data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=" />
    <a id="vb-link" href="vbscript:msgbox(1)">VB</a>
    <a id="good-link" href="chapter2.xhtml#sec1">Good</a>
  </body>
</html>`

    const sanitizedXhtml = sanitizeMarkupDocument(xhtml, "application/xhtml+xml")
    const parsedXhtml = new DOMParser().parseFromString(
      sanitizedXhtml,
      "application/xhtml+xml"
    )
    expect(parsedXhtml.querySelector("script")).toBeNull()
    expect(parsedXhtml.querySelector("img")?.hasAttribute("onerror")).toBe(false)
    expect(parsedXhtml.querySelector("img")?.hasAttribute("onload")).toBe(false)
    expect(
      parsedXhtml.getElementById("bad-link")?.hasAttribute("href")
    ).toBe(false)
    expect(
      parsedXhtml.getElementById("xlink-bad")?.hasAttribute("x:href")
    ).toBe(false)
    expect(
      parsedXhtml.getElementById("svg-data")?.hasAttribute("src")
    ).toBe(false)
    expect(
      parsedXhtml.getElementById("vb-link")?.hasAttribute("href")
    ).toBe(false)
    expect(parsedXhtml.getElementById("good-link")?.getAttribute("href")).toBe(
      "chapter2.xhtml#sec1"
    )

    const svg = `<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><a href="javascript:alert(2)"><rect width="10" height="10" onclick="alert(3)"/></a></svg>`
    const transformTarget = new EventTarget()
    attachBookSanitizer({
      transformTarget,
      sections: [],
      init: async () => {
        throw new Error("not used")
      },
    } as never)

    const detail = {
      name: "OEBPS/diagram.svg",
      data: svg,
      type: "image/svg+xml",
    }
    transformTarget.dispatchEvent(new CustomEvent("data", { detail }))
    const transformedSvg = await Promise.resolve(detail.data)
    expect(typeof transformedSvg).toBe("string")
    expect(String(transformedSvg)).not.toContain("<script")
    expect(String(transformedSvg)).not.toContain("onclick")
    expect(String(transformedSvg)).not.toContain("javascript:")

    // Malformed XHTML falls back to HTML5 sanitizer and updates detail.type to text/html
    const malformedXhtmlDetail = {
      name: "OEBPS/broken.xhtml",
      data: `<html><body><p>Unclosed paragraph<script>alert(1)</script></body></html>`,
      type: "application/xhtml+xml",
    }
    transformTarget.dispatchEvent(
      new CustomEvent("data", { detail: malformedXhtmlDetail })
    )
    const fallbackOut = await Promise.resolve(malformedXhtmlDetail.data)
    const fallbackType = await Promise.resolve(malformedXhtmlDetail.type)
    expect(fallbackType).toBe("text/html")
    expect(String(fallbackOut)).not.toContain("<script")
  })

  it("strips meta refresh redirects, application/xml scripts & xml-stylesheet PIs, and SVG SMIL/foreignObject vectors", async () => {
    const xhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <meta http-equiv="refresh" content="0; url=https://evil.example/phish" />
    <meta http-equiv="Refresh" content="0; url='javascript:alert(1)" />
    <meta http-equiv="refresh" content="0; url=java&#10;script:alert(2)" />
    <meta name="author" content="Keep Me" />
  </head>
  <body>
    <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
      <a>
        <set attributeName="href" to="javascript:alert(1)" />
        <animate attributeName="xlink:href" values="ok;javascript:alert(2)" />
        <animate attributeName="opacity" from="0" to="1" />
      </a>
      <foreignObject><script>alert(3)</script><div onclick="alert(4)">Safe</div></foreignObject>
    </svg>
  </body>
</html>`

    const cleaned = sanitizeMarkupDocument(xhtml, "application/xhtml+xml")
    const doc = new DOMParser().parseFromString(cleaned, "application/xhtml+xml")
    expect(doc.querySelector('meta[http-equiv="refresh" i]')).toBeNull()
    expect(doc.querySelector('meta[name="author"]')?.getAttribute("content")).toBe(
      "Keep Me"
    )
    expect(doc.querySelector("set")).toBeNull()
    expect(doc.querySelectorAll("animate").length).toBe(1)
    expect(doc.querySelector("animate")?.getAttribute("attributeName")).toBe(
      "opacity"
    )
    expect(doc.querySelector("script")).toBeNull()
    expect(doc.querySelector("foreignObject div")?.hasAttribute("onclick")).toBe(
      false
    )

    const xmlPayload = `<?xml version="1.0" encoding="utf-8"?><?xml-stylesheet type="text/xsl" href="evil.xsl"?><html xmlns="http://www.w3.org/1999/xhtml"><body><script>alert(1)</script><p>Hello XML</p></body></html>`
    const transformTarget = new EventTarget()
    attachBookSanitizer({
      transformTarget,
      sections: [],
      init: async () => {
        throw new Error("not used")
      },
    } as never)

    const xmlDetail = {
      name: "OEBPS/raw.xml",
      data: new Blob([xmlPayload], { type: "application/xml" }),
      type: "application/xml",
    }
    transformTarget.dispatchEvent(new CustomEvent("data", { detail: xmlDetail }))
    const outXml = String(await Promise.resolve(xmlDetail.data))
    expect(outXml).not.toContain("xml-stylesheet")
    expect(outXml).not.toContain("<script")
    expect(outXml).toContain("Hello XML")

    // Arbitrary *+xml MIME types (e.g. application/mathml+xml, application/foo+xml) must also sanitize.
    const mathmlPayload = `<?xml version="1.0" encoding="utf-8"?><?xml-stylesheet type="text/xsl" href="evil.xsl"?><math xmlns="http://www.w3.org/1998/Math/MathML"><base href="https://evil.example/"/><script>alert(1)</script><animateColor attributeName="href" to="javascript:alert(1)"/><a href="data:application/mathml+xml;base64,PHNjcmlwdD4=">Bad</a><mi>x</mi></math>`
    const mathDetail = {
      name: "OEBPS/formula.mml",
      data: mathmlPayload,
      type: "application/mathml+xml",
    }
    transformTarget.dispatchEvent(new CustomEvent("data", { detail: mathDetail }))
    const outMath = String(await Promise.resolve(mathDetail.data))
    const outMathType = String(await Promise.resolve(mathDetail.type))
    expect(outMathType).toBe("application/mathml+xml")
    expect(outMath).not.toContain("xml-stylesheet")
    expect(outMath).not.toContain("<base")
    expect(outMath).not.toContain("<script")
    expect(outMath).not.toContain("animateColor")
    expect(outMath).not.toContain("data:application/mathml+xml")
    expect(outMath).toContain("<mi>x</mi>")

    const fooXmlPayload = `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><meta http-equiv="refresh" content="0;url=https://evil.example"/></head><body><img src="data:application/xml,<script/>" /><p>FooXML</p></body></html>`
    const fooDetail = {
      name: "OEBPS/custom.fooxml",
      data: fooXmlPayload,
      type: "application/foo+xml",
    }
    transformTarget.dispatchEvent(new CustomEvent("data", { detail: fooDetail }))
    const outFoo = String(await Promise.resolve(fooDetail.data))
    expect(outFoo).not.toContain("refresh")
    expect(outFoo).not.toContain("data:application/xml")
    expect(outFoo).toContain("FooXML")
  })

  it("projects page-2 multi-column and scaled iframe clicks accurately to normX < 0.33", () => {
    const containerRect = makeRect(0, 0, 1200, 800)

    // Multi-column paginated iframe: 3 pages wide (3600px), scrolled to page 2 (left = -1200)
    const multiColIframe = document.createElement("iframe")
    Object.defineProperty(multiColIframe, "offsetWidth", { value: 3600 })
    Object.defineProperty(multiColIframe, "offsetHeight", { value: 800 })
    multiColIframe.getBoundingClientRect = () => makeRect(-1200, 0, 3600, 800)

    // User clicks 150px from left edge of viewport on page 2 -> clientX inside iframe is 1350
    const page2Left = projectClientPointToNormalizedViewport(
      1350,
      400,
      multiColIframe,
      containerRect
    )
    expect(page2Left.normX).toBeCloseTo(150 / 1200, 3)
    expect(page2Left.normX).toBeLessThan(0.33)

    // Scaled FXL iframe (transform: scale(0.5)) centered in viewport
    const fxlIframe = document.createElement("iframe")
    Object.defineProperty(fxlIframe, "offsetWidth", { value: 800 })
    Object.defineProperty(fxlIframe, "offsetHeight", { value: 1200 })
    fxlIframe.getBoundingClientRect = () => makeRect(200, 100, 400, 600)

    // Click at unscaled local x=100 -> host x = 200 + 100 * 0.5 = 250 -> 250 / 1200 = 0.208 < 0.33
    const fxlLeft = projectClientPointToNormalizedViewport(
      100,
      200,
      fxlIframe,
      containerRect
    )
    expect(fxlLeft.normX).toBeCloseTo(250 / 1200, 3)
    expect(fxlLeft.normX).toBeLessThan(0.33)
  })

  it("suppresses tap on links or active selection, leaves wheel events unhandled, and cleans up on unmount", () => {
    const container = document.createElement("div")
    container.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)

    let fakeNow = 1000
    const onViewportTap = vi.fn()
    const onKeyCommand = vi.fn()

    const bridge = new SectionLifecycleBridge({
      container,
      onViewportTap,
      onKeyCommand,
      now: () => fakeNow,
    })

    const doc = new DOMParser().parseFromString(
      `<?xml version="1.0" encoding="utf-8"?>
       <html xmlns="http://www.w3.org/1999/xhtml">
         <body>
           <p id="para">Selectable text inside paragraph</p>
           <a id="link" href="next.xhtml">Next Chapter</a>
         </body>
       </html>`,
      "application/xhtml+xml"
    )
    const link = doc.getElementById("link")!
    const para = doc.getElementById("para")!

    let mockSelection: { isCollapsed: boolean; toString: () => string } = {
      isCollapsed: true,
      toString: () => "",
    }
    Object.defineProperty(doc, "defaultView", {
      value: {
        frameElement: null,
        getSelection: () => mockSelection,
      },
      configurable: true,
    })

    bridge.mountSection(0, doc)

    // 1. Tap inside <a> link must NOT trigger onViewportTap
    link.dispatchEvent(
      new PointerEvent("pointerdown", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 100,
        clientY: 100,
        bubbles: true,
      })
    )
    fakeNow += 50
    link.dispatchEvent(
      new PointerEvent("pointerup", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 100,
        clientY: 100,
        bubbles: true,
      })
    )
    expect(onViewportTap).not.toHaveBeenCalled()

    // 2. Tap while selection is non-collapsed must NOT trigger onViewportTap
    mockSelection = { isCollapsed: false, toString: () => "Selectable" }
    para.dispatchEvent(
      new PointerEvent("pointerdown", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 200,
        clientY: 200,
        bubbles: true,
      })
    )
    fakeNow += 50
    para.dispatchEvent(
      new PointerEvent("pointerup", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 200,
        clientY: 200,
        bubbles: true,
      })
    )
    expect(onViewportTap).not.toHaveBeenCalled()

    // 3. Clean tap triggers onViewportTap
    mockSelection = { isCollapsed: true, toString: () => "" }
    para.dispatchEvent(
      new PointerEvent("pointerdown", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 200,
        clientY: 200,
        bubbles: true,
      })
    )
    fakeNow += 50
    para.dispatchEvent(
      new PointerEvent("pointerup", {
        button: 0,
        pointerId: 1,
        isPrimary: true,
        clientX: 201,
        clientY: 201,
        bubbles: true,
      })
    )
    expect(onViewportTap).toHaveBeenCalledTimes(1)

    // 4. Wheel events are NOT hijacked / prevented in paginated mode
    const wheelEvent = new WheelEvent("wheel", { deltaY: 120, cancelable: true })
    doc.dispatchEvent(wheelEvent)
    expect(wheelEvent.defaultPrevented).toBe(false)

    // 5. contextmenu prevented and keyboard forwarded
    const ctxEvent = new MouseEvent("contextmenu", { cancelable: true })
    doc.dispatchEvent(ctxEvent)
    expect(ctxEvent.defaultPrevented).toBe(true)

    doc.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }))
    expect(onKeyCommand).toHaveBeenCalledTimes(1)

    // Modified Shift+ArrowRight is ignored; F11 is forwarded
    doc.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", shiftKey: true })
    )
    expect(onKeyCommand).toHaveBeenCalledTimes(1)

    doc.dispatchEvent(new KeyboardEvent("keydown", { key: "F11" }))
    expect(onKeyCommand).toHaveBeenCalledTimes(2)

    // 6. Unmount stops all section listeners
    bridge.unmountSection(0)
    fakeNow += 300
    doc.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }))
    expect(onKeyCommand).toHaveBeenCalledTimes(2)

    bridge.dispose()
  })

  it("switches chapter at scrolled boundaries via wheel and touch overscroll with dwell and cooldown", () => {
    const container = document.createElement("div")
    container.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)

    let fakeNow = 5000
    let boundaryEnabled = true
    let boundaryState = {
      atTop: false,
      atBottom: true,
      hasPrev: true,
      hasNext: true,
    }
    const onBoundaryChapterSwitch = vi.fn()

    const bridge = new SectionLifecycleBridge({
      container,
      onViewportTap: vi.fn(),
      onKeyCommand: vi.fn(),
      isBoundaryScrollEnabled: () => boundaryEnabled,
      getScrollBoundaryState: () => boundaryState,
      onBoundaryChapterSwitch,
      boundaryEdgeDwellMs: 150,
      boundaryWheelThresholdPx: 80,
      boundaryTouchThresholdPx: 56,
      boundarySwitchCooldownMs: 450,
      now: () => fakeNow,
    })

    const doc = new DOMParser().parseFromString(
      `<?xml version="1.0" encoding="utf-8"?>
       <html xmlns="http://www.w3.org/1999/xhtml">
         <body><p id="p1">Chapter content</p></body>
       </html>`,
      "application/xhtml+xml"
    )
    bridge.mountSection(0, doc)

    // Mid-chapter never triggers
    boundaryState = { atTop: false, atBottom: false, hasPrev: true, hasNext: true }
    doc.dispatchEvent(new WheelEvent("wheel", { deltaY: 120, cancelable: true }))
    expect(onBoundaryChapterSwitch).not.toHaveBeenCalled()

    // Scroll activity resets dwell timer: within 150ms dwell at bottom does NOT switch
    boundaryState = { atTop: false, atBottom: true, hasPrev: true, hasNext: true }
    bridge.notifyScrollActivity()
    fakeNow += 80
    doc.dispatchEvent(new WheelEvent("wheel", { deltaY: 120, cancelable: true }))
    expect(onBoundaryChapterSwitch).not.toHaveBeenCalled()

    // After dwell expires, accumulating >= 80px triggers "next" and prevents default
    fakeNow += 200
    const w1 = new WheelEvent("wheel", { deltaY: 45, cancelable: true })
    doc.dispatchEvent(w1)
    expect(onBoundaryChapterSwitch).not.toHaveBeenCalled()

    const w2 = new WheelEvent("wheel", { deltaY: 45, cancelable: true })
    doc.dispatchEvent(w2)
    expect(w2.defaultPrevented).toBe(true)
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(1)
    expect(onBoundaryChapterSwitch).toHaveBeenLastCalledWith("next")

    // Within 450ms cooldown, further wheel events are ignored
    fakeNow += 200
    doc.dispatchEvent(new WheelEvent("wheel", { deltaY: 160, cancelable: true }))
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(1)

    // After cooldown expires, wheel up at top triggers "prev"
    fakeNow += 400
    boundaryState = { atTop: true, atBottom: false, hasPrev: true, hasNext: true }
    const wUp = new WheelEvent("wheel", { deltaY: -100, cancelable: true })
    doc.dispatchEvent(wUp)
    expect(wUp.defaultPrevented).toBe(true)
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(2)
    expect(onBoundaryChapterSwitch).toHaveBeenLastCalledWith("prev")

    // Touch swipe starting mid-chapter does NOT trigger even if reaching bottom later
    fakeNow += 600
    boundaryState = { atTop: false, atBottom: false, hasPrev: true, hasNext: true }
    const makeTouchEvent = (type: string, screenX: number, screenY: number) => {
      const ev = new Event(type, { cancelable: true })
      Object.defineProperty(ev, "touches", {
        value: type === "touchend" ? [] : [{ screenX, screenY }],
      })
      Object.defineProperty(ev, "changedTouches", {
        value: [{ screenX, screenY }],
      })
      return ev
    }
    doc.dispatchEvent(makeTouchEvent("touchstart", 200, 500))
    boundaryState = { atTop: false, atBottom: true, hasPrev: true, hasNext: true }
    doc.dispatchEvent(makeTouchEvent("touchmove", 200, 380))
    doc.dispatchEvent(makeTouchEvent("touchend", 200, 380))
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(2)

    // Touch swipe up starting at bottom triggers "next" on touchend
    doc.dispatchEvent(makeTouchEvent("touchstart", 200, 500))
    doc.dispatchEvent(makeTouchEvent("touchmove", 202, 420))
    doc.dispatchEvent(makeTouchEvent("touchend", 202, 420))
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(3)
    expect(onBoundaryChapterSwitch).toHaveBeenLastCalledWith("next")

    // Touch swipe down starting at top after cooldown triggers "prev"
    fakeNow += 500
    boundaryState = { atTop: true, atBottom: false, hasPrev: true, hasNext: true }
    doc.dispatchEvent(makeTouchEvent("touchstart", 200, 200))
    doc.dispatchEvent(makeTouchEvent("touchmove", 204, 280))
    doc.dispatchEvent(makeTouchEvent("touchend", 204, 280))
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(4)
    expect(onBoundaryChapterSwitch).toHaveBeenLastCalledWith("prev")

    // Disabled (e.g. paginated or seamless mode) never fires
    fakeNow += 500
    boundaryEnabled = false
    doc.dispatchEvent(new WheelEvent("wheel", { deltaY: -120, cancelable: true }))
    expect(onBoundaryChapterSwitch).toHaveBeenCalledTimes(4)

    bridge.dispose()
  })
})
