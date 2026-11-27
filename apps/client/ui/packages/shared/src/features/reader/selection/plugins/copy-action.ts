import { createElement } from "react"
import { Copy } from "lucide-react"
import type { SelectionActionPlugin } from "../../contracts/selection"

export async function writeTextToClipboard(text: string): Promise<void> {
  if (
    typeof navigator !== "undefined" &&
    navigator.clipboard &&
    typeof navigator.clipboard.writeText === "function"
  ) {
    try {
      await navigator.clipboard.writeText(text)
      return
    } catch {
      // Fall through to hidden textarea fallback when WebView focus restricts clipboard API.
    }
  }

  if (typeof document === "undefined") {
    throw new Error("Clipboard API is unavailable in this environment.")
  }

  const textarea = document.createElement("textarea")
  textarea.value = text
  textarea.setAttribute("readonly", "")
  textarea.style.position = "fixed"
  textarea.style.left = "-9999px"
  textarea.style.top = "0"
  textarea.style.opacity = "0"

  const activeBefore = document.activeElement as HTMLElement | null
  document.body.appendChild(textarea)
  try {
    textarea.select()
    textarea.setSelectionRange(0, textarea.value.length)
    const ok = document.execCommand("copy")
    if (!ok) {
      throw new Error("document.execCommand('copy') returned false")
    }
  } finally {
    textarea.remove()
    activeBefore?.focus?.({ preventScroll: true })
  }
}

export const copyActionPlugin: SelectionActionPlugin = {
  id: "builtin.copy",
  group: "clipboard",
  order: 10,
  labelKey: "local.reader.selection.copy",
  shortcutHint: "Ctrl+C",
  renderIcon: () =>
    createElement(Copy, { className: "size-4", "aria-hidden": true }),
  isAvailable: (ctx) => ctx.text.trim().length > 0,
  execute: async (ctx) => {
    await writeTextToClipboard(ctx.text)
    return { outcome: "dismiss" }
  },
}
