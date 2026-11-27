import type { SelectionAnchorGeometry } from "../contracts/selection"

const PREFERRED_TOP_CLEARANCE_PX = 56
const ANDROID_DROPLET_CLEARANCE_PX = 28
const VERTICAL_GAP_PX = 10
const EDGE_MARGIN_PX = 12
const INSIDE_TOP_OFFSET_PX = 16
const SAME_LINE_TOLERANCE_PX = 4

export interface SelectionProjectionOptions {
  readonly isAndroid?: boolean | undefined
  readonly safeTopPx?: number | undefined
  readonly safeBottomPx?: number | undefined
  readonly viewportHeightPx?: number | undefined
}

export interface SafeViewportInsets {
  readonly top?: number | undefined
  readonly bottom?: number | undefined
  readonly left?: number | undefined
  readonly right?: number | undefined
}

export interface ToolbarPositionResult {
  readonly left: number
  readonly top: number
  readonly placement: "top" | "bottom" | "inside-top"
  readonly arrowOffsetX: number
}

interface ProjectedRect {
  readonly left: number
  readonly top: number
  readonly right: number
  readonly bottom: number
}

function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  if (max < min) return min
  return Math.min(max, Math.max(min, value))
}

function getHorizontalSpanCenter(rects: readonly ProjectedRect[]): number {
  let minLeft = Infinity
  let maxRight = -Infinity
  for (const rect of rects) {
    if (rect.left < minLeft) minLeft = rect.left
    if (rect.right > maxRight) maxRight = rect.right
  }
  return (minLeft + maxRight) / 2
}

export function projectSelectionGeometry(
  range: Range,
  iframe: HTMLIFrameElement | null | undefined,
  containerRect: Pick<DOMRectReadOnly, "left" | "top" | "width" | "height">,
  options: SelectionProjectionOptions = {}
): SelectionAnchorGeometry | null {
  let scaleX = 1
  let scaleY = 1
  let offsetX = 0
  let offsetY = 0

  if (iframe) {
    const iframeRect = iframe.getBoundingClientRect()
    const baseWidth =
      iframe.offsetWidth > 0 ? iframe.offsetWidth : iframeRect.width
    const baseHeight =
      iframe.offsetHeight > 0 ? iframe.offsetHeight : iframeRect.height
    scaleX = baseWidth > 0 ? iframeRect.width / baseWidth : 1
    scaleY = baseHeight > 0 ? iframeRect.height / baseHeight : 1
    offsetX = iframeRect.left
    offsetY = iframeRect.top
  }

  const rawRects = Array.from(range.getClientRects())
  const candidateRects =
    rawRects.length > 0 ? rawRects : [range.getBoundingClientRect()]

  const cLeft = containerRect.left
  const cTop = containerRect.top
  const cRight = containerRect.left + containerRect.width
  const cBottom = containerRect.top + containerRect.height

  const visibleRects: ProjectedRect[] = []
  for (const rect of candidateRects) {
    if (rect.width <= 0 || rect.height <= 0) continue

    const left = rect.left * scaleX + offsetX
    const top = rect.top * scaleY + offsetY
    const right = left + rect.width * scaleX
    const bottom = top + rect.height * scaleY

    // Discard off-screen multi-column phantom rects outside the visible viewport.
    if (
      right <= cLeft + 1 ||
      left >= cRight - 1 ||
      bottom <= cTop + 1 ||
      top >= cBottom - 1
    ) {
      continue
    }

    visibleRects.push({
      left: Math.max(cLeft, left),
      top: Math.max(cTop, top),
      right: Math.min(cRight, right),
      bottom: Math.min(cBottom, bottom),
    })
  }

  if (visibleRects.length === 0) {
    return null
  }

  let minTop = Infinity
  let maxBottom = -Infinity
  for (const rect of visibleRects) {
    if (rect.top < minTop) minTop = rect.top
    if (rect.bottom > maxBottom) maxBottom = rect.bottom
  }

  const firstRowRects = visibleRects.filter(
    (rect) => Math.abs(rect.top - minTop) <= SAME_LINE_TOLERANCE_PX
  )
  const lastRowRects = visibleRects.filter(
    (rect) => Math.abs(rect.bottom - maxBottom) <= SAME_LINE_TOLERANCE_PX
  )

  const rawSafeTopPx = Math.max(0, options.safeTopPx ?? 0)
  const rawSafeBottomPx = Math.max(0, options.safeBottomPx ?? 0)
  const effectiveSafeTopPx = cTop >= rawSafeTopPx ? 0 : rawSafeTopPx - cTop
  const viewportHeight =
    options.viewportHeightPx !== undefined &&
    Number.isFinite(options.viewportHeightPx)
      ? options.viewportHeightPx
      : cBottom
  const alreadyInsetBottom = Math.max(0, viewportHeight - cBottom)
  const effectiveSafeBottomPx = Math.max(0, rawSafeBottomPx - alreadyInsetBottom)
  const isAndroid = options.isAndroid === true

  const spaceAbove = minTop - (cTop + effectiveSafeTopPx)
  const spaceBelow = cBottom - effectiveSafeBottomPx - maxBottom
  const bottomOffset =
    VERTICAL_GAP_PX + (isAndroid ? ANDROID_DROPLET_CLEARANCE_PX : 0)

  const topY = minTop - VERTICAL_GAP_PX
  const bottomY = maxBottom + bottomOffset

  if (spaceAbove >= PREFERRED_TOP_CLEARANCE_PX) {
    return {
      preferredPlacement: "top",
      anchorX: getHorizontalSpanCenter(firstRowRects),
      topY,
      bottomY,
    }
  }

  const requiredBottomSpace =
    PREFERRED_TOP_CLEARANCE_PX + (isAndroid ? ANDROID_DROPLET_CLEARANCE_PX : 0)
  if (spaceBelow >= requiredBottomSpace) {
    return {
      preferredPlacement: "bottom",
      anchorX: getHorizontalSpanCenter(lastRowRects),
      topY,
      bottomY,
    }
  }

  return {
    preferredPlacement: "inside-top",
    anchorX: getHorizontalSpanCenter(visibleRects),
    topY: cTop + effectiveSafeTopPx + INSIDE_TOP_OFFSET_PX,
    bottomY,
  }
}

