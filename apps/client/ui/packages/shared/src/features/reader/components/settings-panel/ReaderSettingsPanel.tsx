import { useId, useState } from "react"
import { ChevronRight, RotateCcw } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { Separator } from "@shiro/ui-shared/components/ui/separator"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { DEFAULT_APP_THEME } from "@shiro/ui-shared/lib/theme"
import { cn } from "cn"
import type { EngineCapabilities } from "../../contracts/engine"
import type {
  CustomFontMeta,
  ReaderColumnCount,
  ReaderFlowMode,
  ReaderFontChoice,
  ReaderProfile,
  ReaderTextAlign,
} from "../../contracts/settings"
import {
  countBookOverrides,
  isFieldOverridden,
} from "../../settings/cascade"
import { READER_SETTING_BOUNDS } from "../../settings/defaults"
import {
  clearBookReaderOverrideField,
  deleteCustomFont,
  pickAndImportCustomFont,
  resetBookReaderOverrides,
  resetGlobalReaderSettings,
  setBookReaderOverrideField,
  updateGlobalReaderField,
  useEffectiveReaderProfile,
  useSystemFonts,
} from "../../settings/reader-settings-store"
import { FontPickerModal } from "./FontPickerModal"
import { SurfacePresetPicker } from "./SurfacePresetPicker"

export type SettingsScope = "global" | "book"

export interface ReaderSettingsPanelProps {
  readonly bookHash: string
  readonly capabilities: EngineCapabilities
  readonly isMobile?: boolean | undefined
  readonly surfaceCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
  readonly className?: string | undefined
}

function getFontChoiceLabel(
  choice: ReaderFontChoice,
  customFonts: readonly CustomFontMeta[],
  t: (key: Parameters<ReturnType<typeof useTranslation>["t"]>[0]) => string
): string {
  switch (choice.kind) {
    case "publisher":
      return t("local.reader.settings.font.publisher")
    case "preset":
      return t(`local.reader.settings.font.preset_${choice.preset}`)
    case "system":
      return choice.family
    case "custom":
      return (
        customFonts.find((f) => f.fontId === choice.fontId)?.familyName ??
        t("local.reader.settings.font.publisher")
      )
  }
}

