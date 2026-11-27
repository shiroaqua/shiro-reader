import { describe, expect, it } from "vitest"
import {
  computeToolbarPosition,
  projectSelectionGeometry,
} from "@shiro/ui-shared/features/reader/selection/geometry"

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

function makeFakeRange(clientRects: readonly DOMRect[]): Range {
  const bounding =
    clientRects[0] ?? makeRect(0, 0, 0, 0)
  return {
    getClientRects: () => clientRects as unknown as DOMRectList,
    getBoundingClientRect: () => bounding,
  } as unknown as Range
}

describe("selection geometry projection and viewport clamping", () => {
  it("places toolbar above selection when clearance >= 56px", () => {
    const containerRect = makeRect(0, 0, 1000, 800)
    const iframe = document.createElement("iframe")
    Object.defineProperty(iframe, "offsetWidth", { value: 1000 })
    Object.defineProperty(iframe, "offsetHeight", { value: 800 })
    iframe.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)

    const range = makeFakeRange([makeRect(400, 200, 120, 24)])
    const anchor = projectSelectionGeometry(range, iframe, containerRect)

    expect(anchor).toEqual({
      preferredPlacement: "top",
      anchorX: 460,
      topY: 190,
      bottomY: 234,
    })

    const pos = computeToolbarPosition(
      anchor!,
      { width: 100, height: 40 },
      { width: 1000, height: 800 }
    )
    expect(pos.placement).toBe("top")
    expect(pos.left).toBe(410)
    expect(pos.top).toBe(150)
    expect(pos.arrowOffsetX).toBe(50)
  })

  it("projects scaled FXL iframe rects and flips below with +28px Android waterdrop offset near top bar", () => {
    const containerRect = makeRect(0, 0, 800, 900)
    const iframe = document.createElement("iframe")
    // FXL 800x1200 scaled 0.5x to 400x600 at left=200, top=20
    Object.defineProperty(iframe, "offsetWidth", { value: 800 })
    Object.defineProperty(iframe, "offsetHeight", { value: 1200 })
    iframe.getBoundingClientRect = () => makeRect(200, 20, 400, 600)

    // Local rect at (100, 40, 200, 30) -> host rect at (250, 40, 100, 15)
    // spaceAbove = 40 - (0 + 20 safeTop) = 20 < 56 -> flips bottom
    const range = makeFakeRange([makeRect(100, 40, 200, 30)])
    const desktopAnchor = projectSelectionGeometry(range, iframe, containerRect, {
      isAndroid: false,
      safeTopPx: 20,
    })
    const androidAnchor = projectSelectionGeometry(range, iframe, containerRect, {
      isAndroid: true,
      safeTopPx: 20,
    })

    expect(desktopAnchor?.preferredPlacement).toBe("bottom")
    expect(desktopAnchor?.anchorX).toBe(300)
    expect(desktopAnchor?.bottomY).toBe(55 + 10)

    expect(androidAnchor?.preferredPlacement).toBe("bottom")
    expect(androidAnchor?.bottomY).toBe(55 + 10 + 28)
    expect(androidAnchor!.bottomY - desktopAnchor!.bottomY).toBe(28)
  })

  it("discards off-screen multi-column phantom rects and clamps edge toolbars while preserving arrow alignment", () => {
    const containerRect = makeRect(0, 0, 1000, 800)
    const iframe = document.createElement("iframe")
    Object.defineProperty(iframe, "offsetWidth", { value: 3000 })
    Object.defineProperty(iframe, "offsetHeight", { value: 800 })
    // Page 2 visible at host x=[0..1000]; page 1 at [-1000..0], page 3 at [1000..2000]
    iframe.getBoundingClientRect = () => makeRect(-1000, 0, 3000, 800)

    const range = makeFakeRange([
      makeRect(900, 180, 0, 20), // zero-width phantom newline rect
      makeRect(200, 180, 300, 20), // off-screen prev column -> host [-800..-500]
      makeRect(1010, 180, 40, 20), // visible left edge -> host [10..50], center = 30
      makeRect(2400, 180, 200, 20), // off-screen next column -> host [1400..1600]
    ])

    const anchor = projectSelectionGeometry(range, iframe, containerRect)
    expect(anchor?.anchorX).toBe(30)

    const leftEdgePos = computeToolbarPosition(
      anchor!,
      { width: 160, height: 40 },
      { width: 1000, height: 800 },
      { left: 0, right: 0 }
    )
    expect(leftEdgePos.left).toBe(12)
    expect(leftEdgePos.arrowOffsetX).toBe(18) // 30 - 12

    const rightAnchor = {
      preferredPlacement: "top" as const,
      anchorX: 985,
      topY: 200,
      bottomY: 240,
    }
    const rightEdgePos = computeToolbarPosition(
      rightAnchor,
      { width: 160, height: 40 },
      { width: 1000, height: 800 }
    )
    expect(rightEdgePos.left).toBe(1000 - 160 - 12)
    expect(rightEdgePos.arrowOffsetX).toBe(
      160 - 14 // clamped to max arrow position inside toolbar while pointing toward edge
    )
  })

  it("requires full 84px below on Android before choosing bottom and avoids double-counting inset containers", () => {
    const iframe = document.createElement("iframe")
    Object.defineProperty(iframe, "offsetWidth", { value: 400 })
    Object.defineProperty(iframe, "offsetHeight", { value: 760 })
    iframe.getBoundingClientRect = () => makeRect(0, 40, 400, 760)

    // Container already starts below status bar (top: 40) and ends at bottom: 800
    // Range occupies local top=20..bottom=690 -> host y=60..730
    // spaceAbove = 60 - (40 + 0 effectiveSafeTop) = 20 (< 56), spaceBelow = 800 - 730 - 44 = 26 (< 84)
    // Wait: make spaceBelow = 60 (between 44 and 83) so Android MUST choose inside-top!
    const containerRect = makeRect(0, 40, 400, 760)
    // host bottom = 740 -> spaceBelow = 800 - 740 = 60px when safeBottomPx=0 (or safeBottom already inset with viewportHeightPx=844)
    const rangeAndroidMidSpace = makeFakeRange([makeRect(50, 20, 120, 680)])
    const androidMidAnchor = projectSelectionGeometry(
      rangeAndroidMidSpace,
      iframe,
      containerRect,
      {
        isAndroid: true,
        safeTopPx: 40,
        safeBottomPx: 44,
        viewportHeightPx: 844,
      }
    )
    expect(androidMidAnchor?.preferredPlacement).toBe("inside-top")

    // With container already inset at top=40 and range at host top=98 (spaceAbove=58 >= 56),
    // safeTopPx=40 must NOT be subtracted a second time.
    const rangeAboveEnough = makeFakeRange([makeRect(50, 58, 120, 20)])
    const topAnchor = projectSelectionGeometry(
      rangeAboveEnough,
      iframe,
      containerRect,
      {
        isAndroid: true,
        safeTopPx: 40,
        safeBottomPx: 44,
        viewportHeightPx: 844,
      }
    )
    expect(topAnchor?.preferredPlacement).toBe("top")
  })

  it("avoids double-counting 4-edge safeInsets when viewportBounds is already inset from windowViewport", () => {
    const anchor = {
      preferredPlacement: "top" as const,
      anchorX: 390,
      topY: 45,
      bottomY: 85,
    }
    // Container is already inset by (left=16, top=40, right=16, bottom=44) inside a 400x844 window.
    const insetPos = computeToolbarPosition(
      anchor,
      { width: 120, height: 40 },
      { left: 16, top: 40, width: 368, height: 760 },
      { left: 16, top: 40, right: 16, bottom: 44 },
      { width: 400, height: 844 }
    )
    // Edge margin = 12px from the already-inset container bounds (not 16 + 16 + 12).
    expect(insetPos.left).toBe(16 + 368 - 120 - 12)
    expect(insetPos.top).toBe(40 + 12)

    // Full-bleed container (0, 0, 400, 844) still applies full safeInsets + 12px margin.
    const fullBleedPos = computeToolbarPosition(
      anchor,
      { width: 120, height: 40 },
      { left: 0, top: 0, width: 400, height: 844 },
      { left: 16, top: 40, right: 16, bottom: 44 },
      { width: 400, height: 844 }
    )
    expect(fullBleedPos.left).toBe(400 - 16 - 120 - 12)
    expect(fullBleedPos.top).toBe(40 + 12)
  })
})