export function computeToolbarPosition(
  anchor: SelectionAnchorGeometry,
  toolbarSize: { readonly width: number; readonly height: number },
  viewportBounds: {
    readonly left?: number | undefined
    readonly top?: number | undefined
    readonly width: number
    readonly height: number
  },
  safeInsets: SafeViewportInsets = {},
  windowViewport?: { readonly width: number; readonly height: number } | undefined
): ToolbarPositionResult {
  const vLeft = viewportBounds.left ?? 0
  const vTop = viewportBounds.top ?? 0
  const vRight = vLeft + viewportBounds.width
  const vBottom = vTop + viewportBounds.height

  const rawSafeLeft = Math.max(0, safeInsets.left ?? 0)
  const rawSafeRight = Math.max(0, safeInsets.right ?? 0)
  const rawSafeTop = Math.max(0, safeInsets.top ?? 0)
  const rawSafeBottom = Math.max(0, safeInsets.bottom ?? 0)

  const safeLeft = Math.max(0, rawSafeLeft - Math.max(0, vLeft))
  const safeTop = Math.max(0, rawSafeTop - Math.max(0, vTop))
  const alreadyInsetRight = windowViewport
    ? Math.max(0, windowViewport.width - vRight)
    : 0
  const alreadyInsetBottom = windowViewport
    ? Math.max(0, windowViewport.height - vBottom)
    : 0
  const safeRight = Math.max(0, rawSafeRight - alreadyInsetRight)
  const safeBottom = Math.max(0, rawSafeBottom - alreadyInsetBottom)

  const minLeft = vLeft + safeLeft + EDGE_MARGIN_PX
  const maxLeft = Math.max(
    minLeft,
    vRight - safeRight - toolbarSize.width - EDGE_MARGIN_PX
  )
  const desiredLeft = anchor.anchorX - toolbarSize.width / 2
  const left = clamp(desiredLeft, minLeft, maxLeft)

  const minTop = vTop + safeTop + EDGE_MARGIN_PX
  const maxTop = Math.max(
    minTop,
    vBottom - safeBottom - toolbarSize.height - EDGE_MARGIN_PX
  )

  let rawTop: number
  if (anchor.preferredPlacement === "top") {
    rawTop = anchor.topY - toolbarSize.height
  } else if (anchor.preferredPlacement === "bottom") {
    rawTop = anchor.bottomY
  } else {
    rawTop = anchor.topY
  }

  const top = clamp(rawTop, minTop, maxTop)
  const arrowMargin = Math.min(14, Math.max(4, toolbarSize.width / 4))
  const arrowOffsetX = clamp(
    anchor.anchorX - left,
    arrowMargin,
    Math.max(arrowMargin, toolbarSize.width - arrowMargin)
  )

  return {
    left,
    top,
    placement: anchor.preferredPlacement,
    arrowOffsetX,
  }
}
