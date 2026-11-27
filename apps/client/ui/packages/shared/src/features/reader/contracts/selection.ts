import type { ReactNode } from "react"
import type { LocalTranslationKey } from "@shiro/ui-shared/lib/i18n"
import type {
  BookFormat,
  EngineCapabilities,
  HighlightLocator,
} from "./engine"

export interface SelectionAnchorGeometry {
  readonly preferredPlacement: "top" | "bottom" | "inside-top"
  readonly anchorX: number
  readonly topY: number
  readonly bottomY: number
}

export type SelectionTextOrigin = "original" | "translation" | "mixed"

export interface SelectionContext {
  readonly tabId: string
  readonly bookId: string
  readonly bookHash: string
  readonly format: BookFormat
  readonly sectionIndex: number
  readonly locator: HighlightLocator | null
  readonly textOrigin: SelectionTextOrigin
  readonly text: string
  readonly anchorBlockOriginalText: string | null
  readonly anchor: SelectionAnchorGeometry
  clearSelection(): void
}

export type SelectionActionGroup =
  | "clipboard"
  | "annotation"
  | "lookup"
  | "ai"
  | "share"

export interface SelectionSubpanelControls {
  closeToolbar(): void
  collapseSubpanel(): void
}

export type SelectionActionResult =
  | { readonly outcome: "dismiss" }
  | { readonly outcome: "keep_open" }
  | { readonly outcome: "open_subpanel" }

export interface SelectionActionPlugin {
  readonly id: string
  readonly group: SelectionActionGroup
  readonly order: number
  readonly labelKey: LocalTranslationKey
  readonly renderIcon: () => ReactNode
  readonly shortcutHint?: string | undefined
  readonly isAvailable?:
    | ((ctx: SelectionContext, caps: EngineCapabilities) => boolean)
    | undefined
  execute(ctx: SelectionContext): Promise<SelectionActionResult>
  readonly renderSubpanel?:
    | ((ctx: SelectionContext, controls: SelectionSubpanelControls) => ReactNode)
    | undefined
}
