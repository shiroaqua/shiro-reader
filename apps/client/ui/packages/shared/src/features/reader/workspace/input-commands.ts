import type { ReaderFlowMode } from "../contracts/settings"

export type ReaderTapCommand =
  | "go_left"
  | "go_right"
  | "toggle_chrome"
  | "none"

export type ReaderKeyCommand =
  | "go_left"
  | "go_right"
  | "go_prev"
  | "go_next"
  | "go_start"
  | "go_end"
  | "toggle_zen"
  | "escape"
  | "copy"
  | "none"

export function resolveViewportTapCommand(
  normX: number,
  flow: ReaderFlowMode,
  tapZonesEnabled: boolean
): ReaderTapCommand {
  if (flow === "scrolled" || !tapZonesEnabled) {
    return "toggle_chrome"
  }
  if (!Number.isFinite(normX)) {
    return "toggle_chrome"
  }
  if (normX < 0.33) {
    return "go_left"
  }
  if (normX > 0.67) {
    return "go_right"
  }
  return "toggle_chrome"
}

export function resolveReaderKeyCommand(
  event: Pick<
    KeyboardEvent,
    "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey"
  >
): ReaderKeyCommand {
  const hasModifier = event.ctrlKey || event.metaKey || event.altKey
  if (
    (event.ctrlKey || event.metaKey) &&
    !event.altKey &&
    event.key.toLowerCase() === "c"
  ) {
    return "copy"
  }
  if (hasModifier) {
    return "none"
  }
  if (event.key === "F11") {
    return "toggle_zen"
  }
  if (event.key === "Escape") {
    return "escape"
  }
  if (event.key === " ") {
    return event.shiftKey ? "go_prev" : "go_next"
  }
  if (event.shiftKey) {
    return "none"
  }
  switch (event.key) {
    case "ArrowLeft":
      return "go_left"
    case "ArrowRight":
      return "go_right"
    case "PageUp":
      return "go_prev"
    case "PageDown":
      return "go_next"
    case "Home":
      return "go_start"
    case "End":
      return "go_end"
    default:
      return "none"
  }
}