export function ReaderSettingsPanel({
  bookHash,
  capabilities,
  isMobile = false,
  surfaceCssVars,
  isDarkSurface,
  className,
}: ReaderSettingsPanelProps) {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const appTheme = runtime?.bootstrap?.theme ?? DEFAULT_APP_THEME

  const idPrefix = useId()
  const [scope, setScope] = useState<SettingsScope>("global")
  const [fontModalOpen, setFontModalOpen] = useState(false)

  const { snapshot, bookPatch, effectiveProfile } = useEffectiveReaderProfile(
    bookHash,
    isMobile
  )
  const { systemFonts } = useSystemFonts(
    fontModalOpen && snapshot.canEnumerateSystemFonts
  )

  const displayedProfile = scope === "global" ? snapshot.global : effectiveProfile
  const isScrolledFlow =
    capabilities.supportsFlowSwitch && displayedProfile.behavior.flow === "scrolled"
  const overrideCount = countBookOverrides(bookPatch)
  const reflowDisabled = !capabilities.supportsReflowTypography

  const updateField = <
    G extends keyof ReaderProfile,
    K extends keyof ReaderProfile[G],
  >(
    group: G,
    key: K,
    value: ReaderProfile[G][K]
  ) => {
    if (scope === "global") {
      updateGlobalReaderField(group, key, value)
    } else {
      setBookReaderOverrideField(bookHash, group, key, value)
    }
  }

  const renderFieldHeader = <G extends keyof ReaderProfile>(
    group: G,
    key: keyof ReaderProfile[G],
    label: string,
    valueText?: string,
    htmlFor?: string
  ) => {
    const overridden =
      scope === "book" && isFieldOverridden(bookPatch, group, key)
    return (
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="flex items-center gap-1.5 font-medium">
          {htmlFor ? (
            <label htmlFor={htmlFor}>{label}</label>
          ) : (
            <span>{label}</span>
          )}
          {overridden && (
            <span
              data-testid={`override-dot-${String(group)}-${String(key)}`}
              className="inline-block size-1.5 rounded-full bg-primary"
              aria-hidden="true"
            />
          )}
        </span>
        <div className="flex items-center gap-1.5">
          {valueText && (
            <span className="tabular-nums text-muted-foreground">
              {valueText}
            </span>
          )}
          {overridden && (
            <button
              type="button"
              aria-label={`${t("local.reader.settings.reset_field")}: ${label}`}
              onClick={() => clearBookReaderOverrideField(bookHash, group, key)}
              className="rounded px-1 py-0.5 text-[11px] text-primary hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {t("local.reader.settings.reset_field")}
            </button>
          )}
        </div>
      </div>
    )
  }

  return (
    <div
      className={cn(
        "flex flex-col gap-4 overflow-y-auto p-3 text-xs",
        className
      )}
    >
      <div className="flex flex-col gap-2">
        <div
          role="tablist"
          aria-label={t("local.reader.settings.title")}
          className="grid grid-cols-2 rounded-lg bg-muted p-1"
        >
          <button
            type="button"
            role="tab"
            aria-selected={scope === "global"}
            onClick={() => setScope("global")}
            className={cn(
              "rounded-md px-2 py-1.5 text-xs font-medium transition-colors",
              scope === "global"
                ? "bg-background text-foreground shadow-xs"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            {t("local.reader.settings.scope.global")}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={scope === "book"}
            onClick={() => setScope("book")}
            className={cn(
              "rounded-md px-2 py-1.5 text-xs font-medium transition-colors",
              scope === "book"
                ? "bg-background text-foreground shadow-xs"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            {t("local.reader.settings.scope.book")}
          </button>
        </div>

        {scope === "global" && (
          <div className="flex items-center justify-end">
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={() => {
                void resetGlobalReaderSettings(isMobile).catch(() => {})
              }}
            >
              <RotateCcw className="size-3" aria-hidden="true" />
              {t("local.reader.settings.reset_global")}
            </Button>
          </div>
        )}

        {scope === "book" && overrideCount > 0 && (
          <div className="flex items-center justify-between rounded-lg border px-2.5 py-1.5 text-xs">
            <span className="text-muted-foreground">
              {t("local.reader.settings.override_count").replace(
                "{count}",
                String(overrideCount)
              )}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => {
                void resetBookReaderOverrides(bookHash).catch(() => {})
              }}
            >
              <RotateCcw className="size-3" aria-hidden="true" />
              {t("local.reader.settings.reset_book")}
            </Button>
          </div>
        )}

        {reflowDisabled && (
          <p className="rounded-lg border border-border bg-muted/40 px-2.5 py-2 text-[11px] text-muted-foreground">
            {t("local.reader.settings.fixed_layout_notice")}
          </p>
        )}
      </div>

      <section className="flex flex-col gap-2">
        {renderFieldHeader(
          "appearance",
          "surface",
          t("local.reader.settings.group.appearance")
        )}
        <SurfacePresetPicker
          value={displayedProfile.appearance.surface}
          appTheme={appTheme}
          onChange={(surface) => updateField("appearance", "surface", surface)}
        />
      </section>

      <Separator />

      <section className="flex flex-col gap-3">
        <span className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
          {t("local.reader.settings.group.typography")}
        </span>

        <div className="flex flex-col gap-1.5">
          {renderFieldHeader(
            "typography",
            "font",
            t("local.reader.settings.font.label")
          )}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={reflowDisabled}
            onClick={() => setFontModalOpen(true)}
            className="justify-between text-xs"
          >
            <span className="truncate">
              {getFontChoiceLabel(
                displayedProfile.typography.font,
                snapshot.customFonts,
                t
              )}
            </span>
            <ChevronRight className="size-3.5 shrink-0" aria-hidden="true" />
          </Button>
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "typography",
            "fontSizePx",
            t("local.reader.settings.font_size"),
            `${displayedProfile.typography.fontSizePx}px`,
            `${idPrefix}-font-size`
          )}
          <input
            id={`${idPrefix}-font-size`}
            type="range"
            aria-label={t("local.reader.settings.font_size")}
            min={READER_SETTING_BOUNDS.fontSizePx.min}
            max={READER_SETTING_BOUNDS.fontSizePx.max}
            step={READER_SETTING_BOUNDS.fontSizePx.step}
            disabled={reflowDisabled}
            value={displayedProfile.typography.fontSizePx}
            onChange={(e) =>
              updateField("typography", "fontSizePx", Number(e.target.value))
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "typography",
            "lineHeight",
            t("local.reader.settings.line_height"),
            displayedProfile.typography.lineHeight.toFixed(2),
            `${idPrefix}-line-height`
          )}
          <input
            id={`${idPrefix}-line-height`}
            type="range"
            aria-label={t("local.reader.settings.line_height")}
            min={READER_SETTING_BOUNDS.lineHeight.min}
            max={READER_SETTING_BOUNDS.lineHeight.max}
            step={READER_SETTING_BOUNDS.lineHeight.step}
            disabled={reflowDisabled}
            value={displayedProfile.typography.lineHeight}
            onChange={(e) =>
              updateField("typography", "lineHeight", Number(e.target.value))
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1.5">
          {renderFieldHeader(
            "typography",
            "fontWeight",
            t("local.reader.settings.font_weight"),
            displayedProfile.typography.fontWeight === "auto"
              ? t("local.reader.settings.font_weight_auto")
              : String(displayedProfile.typography.fontWeight)
          )}
          <div className="grid grid-cols-6 gap-1">
            {(["auto", 300, 400, 500, 600, 700] as const).map((weight) => (
              <button
                key={String(weight)}
                type="button"
                aria-pressed={displayedProfile.typography.fontWeight === weight}
                disabled={reflowDisabled}
                onClick={() => updateField("typography", "fontWeight", weight)}
                className={cn(
                  "rounded-md border py-1 text-[11px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                  displayedProfile.typography.fontWeight === weight
                    ? "border-primary bg-primary/15 font-semibold"
                    : "border-border hover:bg-muted"
                )}
              >
                {weight === "auto"
                  ? t("local.reader.settings.font_weight_auto")
                  : weight}
              </button>
            ))}
          </div>
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "typography",
            "letterSpacingEm",
            t("local.reader.settings.letter_spacing"),
            `${displayedProfile.typography.letterSpacingEm.toFixed(2)}em`,
            `${idPrefix}-letter-spacing`
          )}
          <input
            id={`${idPrefix}-letter-spacing`}
            type="range"
            aria-label={t("local.reader.settings.letter_spacing")}
            min={READER_SETTING_BOUNDS.letterSpacingEm.min}
            max={READER_SETTING_BOUNDS.letterSpacingEm.max}
            step={READER_SETTING_BOUNDS.letterSpacingEm.step}
            disabled={reflowDisabled}
            value={displayedProfile.typography.letterSpacingEm}
            onChange={(e) =>
              updateField(
                "typography",
                "letterSpacingEm",
                Number(e.target.value)
              )
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "typography",
            "wordSpacingEm",
            t("local.reader.settings.word_spacing"),
            `${displayedProfile.typography.wordSpacingEm.toFixed(2)}em`,
            `${idPrefix}-word-spacing`
          )}
          <input
            id={`${idPrefix}-word-spacing`}
            type="range"
            aria-label={t("local.reader.settings.word_spacing")}
            min={READER_SETTING_BOUNDS.wordSpacingEm.min}
            max={READER_SETTING_BOUNDS.wordSpacingEm.max}
            step={READER_SETTING_BOUNDS.wordSpacingEm.step}
            disabled={reflowDisabled}
            value={displayedProfile.typography.wordSpacingEm}
            onChange={(e) =>
              updateField("typography", "wordSpacingEm", Number(e.target.value))
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1.5">
          {renderFieldHeader(
            "typography",
            "textAlign",
            t("local.reader.settings.text_align")
          )}
          <div className="grid grid-cols-3 gap-1">
            {(["auto", "start", "justify"] as const satisfies readonly ReaderTextAlign[]).map(
              (align) => (
                <button
                  key={align}
                  type="button"
                  aria-pressed={displayedProfile.typography.textAlign === align}
                  disabled={reflowDisabled}
                  onClick={() => updateField("typography", "textAlign", align)}
                  className={cn(
                    "rounded-md border py-1 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                    displayedProfile.typography.textAlign === align
                      ? "border-primary bg-primary/15 font-semibold"
                      : "border-border hover:bg-muted"
                  )}
                >
                  {t(`local.reader.settings.text_align_${align}`)}
                </button>
              )
            )}
          </div>
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "typography",
            "hyphenation",
            t("local.reader.settings.hyphenation"),
            undefined,
            `${idPrefix}-hyphenation`
          )}
          <input
            id={`${idPrefix}-hyphenation`}
            type="checkbox"
            aria-label={t("local.reader.settings.hyphenation")}
            disabled={reflowDisabled}
            checked={displayedProfile.typography.hyphenation === "auto"}
            onChange={(e) =>
              updateField(
                "typography",
                "hyphenation",
                e.target.checked ? "auto" : "none"
              )
            }
          />
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "typography",
            "overridePublisherStyles",
            t("local.reader.settings.override_publisher"),
            undefined,
            `${idPrefix}-override-publisher`
          )}
          <input
            id={`${idPrefix}-override-publisher`}
            type="checkbox"
            aria-label={t("local.reader.settings.override_publisher")}
            disabled={!capabilities.supportsPublisherStyleOverride}
            checked={displayedProfile.typography.overridePublisherStyles}
            onChange={(e) =>
              updateField(
                "typography",
                "overridePublisherStyles",
                e.target.checked
              )
            }
          />
        </div>
      </section>

      <Separator />

      <section className="flex flex-col gap-3">
        <span className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
          {t("local.reader.settings.group.layout")}
        </span>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "layout",
            "paragraphSpacingEm",
            t("local.reader.settings.paragraph_spacing"),
            `${displayedProfile.layout.paragraphSpacingEm.toFixed(2)}em`,
            `${idPrefix}-paragraph-spacing`
          )}
          <input
            id={`${idPrefix}-paragraph-spacing`}
            type="range"
            aria-label={t("local.reader.settings.paragraph_spacing")}
            min={READER_SETTING_BOUNDS.paragraphSpacingEm.min}
            max={READER_SETTING_BOUNDS.paragraphSpacingEm.max}
            step={READER_SETTING_BOUNDS.paragraphSpacingEm.step}
            disabled={reflowDisabled}
            value={displayedProfile.layout.paragraphSpacingEm}
            onChange={(e) =>
              updateField(
                "layout",
                "paragraphSpacingEm",
                Number(e.target.value)
              )
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1.5">
          {renderFieldHeader(
            "layout",
            "paragraphIndentEm",
            t("local.reader.settings.paragraph_indent"),
            displayedProfile.layout.paragraphIndentEm === "auto"
              ? t("local.reader.settings.paragraph_indent_auto")
              : `${displayedProfile.layout.paragraphIndentEm.toFixed(1)}em`,
            `${idPrefix}-paragraph-indent`
          )}
          <div className="flex items-center gap-2">
            <button
              type="button"
              aria-pressed={displayedProfile.layout.paragraphIndentEm === "auto"}
              disabled={reflowDisabled}
              onClick={() =>
                updateField(
                  "layout",
                  "paragraphIndentEm",
                  displayedProfile.layout.paragraphIndentEm === "auto"
                    ? 2
                    : "auto"
                )
              }
              className={cn(
                "shrink-0 rounded-md border px-2 py-1 text-[11px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                displayedProfile.layout.paragraphIndentEm === "auto"
                  ? "border-primary bg-primary/15 font-semibold"
                  : "border-border hover:bg-muted"
              )}
            >
              {t("local.reader.settings.paragraph_indent_auto")}
            </button>
            <input
              id={`${idPrefix}-paragraph-indent`}
              type="range"
              aria-label={t("local.reader.settings.paragraph_indent")}
              min={READER_SETTING_BOUNDS.paragraphIndentEm.min}
              max={READER_SETTING_BOUNDS.paragraphIndentEm.max}
              step={READER_SETTING_BOUNDS.paragraphIndentEm.step}
              disabled={reflowDisabled}
              value={
                displayedProfile.layout.paragraphIndentEm === "auto"
                  ? 0
                  : displayedProfile.layout.paragraphIndentEm
              }
              onChange={(e) =>
                updateField(
                  "layout",
                  "paragraphIndentEm",
                  Number(e.target.value)
                )
              }
              className="min-w-0 flex-1 accent-primary disabled:opacity-50"
            />
          </div>
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "layout",
            "pageMarginInlinePx",
            t("local.reader.settings.margin_inline"),
            `${displayedProfile.layout.pageMarginInlinePx}px`,
            `${idPrefix}-margin-inline`
          )}
          <input
            id={`${idPrefix}-margin-inline`}
            type="range"
            aria-label={t("local.reader.settings.margin_inline")}
            min={READER_SETTING_BOUNDS.pageMarginInlinePx.min}
            max={READER_SETTING_BOUNDS.pageMarginInlinePx.max}
            step={READER_SETTING_BOUNDS.pageMarginInlinePx.step}
            disabled={reflowDisabled}
            value={displayedProfile.layout.pageMarginInlinePx}
            onChange={(e) =>
              updateField(
                "layout",
                "pageMarginInlinePx",
                Number(e.target.value)
              )
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "layout",
            "pageMarginBlockPx",
            t("local.reader.settings.margin_block"),
            `${displayedProfile.layout.pageMarginBlockPx}px`,
            `${idPrefix}-margin-block`
          )}
          <input
            id={`${idPrefix}-margin-block`}
            type="range"
            aria-label={t("local.reader.settings.margin_block")}
            min={READER_SETTING_BOUNDS.pageMarginBlockPx.min}
            max={READER_SETTING_BOUNDS.pageMarginBlockPx.max}
            step={READER_SETTING_BOUNDS.pageMarginBlockPx.step}
            disabled={reflowDisabled}
            value={displayedProfile.layout.pageMarginBlockPx}
            onChange={(e) =>
              updateField("layout", "pageMarginBlockPx", Number(e.target.value))
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        <div className="flex flex-col gap-1">
          {renderFieldHeader(
            "layout",
            "maxInlineSizePx",
            t("local.reader.settings.max_inline_size"),
            `${displayedProfile.layout.maxInlineSizePx}px`,
            `${idPrefix}-max-inline-size`
          )}
          <input
            id={`${idPrefix}-max-inline-size`}
            type="range"
            aria-label={t("local.reader.settings.max_inline_size")}
            min={READER_SETTING_BOUNDS.maxInlineSizePx.min}
            max={READER_SETTING_BOUNDS.maxInlineSizePx.max}
            step={READER_SETTING_BOUNDS.maxInlineSizePx.step}
            disabled={reflowDisabled}
            value={displayedProfile.layout.maxInlineSizePx}
            onChange={(e) =>
              updateField("layout", "maxInlineSizePx", Number(e.target.value))
            }
            className="accent-primary disabled:opacity-50"
          />
        </div>

        {!isMobile && (
          <>
            <div className="flex flex-col gap-1.5">
              {renderFieldHeader(
                "layout",
                "columnCount",
                t("local.reader.settings.column_count")
              )}
              <div className="grid grid-cols-3 gap-1">
                {(["auto", 1, 2] as const satisfies readonly ReaderColumnCount[]).map(
                  (col) => (
                    <button
                      key={String(col)}
                      type="button"
                      aria-pressed={displayedProfile.layout.columnCount === col}
                      disabled={!capabilities.supportsColumnSwitch || isScrolledFlow}
                      onClick={() => updateField("layout", "columnCount", col)}
                      className={cn(
                        "rounded-md border py-1 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                        displayedProfile.layout.columnCount === col
                          ? "border-primary bg-primary/15 font-semibold"
                          : "border-border hover:bg-muted"
                      )}
                    >
                      {t(`local.reader.settings.column_${col}`)}
                    </button>
                  )
                )}
              </div>
            </div>

            <div className="flex flex-col gap-1">
              {renderFieldHeader(
                "layout",
                "columnGapPercent",
                t("local.reader.settings.column_gap"),
                `${displayedProfile.layout.columnGapPercent}%`,
                `${idPrefix}-column-gap`
              )}
              <input
                id={`${idPrefix}-column-gap`}
                type="range"
                aria-label={t("local.reader.settings.column_gap")}
                min={READER_SETTING_BOUNDS.columnGapPercent.min}
                max={READER_SETTING_BOUNDS.columnGapPercent.max}
                step={READER_SETTING_BOUNDS.columnGapPercent.step}
                disabled={!capabilities.supportsColumnSwitch || isScrolledFlow}
                value={displayedProfile.layout.columnGapPercent}
                onChange={(e) =>
                  updateField(
                    "layout",
                    "columnGapPercent",
                    Number(e.target.value)
                  )
                }
                className="accent-primary disabled:opacity-50"
              />
            </div>
          </>
        )}
      </section>

      <Separator />

      <section className="flex flex-col gap-3">
        <span className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
          {t("local.reader.settings.group.behavior")}
        </span>

        <div className="flex flex-col gap-1.5">
          {renderFieldHeader(
            "behavior",
            "flow",
            t("local.reader.settings.flow")
          )}
          <div className="grid grid-cols-2 gap-1">
            {(["paginated", "scrolled"] as const satisfies readonly ReaderFlowMode[]).map(
              (flow) => (
                <button
                  key={flow}
                  type="button"
                  aria-pressed={displayedProfile.behavior.flow === flow}
                  disabled={!capabilities.supportsFlowSwitch}
                  onClick={() => updateField("behavior", "flow", flow)}
                  className={cn(
                    "rounded-md border py-1 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                    displayedProfile.behavior.flow === flow
                      ? "border-primary bg-primary/15 font-semibold"
                      : "border-border hover:bg-muted"
                  )}
                >
                  {t(`local.reader.settings.flow_${flow}`)}
                </button>
              )
            )}
          </div>
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "behavior",
            "animatedPageTurn",
            t("local.reader.settings.animated_page_turn"),
            undefined,
            `${idPrefix}-animated-page-turn`
          )}
          <input
            id={`${idPrefix}-animated-page-turn`}
            type="checkbox"
            aria-label={t("local.reader.settings.animated_page_turn")}
            disabled={isScrolledFlow || !capabilities.supportsFlowSwitch}
            checked={displayedProfile.behavior.animatedPageTurn}
            onChange={(e) =>
              updateField("behavior", "animatedPageTurn", e.target.checked)
            }
          />
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "behavior",
            "seamlessScroll",
            t("local.reader.settings.seamless_scroll"),
            undefined,
            `${idPrefix}-seamless-scroll`
          )}
          <input
            id={`${idPrefix}-seamless-scroll`}
            type="checkbox"
            aria-label={t("local.reader.settings.seamless_scroll")}
            disabled={!isScrolledFlow || !capabilities.supportsFlowSwitch}
            checked={displayedProfile.behavior.seamlessScroll}
            onChange={(e) =>
              updateField("behavior", "seamlessScroll", e.target.checked)
            }
          />
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "behavior",
            "tapZonesEnabled",
            t("local.reader.settings.tap_zones"),
            undefined,
            `${idPrefix}-tap-zones`
          )}
          <input
            id={`${idPrefix}-tap-zones`}
            type="checkbox"
            aria-label={t("local.reader.settings.tap_zones")}
            checked={displayedProfile.behavior.tapZonesEnabled}
            onChange={(e) =>
              updateField("behavior", "tapZonesEnabled", e.target.checked)
            }
          />
        </div>

        <div className="flex items-center justify-between gap-2">
          {renderFieldHeader(
            "behavior",
            "showPageNumber",
            t("local.reader.settings.show_page_number"),
            undefined,
            `${idPrefix}-show-page-number`
          )}
          <input
            id={`${idPrefix}-show-page-number`}
            type="checkbox"
            aria-label={t("local.reader.settings.show_page_number")}
            checked={displayedProfile.behavior.showPageNumber}
            onChange={(e) =>
              updateField("behavior", "showPageNumber", e.target.checked)
            }
          />
        </div>
      </section>

      <FontPickerModal
        open={fontModalOpen}
        onOpenChange={setFontModalOpen}
        value={displayedProfile.typography.font}
        customFonts={snapshot.customFonts}
        systemFonts={systemFonts}
        canEnumerateSystemFonts={snapshot.canEnumerateSystemFonts}
        surfaceCssVars={surfaceCssVars}
        isDarkSurface={isDarkSurface}
        onSelect={(choice) => updateField("typography", "font", choice)}
        onImportCustomFont={() => {
          void pickAndImportCustomFont().catch(() => {})
        }}
        onDeleteCustomFont={(fontId) => {
          void deleteCustomFont(fontId).catch(() => {})
        }}
      />
    </div>
  )
}
